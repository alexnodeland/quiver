/*
 * docs/viz/voct.js — "The Geometry of Pitch"
 *
 * The V/Oct standard, made draggable. HERO: a linear-Hz plot of
 * f(V) = 261.63 · 2^V over a voltage ruler from −2 V to +3 V, tied together by
 * a marker you drag — the exponential blowup is the point. A "quantize to
 * semitones" toggle does exactly what Quiver's Quantizer module does (snap to
 * multiples of 1/12 V), showing the raw CV as a ghost. A PIANO strip rides on
 * the ruler (C2–C7): every key sits on an exact multiple of 1/12 V, so tapping
 * a key snaps the marker to that note's voltage — the volts↔notes bijection,
 * made tactile. Below, a MINI view replots the same curve on a log-frequency
 * axis, where it is a straight line.
 *
 * Audio: "hear it" loops a sine at the marker's frequency; "play interval"
 * plays the marker note, then the note exactly +1.0 V above (×2 Hz), then both
 * (one pre-rendered buffer — no setTimeout chain, so re-taps can't stack).
 * "detune demo" loops the RAW pitch and its QUANTIZED ghost together (half
 * amplitude each) and plots the summed waveform's envelope over ~2 s — beats
 * at |f_raw − f_quant| Hz, the audible form of the cents readout.
 *
 * No continuous animation loop — redraws on input and theme change only, and
 * drag-driven redraws are rAF-coalesced (QV.coalesce) while audio retunes
 * while dragging are debounced (QV.debounce). Canvases use touch-action:
 * pan-y so horizontal drags belong to the widget and vertical swipes still
 * scroll the page. Self-contained ES5 IIFE. Consumes window.QuiverViz.
 */
(function () {
  "use strict";
  if (typeof window === "undefined" || !window.QuiverViz) return;
  var QuiverViz = window.QuiverViz;

  QuiverViz.register("voct", function (root, QV) {
    var dsp = QV.dsp;
    var MONO = 'ui-monospace, "Source Code Pro", SFMono-Regular, Menlo, monospace';

    // ---- Constants -----------------------------------------------------------
    var V_MIN = -2, V_MAX = 3;
    var S_MIN = V_MIN * 12, S_MAX = V_MAX * 12; // semitone indices (C2..C7)
    var F_MAX = dsp.voctToHz(V_MAX) * 1.07; // headroom above C7 (2093 Hz)
    var SR = 44100;
    var BEAT_WINDOW_SEC = 2.0; // envelope plot + loop length for the detune demo
    var BLACK_SEMIS = { 1: 1, 3: 1, 6: 1, 8: 1, 10: 1 }; // C#, D#, F#, G#, A#
    var BLACK_W = 0.64; // black key width, in semitone widths
    var BLACK_H = 0.6;  // black key height, in piano-strip heights

    // ---- State ---------------------------------------------------------------
    var rawV = 0.75;        // the un-quantized pitch CV (what the cable carries)
    var quantize = false;   // the Quantizer module: snap to multiples of 1/12 V
    var hearing = false;    // looped sine at the marker's frequency
    var beating = false;    // detune demo: raw + quantized ghost together

    // Chromatic quantization — exactly Quantizer::quantize for Scale::Chromatic
    // (src/modules/utilities.rs): nearest multiple of 1/12 V.
    function quantizeV(v) { return Math.round(v * 12) / 12; }
    function effV() { return quantize ? quantizeV(rawV) : rawV; }
    function currentFreq() { return dsp.voctToHz(effV()); }
    // Cents deviation from the nearest equal-tempered note.
    function cents(v) { var s = v * 12; return (s - Math.round(s)) * 100; }
    // Predicted beat rate of the detune demo: |f_raw − f_quantized|.
    function beatHz() {
      return Math.abs(dsp.voctToHz(rawV) - dsp.voctToHz(quantizeV(rawV)));
    }

    function clampV(v) {
      if (v < V_MIN) v = V_MIN;
      if (v > V_MAX) v = V_MAX;
      return v;
    }

    function isBlackKey(s) { return !!BLACK_SEMIS[((s % 12) + 12) % 12]; }

    // ---- Formatting ----------------------------------------------------------
    function fmtVolts(v) { return (v >= 0 ? "+" : "") + v.toFixed(3) + " V"; }
    function fmtFreq(f) { return (f >= 1000 ? f.toFixed(1) : f.toFixed(2)) + " Hz"; }
    function fmtCents(c) { return (c >= 0 ? "+" : "") + c.toFixed(1) + " ¢"; }
    function fmtBeat(df) { return (df >= 10 ? df.toFixed(1) : df.toFixed(2)) + " Hz"; }

    // ---- DOM shell: controls / canvases / instruction / readouts / hint -------
    var controls = QV.el("div", "qv-controls", root);

    QV.toggle(controls, {
      label: "quantize to semitones",
      value: quantize,
      onChange: function (on) {
        quantize = on;
        onPitchChanged();
      }
    });

    var beatToggle = QV.toggle(controls, {
      label: "detune demo (beats)",
      value: beating,
      onChange: function (on) {
        if (on) startBeatDemo();
        else stopBeatDemo(true);
      }
    });

    var btnRoot = QV.buttons(controls, [
      { label: "▶ hear it", title: "Loop a sine at the marker's frequency", primary: true, onClick: toggleHear },
      { label: "play interval", title: "Marker note, then +1.0 V above it (an octave, ×2 Hz), then both", onClick: playInterval }
    ]);
    var hearBtn = btnRoot.qvButtons["▶ hear it"];
    function syncHearLabel() {
      if (hearBtn) hearBtn.textContent = hearing ? "■ stop" : "▶ hear it";
    }

    var canvasWrap = QV.el("div", "", root);
    var ready = false;
    var hero = QV.canvas(canvasWrap, { height: 400, onResize: function () { if (ready) draw(); } });
    var mini = QV.canvas(canvasWrap, { height: 120, onResize: function () { if (ready) draw(); } });
    // Slim envelope strip for the detune demo — hidden until the toggle is on.
    var beatCv = QV.canvas(canvasWrap, { height: 90, onResize: function () { if (ready) draw(); } });
    beatCv.el.style.display = "none";

    var instruction = QV.el("div", "qv-instruction", root);
    instruction.textContent = "drag the marker (anywhere on either plot), tap a piano key to snap to that note — or scrub the voltage in the text below";

    var readouts = QV.el("div", "qv-readouts", root);
    var roVolts = QV.readout(readouts, { label: "pitch CV (raw)" });
    var roQuant = QV.readout(readouts, { label: "quantized" });
    var roFreq = QV.readout(readouts, { label: "frequency" });
    var roNote = QV.readout(readouts, { label: "nearest note" });
    var roCents = QV.readout(readouts, { label: "cents off" });
    var roBeat = QV.readout(readouts, { label: "beat rate" });

    var hint = QV.el("div", "qv-hint", root);
    hint.textContent = "park the marker between two semitones, switch quantize on, then drag slowly — the yellow marker snaps 83.3 mV at a time while the raw ghost glides.";

    function updateReadouts() {
      var ev = effV();
      roVolts.set(fmtVolts(rawV), "voct");
      roQuant.set(quantize ? fmtVolts(quantizeV(rawV)) : "off", quantize ? "voct" : "");
      roFreq.set(fmtFreq(dsp.voctToHz(ev)), "audio");
      roNote.set(dsp.voctToNote(ev), "voct");
      roCents.set(fmtCents(cents(ev)), Math.abs(cents(ev)) < 0.05 ? "gate" : "cv");
      roBeat.set(beating ? fmtBeat(beatHz()) : "—", beating ? "cv" : "");
    }

    // ---- Prose scrub (the voltage lives inside a sentence) --------------------
    var scrubEl = document.getElementById("qv-voct-volts");
    if (scrubEl) {
      QV.scrub(scrubEl, {
        min: V_MIN, max: V_MAX, step: 0.001, value: rawV,
        fmt: fmtVolts,
        onInput: function (v) {
          rawV = clampV(v);
          onPitchChanged();
        }
      });
      // mdbook's document-level keydown handler flips CHAPTERS on
      // ArrowLeft/ArrowRight — keep the scrub's editing keys inside the
      // widget, or stepping the voltage navigates away from the page.
      scrubEl.addEventListener("keydown", function (e) {
        if (e.key === "ArrowLeft" || e.key === "ArrowRight" ||
            e.key === "ArrowUp" || e.key === "ArrowDown" ||
            e.key === "Home" || e.key === "End") {
          e.stopPropagation();
        }
      });
    }

    function syncScrub() {
      if (scrubEl && scrubEl.qvSet) scrubEl.qvSet(rawV);
    }

    // ---- Change plumbing -------------------------------------------------------
    // Drags fire dozens of pointermoves per frame; coalesce the (expensive)
    // canvas redraws to one per animation frame, and debounce the audio-loop
    // restart so a drag retunes ~120 ms after the pointer settles instead of
    // machine-gunning buffer rebuilds.
    var scheduleDraw = QV.coalesce ? QV.coalesce(function () { draw(); }) : draw;
    var retuneAudio = function () {
      if (hearing) startHear();
      else if (beating) startBeatLoop();
    };
    var debouncedRetune = QV.debounce ? QV.debounce(retuneAudio, 120) : retuneAudio;

    // Every pitch-affecting change funnels through here.
    function onPitchChanged() {
      updateReadouts();
      scheduleDraw();
      if (hearing || beating) debouncedRetune();
    }

    // ---- Audio -----------------------------------------------------------------
    // A sine buffer of ~`seconds` trimmed to a whole number of cycles, so the
    // loop point is click-free. Rendered in VOLTS by the shared engine (the
    // same PolyBLEP VCO math as the Rust module); play() divides by 5 V.
    function renderSine(freq, seconds) {
      var cycles = Math.max(1, Math.round(freq * seconds));
      var n = Math.max(32, Math.round((cycles / freq) * SR));
      return dsp.renderVco("sin", freq, SR, n, {});
    }

    function startHear() {
      QV.audio.play(renderSine(currentFreq(), 0.5), SR, { loop: true, gain: 0.25 });
    }
    function toggleHear() {
      if (hearing) {
        hearing = false;
        QV.audio.stop();
      } else {
        if (beating) stopBeatDemo(false); // one voice at a time
        hearing = true;
        startHear();
      }
      syncHearLabel();
    }

    // ---- Detune demo: raw + quantized ghost, half amplitude each ---------------
    // The loop is trimmed to whole cycles of the raw tone (~2 s), matching the
    // envelope plot's window.
    function startBeatLoop() {
      var f1 = dsp.voctToHz(rawV);
      var f2 = dsp.voctToHz(quantizeV(rawV));
      var cycles = Math.max(1, Math.round(f1 * BEAT_WINDOW_SEC));
      var n = Math.max(64, Math.round((cycles / f1) * SR));
      var a = dsp.renderVco("sin", f1, SR, n, {});
      var b = dsp.renderVco("sin", f2, SR, n, {});
      var out = new Float32Array(n);
      for (var i = 0; i < n; i++) out[i] = 0.5 * (a[i] + b[i]);
      QV.audio.play(out, SR, { loop: true, gain: 0.25 });
    }

    function startBeatDemo() {
      if (hearing) {
        hearing = false;
        syncHearLabel();
      }
      beating = true;
      beatCv.el.style.display = "block";
      if (beatToggle && beatToggle.qvSet) beatToggle.qvSet(true);
      startBeatLoop();
      updateReadouts();
      draw();
    }

    // stopAudio=false leaves QV.audio alone (the caller is about to start
    // another voice, and play() already stops whatever is live).
    function stopBeatDemo(stopAudio) {
      beating = false;
      beatCv.el.style.display = "none";
      if (beatToggle && beatToggle.qvSet) beatToggle.qvSet(false);
      if (stopAudio) QV.audio.stop();
      updateReadouts();
      draw();
    }

    // Marker note, the note exactly +1.0 V above it (double the frequency),
    // then both together — the octave as a frequency ratio you can hear. The
    // whole sequence is ONE pre-rendered buffer (no setTimeout chain), and
    // QV.audio.play stops anything already live, so re-taps replace the
    // sequence instead of stacking overlapping copies.
    function playInterval() {
      if (hearing) {
        hearing = false;
        syncHearLabel();
      }
      if (beating) stopBeatDemo(false);
      var f1 = currentFreq();
      var f2 = f1 * 2; // +1.0 V is exactly ×2 — that IS the V/Oct law
      var segA = renderSine(f1, 0.45);
      var segB = renderSine(f2, 0.45);
      var bothA = renderSine(f1, 0.9);
      var bothB = dsp.renderVco("sin", f2, SR, bothA.length, {});
      var gap = Math.round(SR * 0.06);
      var out = new Float32Array(segA.length + gap + segB.length + gap + bothA.length);
      var i, o = 0;
      for (i = 0; i < segA.length; i++) out[o + i] = segA[i];
      o += segA.length + gap;
      for (i = 0; i < segB.length; i++) out[o + i] = segB[i];
      o += segB.length + gap;
      for (i = 0; i < bothA.length; i++) out[o + i] = 0.5 * (bothA[i] + bothB[i]);
      QV.audio.play(out, SR, { gain: 0.25 });
    }

    // ---- Layout ----------------------------------------------------------------
    var heroL = null; // {xs, ys, plotL, plotR, curveT, curveB, pianoT, pianoB, rulerY}
    var miniL = null; // {xs, ys, plotL, plotR, top, bottom}

    function layoutHero() {
      var w = hero.w, h = hero.h;
      var plotL = 52, plotR = w - 14;
      var curveT = 18, curveB = h - 112;
      var pianoT = h - 104, pianoB = h - 52; // the piano strip rides on the ruler
      var rulerY = h - 48;
      return {
        plotL: plotL, plotR: plotR, curveT: curveT, curveB: curveB,
        pianoT: pianoT, pianoB: pianoB, rulerY: rulerY,
        xs: QV.scale([V_MIN, V_MAX], [plotL, plotR]),
        ys: QV.scale([0, F_MAX], [curveB, curveT])
      };
    }

    function layoutMini() {
      var w = mini.w, h = mini.h;
      var plotL = 52, plotR = w - 14;
      var top = 20, bottom = h - 8;
      return {
        plotL: plotL, plotR: plotR, top: top, bottom: bottom,
        xs: QV.scale([V_MIN, V_MAX], [plotL, plotR]),
        ys: QV.logScale([dsp.voctToHz(V_MIN - 0.25), dsp.voctToHz(V_MAX + 0.25)], [bottom, top])
      };
    }

    // ---- Drawing: hero -----------------------------------------------------------
    function draw() {
      drawHero();
      drawMini();
      drawBeat();
    }

    function drawHero() {
      var th = QV.theme();
      var colors = th.colors;
      hero.clear();
      var L = heroL = layoutHero();
      var ctx = hero.ctx;
      var xs = L.xs, ys = L.ys;
      var oct, f, px, py, v;

      // -- Hz gridlines + left labels (linear axis: the blowup is visible) ------
      ctx.save();
      ctx.strokeStyle = colors.grid;
      ctx.fillStyle = colors.ink;
      ctx.lineWidth = 1;
      ctx.font = "10px " + MONO;
      ctx.textAlign = "right";
      ctx.textBaseline = "middle";
      var hzTicks = [0, 500, 1000, 1500, 2000];
      for (var ti = 0; ti < hzTicks.length; ti++) {
        py = ys(hzTicks[ti]);
        ctx.globalAlpha = 1;
        ctx.beginPath();
        ctx.moveTo(L.plotL, py);
        ctx.lineTo(L.plotR, py);
        ctx.stroke();
        ctx.globalAlpha = 0.65;
        ctx.fillText(hzTicks[ti] === 0 ? "0 Hz" : String(hzTicks[ti]), L.plotL - 6, py);
      }
      ctx.restore();

      // -- Faint horizontal guides at octave frequencies (C1..C7 in range) ------
      ctx.save();
      ctx.font = "10px " + MONO;
      ctx.textAlign = "right";
      ctx.textBaseline = "bottom";
      for (oct = -3; oct <= V_MAX; oct++) {
        f = dsp.voctToHz(oct);
        if (f > F_MAX) continue;
        py = ys(f);
        ctx.strokeStyle = colors.voct;
        ctx.globalAlpha = 0.18;
        ctx.setLineDash([3, 4]);
        ctx.beginPath();
        ctx.moveTo(L.plotL, py);
        ctx.lineTo(L.plotR, py);
        ctx.stroke();
        ctx.setLineDash([]);
        ctx.globalAlpha = 0.6;
        ctx.fillStyle = colors.voct;
        ctx.fillText(dsp.voctToNote(oct) + " · " + Math.round(f) + " Hz", L.plotR - 4, py - 1);
      }
      ctx.restore();

      // -- The frequency curve f(V) = 261.63 · 2^V (audio blue) ------------------
      var pts = [];
      var N = 240;
      for (var i = 0; i <= N; i++) {
        v = V_MIN + ((V_MAX - V_MIN) * i) / N;
        pts.push([xs(v), ys(dsp.voctToHz(v))]);
      }
      QV.curve(ctx, pts, { color: colors.audio, width: 2.5 });

      // Curve legend
      ctx.save();
      ctx.font = "600 11px " + MONO;
      ctx.fillStyle = colors.audio;
      ctx.textAlign = "left";
      ctx.textBaseline = "top";
      ctx.fillText("f(V) = 261.63 · 2^V   (linear Hz axis)", L.plotL + 6, L.curveT);
      ctx.restore();

      // -- The voltage ruler (voct yellow) ---------------------------------------
      ctx.save();
      ctx.strokeStyle = colors.voct;
      ctx.globalAlpha = 0.85;
      ctx.lineWidth = 1.5;
      ctx.beginPath();
      ctx.moveTo(L.plotL, L.rulerY);
      ctx.lineTo(L.plotR, L.rulerY);
      ctx.stroke();
      // Note ticks every 1/12 V; tall ticks + labels at whole volts.
      ctx.lineWidth = 1;
      for (var s = S_MIN; s <= S_MAX; s++) {
        v = s / 12;
        px = xs(v);
        var whole = s % 12 === 0;
        ctx.globalAlpha = whole ? 0.9 : 0.4;
        ctx.beginPath();
        ctx.moveTo(px, L.rulerY);
        ctx.lineTo(px, L.rulerY + (whole ? 11 : 5));
        ctx.stroke();
      }
      ctx.font = "10px " + MONO;
      ctx.textAlign = "center";
      ctx.textBaseline = "top";
      for (v = V_MIN; v <= V_MAX; v++) {
        px = xs(v);
        ctx.globalAlpha = 0.9;
        ctx.fillStyle = colors.ink;
        ctx.fillText((v > 0 ? "+" : "") + v + " V", px, L.rulerY + 14);
        ctx.globalAlpha = 0.75;
        ctx.fillStyle = colors.voct;
        ctx.fillText(dsp.voctToNote(v), px, L.rulerY + 27);
      }
      ctx.restore();

      // -- Piano strip: the volts↔notes bijection, tappable ----------------------
      drawPiano(ctx, L, colors, th.dark);

      // -- Ghost marker: the OTHER tone — raw CV when quantizing, quantized ------
      //    ghost when the detune demo is sounding both.
      var ev = effV();
      var gv = quantize ? rawV : quantizeV(rawV);
      if ((quantize || beating) && Math.abs(gv - ev) > 1e-9) {
        drawMarker(ctx, L, gv, colors, true);
      }
      // -- The marker: what the VCO actually receives ----------------------------
      drawMarker(ctx, L, ev, colors, false);
    }

    // A one-octave-per-volt keyboard along the ruler, C2..C7. Equal key widths
    // (each key IS 1/12 V — this strip is a voltage ladder), black keys short,
    // whites continuing beneath them; the key nearest the marker is highlighted.
    function drawPiano(ctx, L, colors, dark) {
      var xs = L.xs;
      var kw = xs(1 / 12) - xs(0); // px per semitone
      var stripH = L.pianoB - L.pianoT;
      var blackB = L.pianoT + stripH * BLACK_H;
      var whiteFill = dark ? "#c9d1d9" : "#fbfbf8";
      var blackFill = dark ? "#0d1117" : "#24292f";
      var s, px;
      var sCur = Math.round(effV() * 12);
      if (sCur < S_MIN) sCur = S_MIN;
      if (sCur > S_MAX) sCur = S_MAX;

      ctx.save();
      ctx.beginPath();
      ctx.rect(L.plotL, L.pianoT, L.plotR - L.plotL, stripH);
      ctx.clip();

      // White base for the whole strip.
      ctx.fillStyle = whiteFill;
      ctx.fillRect(L.plotL, L.pianoT, L.plotR - L.plotL, stripH);

      // Highlight the current key BEFORE boundaries/blacks so they overprint.
      if (!isBlackKey(sCur)) {
        ctx.fillStyle = colors.voct;
        ctx.globalAlpha = 0.5;
        ctx.fillRect(xs(sCur / 12) - kw / 2, L.pianoT, kw, stripH);
        ctx.globalAlpha = 1;
      }

      // Key boundaries: E|F and B|C meet edge-to-edge; boundaries between
      // whites separated by a black run only below that black key.
      ctx.strokeStyle = "rgba(0,0,0,0.35)";
      ctx.lineWidth = 1;
      for (s = S_MIN; s <= S_MAX; s++) {
        if (isBlackKey(s)) {
          px = xs(s / 12);
          ctx.beginPath();
          ctx.moveTo(px, blackB);
          ctx.lineTo(px, L.pianoB);
          ctx.stroke();
        } else if (s < S_MAX && !isBlackKey(s + 1)) {
          px = xs((s + 0.5) / 12);
          ctx.beginPath();
          ctx.moveTo(px, L.pianoT);
          ctx.lineTo(px, L.pianoB);
          ctx.stroke();
        }
      }

      // Black keys (with highlight overlay if current).
      for (s = S_MIN; s <= S_MAX; s++) {
        if (!isBlackKey(s)) continue;
        px = xs(s / 12);
        ctx.fillStyle = blackFill;
        ctx.fillRect(px - kw * (BLACK_W / 2), L.pianoT, kw * BLACK_W, stripH * BLACK_H);
        if (s === sCur) {
          ctx.fillStyle = colors.voct;
          ctx.globalAlpha = 0.65;
          ctx.fillRect(px - kw * (BLACK_W / 2), L.pianoT, kw * BLACK_W, stripH * BLACK_H);
          ctx.globalAlpha = 1;
        }
      }
      ctx.restore();

      // Frame.
      ctx.save();
      ctx.strokeStyle = colors.ink;
      ctx.globalAlpha = 0.35;
      ctx.lineWidth = 1;
      ctx.strokeRect(L.plotL, L.pianoT, L.plotR - L.plotL, stripH);
      ctx.restore();
    }

    // A marker on the ruler, tied by a vertical line to its point on the curve,
    // with a dashed horizontal to the Hz axis. ghost=true draws the faint raw CV.
    function drawMarker(ctx, L, v, colors, ghost) {
      var px = L.xs(v);
      var f = dsp.voctToHz(v);
      var py = L.ys(f);
      ctx.save();
      ctx.globalAlpha = ghost ? 0.38 : 1;

      // vertical tie: ruler -> curve point
      ctx.strokeStyle = colors.voct;
      ctx.lineWidth = ghost ? 1 : 1.5;
      if (ghost) ctx.setLineDash([3, 3]);
      ctx.beginPath();
      ctx.moveTo(px, L.rulerY - 2);
      ctx.lineTo(px, py);
      ctx.stroke();
      ctx.setLineDash([]);

      // dashed horizontal: curve point -> Hz axis (solid marker only)
      if (!ghost) {
        ctx.strokeStyle = colors.audio;
        ctx.lineWidth = 1;
        ctx.globalAlpha = 0.5;
        ctx.setLineDash([4, 4]);
        ctx.beginPath();
        ctx.moveTo(L.plotL, py);
        ctx.lineTo(px, py);
        ctx.stroke();
        ctx.setLineDash([]);
        ctx.globalAlpha = 1;
      }

      // handle on the ruler (yellow)
      ctx.beginPath();
      ctx.arc(px, L.rulerY, ghost ? 5.5 : 7, 0, 2 * Math.PI);
      if (ghost) {
        ctx.strokeStyle = colors.voct;
        ctx.lineWidth = 1.5;
        ctx.stroke();
      } else {
        ctx.fillStyle = colors.voct;
        ctx.fill();
      }

      // dot on the curve (blue)
      ctx.beginPath();
      ctx.arc(px, py, ghost ? 3 : 4.5, 0, 2 * Math.PI);
      if (ghost) {
        ctx.strokeStyle = colors.audio;
        ctx.lineWidth = 1.5;
        ctx.stroke();
      } else {
        ctx.fillStyle = colors.audio;
        ctx.fill();
        // frequency callout, kept inside the plot
        ctx.font = "600 11px " + MONO;
        ctx.fillStyle = colors.audio;
        var onRight = px > (L.plotL + L.plotR) / 2;
        ctx.textAlign = onRight ? "right" : "left";
        ctx.textBaseline = "bottom";
        ctx.fillText(fmtFreq(f), px + (onRight ? -9 : 9), Math.max(py - 6, L.curveT + 14));
      }
      ctx.restore();
    }

    // ---- Drawing: mini log view -----------------------------------------------
    function drawMini() {
      var colors = QV.theme().colors;
      mini.clear();
      var L = miniL = layoutMini();
      var ctx = mini.ctx;
      var oct, py, px;

      // Octave guides — now EQUALLY spaced: that is the whole point of the log axis.
      ctx.save();
      ctx.font = "10px " + MONO;
      ctx.textAlign = "right";
      ctx.textBaseline = "middle";
      for (oct = V_MIN; oct <= V_MAX; oct++) {
        py = L.ys(dsp.voctToHz(oct));
        ctx.strokeStyle = colors.voct;
        ctx.globalAlpha = 0.18;
        ctx.setLineDash([3, 4]);
        ctx.beginPath();
        ctx.moveTo(L.plotL, py);
        ctx.lineTo(L.plotR, py);
        ctx.stroke();
        ctx.setLineDash([]);
        ctx.globalAlpha = 0.65;
        ctx.fillStyle = colors.ink;
        ctx.fillText(dsp.voctToNote(oct), L.plotL - 6, py);
      }
      ctx.restore();

      // The same curve on a log-frequency axis: exactly a straight line.
      QV.curve(ctx, [
        [L.xs(V_MIN), L.ys(dsp.voctToHz(V_MIN))],
        [L.xs(V_MAX), L.ys(dsp.voctToHz(V_MAX))]
      ], { color: colors.audio, width: 2 });

      // Markers (ghost + effective), as vertical lines onto the line.
      var ev = effV();
      var gv = quantize ? rawV : quantizeV(rawV);
      if ((quantize || beating) && Math.abs(gv - ev) > 1e-9) {
        drawMiniMarker(ctx, L, gv, colors, true);
      }
      drawMiniMarker(ctx, L, ev, colors, false);

      // Caption
      ctx.save();
      ctx.font = "600 10px " + MONO;
      ctx.fillStyle = colors.ink;
      ctx.globalAlpha = 0.6;
      ctx.textAlign = "left";
      ctx.textBaseline = "top";
      ctx.fillText("same curve, LOG-frequency axis — equal intervals become equal distances", L.plotL + 6, 4);
      ctx.restore();
    }

    function drawMiniMarker(ctx, L, v, colors, ghost) {
      var px = L.xs(v);
      var py = L.ys(dsp.voctToHz(v));
      ctx.save();
      ctx.globalAlpha = ghost ? 0.38 : 0.9;
      ctx.strokeStyle = colors.voct;
      ctx.lineWidth = ghost ? 1 : 1.5;
      if (ghost) ctx.setLineDash([3, 3]);
      ctx.beginPath();
      ctx.moveTo(px, L.bottom);
      ctx.lineTo(px, py);
      ctx.stroke();
      ctx.setLineDash([]);
      ctx.beginPath();
      ctx.arc(px, py, ghost ? 3 : 4, 0, 2 * Math.PI);
      if (ghost) {
        ctx.strokeStyle = colors.audio;
        ctx.stroke();
      } else {
        ctx.fillStyle = colors.audio;
        ctx.fill();
      }
      ctx.restore();
    }

    // ---- Drawing: beat envelope strip (detune demo) -----------------------------
    // The sum of two equal-amplitude sines at f1, f2 has amplitude envelope
    // |cos(π·Δf·t)| — beats at Δf per second. Plot it over the demo's 2 s loop.
    function drawBeat() {
      if (!beating) return;
      var colors = QV.theme().colors;
      beatCv.clear();
      var ctx = beatCv.ctx;
      var w = beatCv.w, h = beatCv.h;
      var plotL = 52, plotR = w - 14;
      var plotT = 18, plotB = h - 14;
      var midY = (plotT + plotB) / 2;
      var amp = (plotB - plotT) / 2;
      var df = beatHz();
      var xs = QV.scale([0, BEAT_WINDOW_SEC], [plotL, plotR]);
      var i, t, env;

      // Envelope samples (enough points to resolve every |cos| lobe).
      var N = Math.min(2000, Math.max(400, Math.round(df * BEAT_WINDOW_SEC * 24)));
      var up = [], dn = [];
      for (i = 0; i <= N; i++) {
        t = (BEAT_WINDOW_SEC * i) / N;
        env = Math.abs(Math.cos(Math.PI * df * t));
        up.push([xs(t), midY - env * amp]);
        dn.push([xs(t), midY + env * amp]);
      }

      // Second ticks + center line.
      ctx.save();
      ctx.strokeStyle = colors.grid;
      ctx.fillStyle = colors.ink;
      ctx.lineWidth = 1;
      ctx.font = "9px " + MONO;
      ctx.textAlign = "center";
      ctx.textBaseline = "top";
      for (t = 0; t <= BEAT_WINDOW_SEC; t++) {
        var px = xs(t);
        ctx.beginPath();
        ctx.moveTo(px, plotT);
        ctx.lineTo(px, plotB);
        ctx.stroke();
        ctx.globalAlpha = 0.6;
        ctx.fillText(t + " s", px, plotB + 3);
        ctx.globalAlpha = 1;
      }
      ctx.globalAlpha = 0.5;
      ctx.beginPath();
      ctx.moveTo(plotL, midY);
      ctx.lineTo(plotR, midY);
      ctx.stroke();
      ctx.restore();

      // Filled body between the two envelope curves — the "breathing" loudness.
      ctx.save();
      ctx.fillStyle = colors.audio;
      ctx.globalAlpha = 0.14;
      ctx.beginPath();
      for (i = 0; i < up.length; i++) {
        if (i === 0) ctx.moveTo(up[i][0], up[i][1]);
        else ctx.lineTo(up[i][0], up[i][1]);
      }
      for (i = dn.length - 1; i >= 0; i--) ctx.lineTo(dn[i][0], dn[i][1]);
      ctx.closePath();
      ctx.fill();
      ctx.restore();
      QV.curve(ctx, up, { color: colors.audio, width: 1.5 });
      QV.curve(ctx, dn, { color: colors.audio, width: 1.5 });

      // Caption: what is playing, and the predicted beat rate. Shrink both
      // labels on narrow (phone) canvases so they never collide.
      ctx.save();
      ctx.font = "600 10px " + MONO;
      ctx.textBaseline = "top";
      var beatTxt = df < 0.005 ? "beat = 0 Hz (in tune)"
        : "beat = |f raw − f quant| = " + fmtBeat(df);
      var leftTxt = "raw + quantized — summed envelope, 2 s";
      if (ctx.measureText(leftTxt).width + ctx.measureText(beatTxt).width + 16 > plotR - plotL) {
        leftTxt = "raw + quantized, 2 s";
        beatTxt = df < 0.005 ? "beat = 0 Hz" : "beat = " + fmtBeat(df);
        if (ctx.measureText(leftTxt).width + ctx.measureText(beatTxt).width + 16 > plotR - plotL) {
          leftTxt = "";
        }
      }
      if (leftTxt) {
        ctx.fillStyle = colors.ink;
        ctx.globalAlpha = 0.6;
        ctx.textAlign = "left";
        ctx.fillText(leftTxt, plotL, 4);
        ctx.globalAlpha = 1;
      }
      ctx.fillStyle = colors.cv;
      ctx.textAlign = "right";
      ctx.fillText(beatTxt, plotR, 4);
      ctx.restore();
    }

    // ---- Pointer interaction ----------------------------------------------------
    // Tap position -> the piano key's exact voltage. Black keys claim their
    // upper zone; everywhere else falls to the nearest white key, exactly like
    // a real keyboard. y is clamped into the strip so a drag that started on
    // the piano keeps playing keys even if the finger wanders vertically.
    function pianoKeyV(L, x, y) {
      var v = clampV(L.xs.invert(x));
      var s = Math.round(v * 12);
      if (s < S_MIN) s = S_MIN;
      if (s > S_MAX) s = S_MAX;
      if (isBlackKey(s)) {
        var kw = L.xs(1 / 12) - L.xs(0);
        var blackB = L.pianoT + (L.pianoB - L.pianoT) * BLACK_H;
        var yy = Math.min(Math.max(y, L.pianoT), L.pianoB);
        var onBlack = yy <= blackB && Math.abs(x - L.xs(s / 12)) <= kw * (BLACK_W / 2);
        if (!onBlack) {
          s += v * 12 < s ? -1 : 1; // fall to the neighboring white key
          if (s < S_MIN) s = S_MIN;
          if (s > S_MAX) s = S_MAX;
        }
      }
      return s / 12;
    }

    // Drag the marker anywhere on a canvas (the WHOLE surface is the hit-band —
    // curve, ruler, labels — no need to grab the 7 px dot). Pointer events with
    // capture: one path for mouse/touch/pen, drags survive leaving the canvas,
    // and pointercancel (browser steals the gesture) cleanly ends the drag.
    // pickMode decides at pointer-DOWN whether this drag snaps to piano keys or
    // tracks voltage continuously; the mode is locked for the whole drag.
    function attachDrag(cv, getLayout, pickMode) {
      var dragging = false;
      var dragPointerId = null;
      var mode = "free";
      cv.el.style.cursor = "ew-resize";
      // Horizontal drags belong to the widget; vertical swipes still scroll
      // the page — crucial on phones, where a full-width canvas that eats
      // every touch makes the page feel broken.
      cv.el.style.touchAction = "pan-y";
      function apply(e, starting) {
        var rect = cv.el.getBoundingClientRect();
        var L = getLayout();
        if (!L) return;
        var x = e.clientX - rect.left;
        var y = e.clientY - rect.top;
        if (starting) mode = pickMode ? pickMode(L, x, y) : "free";
        rawV = mode === "piano" ? pianoKeyV(L, x, y) : clampV(L.xs.invert(x));
        syncScrub();
        onPitchChanged();
      }
      cv.el.addEventListener("pointerdown", function (e) {
        // A second finger mid-drag must not re-pick the mode or jump the value.
        if (dragging && e.pointerId != null && e.pointerId !== dragPointerId) return;
        dragging = true;
        dragPointerId = e.pointerId != null ? e.pointerId : null;
        if (cv.el.setPointerCapture && e.pointerId != null) {
          try { cv.el.setPointerCapture(e.pointerId); } catch (err) {}
        }
        apply(e, true);
        if (e.cancelable) e.preventDefault();
      });
      cv.el.addEventListener("pointermove", function (e) {
        if (!dragging) return;
        if (dragPointerId != null && e.pointerId != null && e.pointerId !== dragPointerId) {
          return; // not the captured pointer
        }
        apply(e, false);
        if (e.cancelable) e.preventDefault();
      });
      function endDrag(e) {
        if (dragging && dragPointerId != null && e.pointerId != null &&
            e.pointerId !== dragPointerId) {
          return; // some other pointer lifted; the drag goes on
        }
        if (dragging && cv.el.releasePointerCapture && e.pointerId != null) {
          try { cv.el.releasePointerCapture(e.pointerId); } catch (err) {}
        }
        dragging = false;
        dragPointerId = null;
      }
      cv.el.addEventListener("pointerup", endDrag);
      cv.el.addEventListener("pointercancel", endDrag);
      cv.el.addEventListener("lostpointercapture", function () {
        dragging = false;
        dragPointerId = null;
      });
    }
    attachDrag(hero, function () { return heroL || layoutHero(); }, function (L, x, y) {
      return y >= L.pianoT && y <= L.pianoB ? "piano" : "free";
    });
    attachDrag(mini, function () { return miniL || layoutMini(); });

    // Be polite: silence the loops when the tab is hidden (labels stay synced).
    document.addEventListener("visibilitychange", function () {
      if (!document.hidden) return;
      if (hearing) {
        hearing = false;
        QV.audio.stop();
        syncHearLabel();
      }
      if (beating) stopBeatDemo(true);
    });

    // ---- Theme + init -----------------------------------------------------------
    QV.onThemeChange(function () { draw(); });
    ready = true;
    updateReadouts();
    draw();
  });
})();
