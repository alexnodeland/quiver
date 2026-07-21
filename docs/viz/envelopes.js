/*
 * docs/viz/envelopes.js — "Envelopes Shape Time"
 *
 * A directly draggable ADSR envelope. The hero canvas shows the module's real
 * contour (QV.dsp.adsrEnvelope mirrors src/modules/dynamics.rs Adsr::tick
 * exactly, including the release-rate scaling from the level at gate-fall).
 * Four handles live on the plot:
 *
 *   - attack peak        drag x -> attack time (through the exponential knob law)
 *   - decay->sustain     drag x -> decay time, drag y -> sustain LEVEL
 *   - gate-off marker    drag x -> gate length (on the green gate bar)
 *   - release end        drag x -> release time
 *
 * Time-handle drags move the 0..1 knob CV, not seconds, so short times get
 * fine control — the same 1 ms .. 10 s law the module's attack/decay/release
 * inputs use (T = 0.001 * 10000^cv). A mini-canvas below ghosts the enveloped
 * saw so "CV shapes audio" is visible, and a knob-law strip plots all three
 * time CVs on T(cv) = 0.001 * 10000^cv — log-time paper, where the law is a
 * straight line and the markers slide as you drag. A "retrigger mid-release"
 * toggle adds a second gate-on halfway through the release: like Adsr::tick,
 * the new attack continues FROM THE CURRENT LEVEL, never resetting to zero
 * (the coral second pass). "▶ hear it" LOOPS whichever version is on screen
 * (the buffer ends in the release's silence, so loop:true reads as a
 * retriggering note) with a playhead that wraps each cycle, until the button
 * — now "■ stop" — is pressed again; param edits mid-loop restart the loop
 * debounced rather than killing it. "gate" stays a one-shot: it IS a single
 * key press.
 *
 * All expensive recompute+redraw is routed through QV.coalesce so a burst of
 * pointermove events costs one render per animation frame, and the audio-band
 * min/max decimation is cached in reused buffers, recomputed only when the
 * envelope actually changed (not per pointer event).
 *
 * Self-contained ES5 IIFE. Consumes window.QuiverViz (loaded first).
 */
(function () {
  "use strict";
  if (typeof window === "undefined" || !window.QuiverViz) return;

  window.QuiverViz.register("envelopes", function (root, QV) {
    var dsp = QV.dsp;
    var MONO = 'var(--mono-font, "Source Code Pro", monospace)';

    // ---- constants -----------------------------------------------------------
    var GATE_MIN = 0.05, GATE_MAX = 4;   // seconds (matches the prose scrub)
    var CV_DRAG_PX = 240;                // px of horizontal drag = full 0..1 knob
    var HIT_R = 12;                      // handle hit radius, CSS px (mouse/pen)
    var HIT_R_TOUCH = 22;                // fingers need bigger corners
    var C3_HZ = dsp.voctToHz(-1);        // -1 V on the V/Oct line = C3
    var AUDIO_PLOT_SR = 4000;            // plotting rate for the ghost waveform

    // ---- state (knob CVs, exactly what the module's inputs would read) -------
    var attackCv = dsp.adsrTimeToCv(0.05);   // 50 ms
    var decayCv = dsp.adsrTimeToCv(0.15);    // 150 ms
    var sustain = 0.6;                       // LEVEL 0..1 (6 V on env out)
    var releaseCv = dsp.adsrTimeToCv(0.3);   // 300 ms
    var gateSec = 0.8;
    var expMode = false;                     // shape input low = linear
    var retrigOn = false;                    // second gate-on mid-release

    // Derived (filled by recompute)
    var attackSec = 0, decaySec = 0, releaseSec = 0, totalSec = 1;
    var plotSr = 2000;
    var envPlot = null;        // Float32Array of LEVEL 0..1 at plotSr (single gate)
    var envRetrigPlot = null;  // same, with the second gate (retrig mode only)
    var audioPlot = null;      // Float32Array of VOLTS (enveloped saw) at AUDIO_PLOT_SR
    var retrigT2 = 0;          // second gate-on time (halfway through the release)
    var retrigGate2End = 0;    // second gate-off time

    var ready = false;
    var handles = [];      // filled each draw: {id, x, y} in CSS px
    var hoverId = null;
    var drag = null;       // {id, pointerId, x0, y0, cv0, sus0}
    var lastL = null;      // last layout, for pointer inversion
    var playT = null;      // playhead time in seconds, or null
    var playDur = 0;
    var playStartMs = 0;   // wall-clock start of the sounding note
    var noteHandle = null; // the looped "▶ hear it" playback, while it sounds

    function clamp01(v) { return v < 0 ? 0 : v > 1 ? 1 : v; }

    function fmtTime(s) {
      var ms = s * 1000;
      if (ms < 10) return ms.toFixed(1) + " ms";
      if (ms < 1000) return Math.round(ms) + " ms";
      if (s < 10) return s.toFixed(2) + " s";
      return s.toFixed(1) + " s";
    }

    // ---- DOM shell: controls / canvases / instruction / readouts / hint ------
    var controls = QV.el("div", "qv-controls", root);
    var noteBtns = QV.buttons(controls, [
      {
        label: "▶ hear it", primary: true,
        title: "Loop a saw at C3 through this exact envelope — the release's silence makes the loop a retriggering note",
        onClick: toggleHear
      },
      {
        label: "gate",
        title: "Press the key again — gate rises, the envelope retriggers (one pass)",
        onClick: playNote
      }
    ]);
    var hearBtn = noteBtns.qvButtons["▶ hear it"];
    function syncHearLabel() {
      if (hearBtn) hearBtn.textContent = noteHandle ? "■ stop" : "▶ hear it";
    }
    QV.toggle(controls, {
      label: "exponential stages",
      value: expMode,
      onChange: function (v) {
        expMode = v;
        render();
      }
    });
    QV.toggle(controls, {
      label: "retrigger mid-release",
      value: retrigOn,
      title: "Gate on, gate off, then a second gate-on halfway through the release — " +
        "the new attack continues from the current level, never from zero",
      onChange: function (v) {
        retrigOn = v;
        render();
      }
    });

    var canvasWrap = QV.el("div", null, root);
    var hero = QV.canvas(canvasWrap, { height: 320, onResize: function () { draw(); } });
    var mini = QV.canvas(canvasWrap, {
      height: 92,
      onResize: function () { miniDirty = true; draw(); }
    });
    var law = QV.canvas(canvasWrap, { height: 130, onResize: function () { draw(); } });

    // Dragging handles IS the interaction: never let the browser turn a handle
    // drag into a page scroll (a scroll-steal fires pointercancel mid-drag).
    hero.el.style.touchAction = "none";

    var instruction = QV.el("div", "qv-instruction", root);
    instruction.textContent =
      "Drag the handles: attack peak (time), decay corner (time; up/down sets sustain), " +
      "the gate-off marker on the green bar, and the release end. Then press ▶ hear it.";

    var readouts = QV.el("div", "qv-readouts", root);
    var roA = QV.readout(readouts, { label: "Attack" });
    var roD = QV.readout(readouts, { label: "Decay" });
    var roS = QV.readout(readouts, { label: "Sustain" });
    var roR = QV.readout(readouts, { label: "Release" });
    var roG = QV.readout(readouts, { label: "Gate" });

    var hint = QV.el("div", "qv-hint", root);
    hint.textContent =
      "drag the attack peak all the way left and the decay corner down to the floor — " +
      "a one-handle pluck. Then flip on retrigger mid-release and watch the coral second " +
      "pass climb from wherever the release left off — never from zero.";

    // ---- prose scrub: gate length lives in the sentence below the widget -----
    var gateScrub = null;
    var gEl = document.getElementById("qv-envelopes-gate");
    if (gEl) {
      gateScrub = QV.scrub(gEl, {
        min: GATE_MIN, max: GATE_MAX, step: 0.05, value: gateSec,
        fmt: function (v) { return v.toFixed(2); },
        onInput: function (v) {
          gateSec = v;
          render();
        }
      });
    }

    // ---- model: multi-gate ADSR, mirroring Adsr::tick's transition order -----
    // gates = [[onSec, offSec], ...]. A gate-rise enters Attack FROM THE CURRENT
    // LEVEL (src/modules/dynamics.rs: "it never resets `level` to zero"); a
    // gate-fall captures the level to scale the release rate. Same stage math
    // as QV.dsp.adsrEnvelope, generalized to any number of gates.
    function adsrMultiEnvelope(gates, opts) {
      var a = Math.max(1e-4, opts.attackSec);
      var d = Math.max(1e-4, opts.decaySec);
      var s = clamp01(opts.sustainLevel);
      var r = Math.max(1e-4, opts.releaseSec);
      var sr = opts.sampleRate || 1000;
      var totalN = Math.round(opts.totalSec * sr);
      var exp = !!opts.exp;
      var EXP_DONE = 1e-3;

      var out = new Float32Array(totalN);
      var level = 0;
      var stage = "idle";
      var releaseStart = 0;
      var prevGate = false;
      var attackRate = 1 / (a * sr);
      var decayRate = (1 - s) / (d * sr);
      var aCoef = dsp.envCoef(a, sr);
      var dCoef = dsp.envCoef(d, sr);
      var rCoef = dsp.envCoef(r, sr);

      for (var i = 0; i < totalN; i++) {
        var t = i / sr;
        var gateHigh = false;
        for (var g = 0; g < gates.length; g++) {
          if (t >= gates[g][0] && t < gates[g][1]) { gateHigh = true; break; }
        }
        // Transitions first, then stage processing — Adsr::tick's exact order.
        if (gateHigh && !prevGate) {
          stage = "attack"; // continues from the current level
        } else if (!gateHigh && prevGate && stage !== "idle") {
          releaseStart = level;
          stage = "release";
        }
        if (stage === "attack") {
          if (exp) {
            level += (1 - level) * (1 - aCoef);
            if (level >= 1 - EXP_DONE) { level = 1; stage = "decay"; }
          } else {
            level += attackRate;
            if (level >= 1) { level = 1; stage = "decay"; }
          }
        } else if (stage === "decay") {
          if (exp) {
            level += (s - level) * (1 - dCoef);
            if (level - s <= EXP_DONE) { level = s; stage = "sustain"; }
          } else {
            level -= decayRate;
            if (level <= s) { level = s; stage = "sustain"; }
          }
        } else if (stage === "sustain") {
          level = s;
        } else if (stage === "release") {
          if (exp) {
            level += (0 - level) * (1 - rCoef);
            if (level <= EXP_DONE) { level = 0; stage = "idle"; }
          } else {
            level -= releaseStart / (r * sr);
            if (level <= 0) { level = 0; stage = "idle"; }
          }
        } else {
          level = 0;
        }
        prevGate = gateHigh;
        out[i] = level;
      }
      return out;
    }

    function retrigGates() {
      return [[0, gateSec], [retrigT2, retrigGate2End]];
    }

    // ---- model: rebuild the envelope + ghost audio from the current knobs ----
    function recompute() {
      attackSec = dsp.adsrCvToTime(attackCv);
      decaySec = dsp.adsrCvToTime(decayCv);
      releaseSec = dsp.adsrCvToTime(releaseCv);
      // One-pole releases need ~5 time constants of runway to read as finished.
      var relDisp = releaseSec * (expMode ? 5 : 1);
      retrigT2 = gateSec + relDisp / 2;
      retrigGate2End = retrigT2 + gateSec;
      var lastEvent = retrigOn ? retrigGate2End + relDisp : gateSec + relDisp;
      totalSec = Math.max(lastEvent, attackSec + decaySec) * 1.08;
      if (totalSec < 0.25) totalSec = 0.25;
      plotSr = Math.max(400, Math.min(4000, Math.round(8000 / totalSec)));

      var envOpts = {
        attackSec: attackSec, decaySec: decaySec, sustainLevel: sustain,
        releaseSec: releaseSec, gateSec: gateSec, totalSec: totalSec,
        sampleRate: plotSr, exp: expMode
      };
      envPlot = dsp.adsrEnvelope(envOpts);
      envRetrigPlot = retrigOn ? adsrMultiEnvelope(retrigGates(), envOpts) : null;

      // Ghost audio: the same envelope multiplied into a saw at C3 (in volts).
      var n = Math.round(totalSec * AUDIO_PLOT_SR);
      var saw = dsp.renderVco("saw", C3_HZ, AUDIO_PLOT_SR, n);
      var aOpts = {
        attackSec: attackSec, decaySec: decaySec, sustainLevel: sustain,
        releaseSec: releaseSec, gateSec: gateSec, totalSec: totalSec,
        sampleRate: AUDIO_PLOT_SR, exp: expMode
      };
      var env = retrigOn ? adsrMultiEnvelope(retrigGates(), aOpts) : dsp.adsrEnvelope(aOpts);
      for (var i = 0; i < n; i++) saw[i] *= env[i];
      audioPlot = saw;
      miniDirty = true; // the min/max band must be re-decimated (once, coalesced)
    }

    function envLevelAt(t) {
      if (!envPlot || !envPlot.length) return 0;
      var i = Math.round(t * plotSr);
      if (i < 0) i = 0;
      if (i >= envPlot.length) i = envPlot.length - 1;
      return envPlot[i];
    }

    // ---- readouts: time (smart ms/s) plus the 0..1 knob CV that produces it --
    function updateReadouts() {
      roA.set(fmtTime(attackSec) + " · cv " + attackCv.toFixed(2), "mod");
      roD.set(fmtTime(decaySec) + " · cv " + decayCv.toFixed(2), "mod");
      roS.set(sustain.toFixed(2) + " → " + (sustain * 10).toFixed(1) + " V", "mod");
      roR.set(fmtTime(releaseSec) + " · cv " + releaseCv.toFixed(2), "mod");
      roG.set(fmtTime(gateSec), "gate");
    }

    // ---- playback: what you see is what you hear ------------------------------
    // "▶ hear it" LOOPS the note with a playhead that wraps each cycle;
    // "gate" is a single key press — one pass. Ear and eye share one clock:
    // the playhead is wall-clock elapsed time (modulo the buffer duration
    // while looping — exactly the period the looped buffer replays).
    var loopApi = QV.loop(root, function () {
      if (playT == null) { loopApi.pause(); return; }
      var elapsed = (performance.now() - playStartMs) / 1000;
      if (noteHandle) {
        if (!QV.audio.playing()) {
          // displaced outside stopHear (another widget's audio): clean up
          noteHandle = null;
          playT = null;
          syncHearLabel();
          loopApi.pause();
        } else {
          playT = playDur > 0 ? elapsed % playDur : 0;
        }
      } else {
        playT = elapsed;
        if (playT >= playDur || !QV.audio.playing()) {
          playT = null;
          loopApi.pause();
        }
      }
      draw();
    });

    // Render the on-screen envelope into a saw at C3 (44.1 kHz), in volts.
    function renderNote() {
      var sr = 44100;
      // Give the one-pole release enough tail to actually reach silence.
      var relTail = releaseSec * (expMode ? 7 : 1);
      var dur, env, n;
      if (retrigOn) {
        dur = Math.min(retrigGate2End + relTail + 0.05, 12);
        n = Math.round(dur * sr);
        env = adsrMultiEnvelope(retrigGates(), {
          attackSec: attackSec, decaySec: decaySec, sustainLevel: sustain,
          releaseSec: releaseSec, totalSec: dur, sampleRate: sr, exp: expMode
        });
      } else {
        dur = Math.min(gateSec + relTail + 0.05, 12);
        n = Math.round(dur * sr);
        env = dsp.adsrEnvelope({
          attackSec: attackSec, decaySec: decaySec, sustainLevel: sustain,
          releaseSec: releaseSec, gateSec: gateSec, totalSec: dur,
          sampleRate: sr, exp: expMode
        });
      }
      var buf = dsp.renderVco("saw", C3_HZ, sr, n);
      for (var i = 0; i < n; i++) buf[i] *= env[i];
      return { buf: buf, dur: dur };
    }

    function playNote() { // the "gate" button: a single key press, one pass
      if (noteHandle) stopHear(); // the key press takes over from the loop
      var r = renderNote();
      var handle = QV.audio.play(r.buf, 44100, { gain: 0.25 });
      if (handle && !loopApi.reduced) {
        playT = 0;
        playDur = r.dur;
        playStartMs = performance.now();
        loopApi.play();
      }
      draw();
    }

    // (Re)start the "▶ hear it" loop with the current envelope. Never
    // autoplays: only ever reached from the button or a mid-loop param edit.
    function startHear() {
      var r = renderNote();
      noteHandle = QV.audio.play(r.buf, 44100, { gain: 0.25, loop: true });
      syncHearLabel();
      if (!noteHandle) return; // no WebAudio: the button must not read "stop"
      playDur = r.dur;
      playStartMs = performance.now();
      if (!loopApi.reduced) {
        playT = 0;
        loopApi.play();
      }
      draw();
    }
    function stopHear() {
      if (noteHandle) {
        noteHandle.stop();
        noteHandle = null;
      }
      playT = null;
      syncHearLabel();
      draw();
    }
    function toggleHear() {
      if (noteHandle) stopHear();
      else startHear();
    }
    // Param edits mid-loop RESTART the loop (trailing ~150 ms) with the new
    // envelope, rather than killing it.
    var restartHear = QV.debounce(function () {
      if (noteHandle) startHear();
    }, 150);

    // ---- layout ----------------------------------------------------------------
    var PAD_L = 46, PAD_R = 14;

    function layout() {
      var padT = 10, padB = 34;
      var plot = { x: PAD_L, y: padT, w: hero.w - PAD_L - PAD_R, h: hero.h - padT - padB };
      var xs = QV.scale([0, totalSec], [plot.x, plot.x + plot.w]);
      var ys = QV.scale([0, 10], [plot.y + plot.h, plot.y]); // env out is 0..10 V
      return { plot: plot, xs: xs, ys: ys };
    }

    // ---- hero drawing -----------------------------------------------------------
    var GATE_BAR_H = 14;

    function gateBar(ctx, xs, baseY, plotX, t0, t1, colors, label) {
      var x0 = xs(t0), x1 = xs(t1);
      ctx.save();
      ctx.fillStyle = colors.gate;
      ctx.globalAlpha = 0.22;
      ctx.fillRect(x0, baseY - GATE_BAR_H, x1 - x0, GATE_BAR_H);
      ctx.globalAlpha = 0.75;
      ctx.strokeStyle = colors.gate;
      ctx.lineWidth = 1.5;
      ctx.beginPath();
      ctx.moveTo(x0, baseY - GATE_BAR_H);
      ctx.lineTo(x1, baseY - GATE_BAR_H);
      ctx.lineTo(x1, baseY);
      ctx.stroke();
      if (label && x1 - x0 > 44) {
        ctx.globalAlpha = 0.9;
        ctx.fillStyle = colors.gate;
        ctx.font = "600 9px " + MONO;
        ctx.textAlign = "left";
        ctx.textBaseline = "middle";
        ctx.fillText(label, x0 + 5, baseY - GATE_BAR_H / 2 + 0.5);
      }
      ctx.restore();
    }

    function drawHero() {
      var ctx = hero.ctx;
      var t = QV.theme();
      var colors = t.colors;
      hero.clear();
      var L = lastL = layout();
      var xs = L.xs, ys = L.ys, plot = L.plot;
      var baseY = plot.y + plot.h;

      QV.axes(ctx, {
        x: plot.x, y: plot.y, w: plot.w, h: plot.h,
        xscale: xs, yscale: ys, theme: t,
        xlabel: "time (s)", ylabel: "env (V)"
      });

      // --- gate bar(s): green region under the curve, high while the key is down
      gateBar(ctx, xs, baseY, plot.x, 0, gateSec, colors, "gate +5 V");
      if (retrigOn) {
        gateBar(ctx, xs, baseY, plot.x, Math.min(retrigT2, totalSec),
          Math.min(retrigGate2End, totalSec), colors, "gate again");
      }

      // --- envelope curve (violet), filled faintly underneath -------------------
      var n = envPlot.length;
      var stride = Math.max(1, Math.floor(n / (plot.w * 2)));
      var pts = [];
      for (var i = 0; i < n; i += stride) {
        pts.push([xs(i / plotSr), ys(envPlot[i] * 10)]);
      }
      pts.push([xs((n - 1) / plotSr), ys(envPlot[n - 1] * 10)]);
      ctx.save();
      ctx.globalAlpha = 0.08;
      ctx.fillStyle = colors.mod;
      ctx.beginPath();
      ctx.moveTo(pts[0][0], baseY);
      for (var p = 0; p < pts.length; p++) ctx.lineTo(pts[p][0], pts[p][1]);
      ctx.lineTo(pts[pts.length - 1][0], baseY);
      ctx.closePath();
      ctx.fill();
      ctx.restore();
      QV.curve(ctx, pts, { color: colors.mod, width: 2.5 });

      // --- retrig second pass (coral): identical until the second gate-on, so
      // only the divergent tail is drawn — the attack that climbs from the
      // current level instead of resetting to zero.
      if (retrigOn && envRetrigPlot) {
        var i0r = Math.max(0, Math.floor(retrigT2 * plotSr));
        var ptsR = [];
        for (var ir = i0r; ir < envRetrigPlot.length; ir += stride) {
          ptsR.push([xs(ir / plotSr), ys(envRetrigPlot[ir] * 10)]);
        }
        if (envRetrigPlot.length) {
          ptsR.push([
            xs((envRetrigPlot.length - 1) / plotSr),
            ys(envRetrigPlot[envRetrigPlot.length - 1] * 10)
          ]);
        }
        QV.curve(ctx, ptsR, { color: colors.cv, width: 2.5 });
      }

      // --- stage letters along the top (only where the segment has room) --------
      var tA = attackSec;
      var tD = attackSec + decaySec;
      var tR = gateSec + releaseSec;
      ctx.save();
      ctx.fillStyle = colors.ink;
      ctx.globalAlpha = 0.45;
      ctx.font = "600 10px " + MONO;
      ctx.textAlign = "center";
      ctx.textBaseline = "top";
      var segs = [
        ["A", 0, Math.min(tA, totalSec)],
        ["D", tA, Math.min(tD, gateSec)],
        ["S", Math.min(tD, gateSec), gateSec],
        ["R", gateSec, Math.min(tR, totalSec)]
      ];
      for (var s = 0; s < segs.length; s++) {
        var w0 = xs(segs[s][1]), w1 = xs(segs[s][2]);
        if (w1 - w0 > 16) ctx.fillText(segs[s][0], (w0 + w1) / 2, plot.y + 4);
      }
      ctx.restore();

      // --- playhead (only while a note is sounding) ------------------------------
      if (playT != null) {
        var px = xs(playT);
        if (px <= plot.x + plot.w) {
          ctx.save();
          ctx.strokeStyle = colors.cv;
          ctx.globalAlpha = 0.8;
          ctx.lineWidth = 1.5;
          ctx.beginPath();
          ctx.moveTo(px, plot.y);
          ctx.lineTo(px, baseY);
          ctx.stroke();
          ctx.restore();
        }
      }

      // --- handles: hollow circles, coral + slightly larger while dragged --------
      handles = [
        { id: "attack", x: xs(Math.min(tA, totalSec)), y: ys(envLevelAt(tA) * 10) },
        { id: "decay", x: xs(Math.min(tD, totalSec)), y: ys(envLevelAt(tD) * 10) },
        { id: "gate", x: xs(gateSec), y: baseY - GATE_BAR_H / 2 },
        { id: "release", x: xs(Math.min(tR, totalSec)), y: ys(0) }
      ];
      for (var hI = 0; hI < handles.length; hI++) {
        var hd = handles[hI];
        var dragging = drag && drag.id === hd.id;
        var hot = dragging || (!drag && hoverId === hd.id);
        var base = hd.id === "gate" ? colors.gate : colors.mod;
        ctx.save();
        ctx.beginPath();
        ctx.arc(hd.x, hd.y, dragging ? 10 : hot ? 8 : 7, 0, 2 * Math.PI);
        ctx.fillStyle = colors.panel;
        ctx.globalAlpha = 1;
        ctx.fill();
        ctx.lineWidth = 2;
        ctx.strokeStyle = hot ? colors.cv : base;
        ctx.stroke();
        ctx.restore();
      }
    }

    // ---- mini canvas: the enveloped saw — CV shaping audio, visibly ------------
    // The min/max decimation is cached in reused buffers and recomputed only
    // when the envelope changed (miniDirty) or the canvas was resized — never
    // per pointer event. Redraws (playhead, theme) just restroke the cache.
    var miniMin = null, miniMax = null, miniCols = 0, miniDirty = true;

    function computeMiniBand(cols) {
      if (!miniMin || miniMin.length < cols) {
        miniMin = new Float32Array(cols);
        miniMax = new Float32Array(cols);
      }
      miniCols = cols;
      var n = audioPlot.length;
      var sampPerPx = n / cols;
      for (var col = 0; col < cols; col++) {
        var i0 = Math.floor(col * sampPerPx);
        var i1 = Math.min(n, Math.floor((col + 1) * sampPerPx) + 1);
        var mn = 0, mx = 0;
        for (var i = i0; i < i1; i++) {
          var v = audioPlot[i];
          if (v < mn) mn = v;
          if (v > mx) mx = v;
        }
        miniMin[col] = mn;
        miniMax[col] = mx;
      }
      miniDirty = false;
    }

    function drawMini() {
      var ctx = mini.ctx;
      var colors = QV.theme().colors;
      mini.clear();
      var padT = 16, padB = 6;
      var plotX = PAD_L, plotW = mini.w - PAD_L - PAD_R;
      var midY = padT + (mini.h - padT - padB) / 2;
      var amp = (mini.h - padT - padB) / 2;

      ctx.save();
      ctx.fillStyle = colors.ink;
      ctx.globalAlpha = 0.55;
      ctx.font = "9px " + MONO;
      ctx.textAlign = "left";
      ctx.textBaseline = "top";
      ctx.fillText("AUDIO OUT — the same envelope, multiplied into a saw at C3 (±5 V)", plotX, 2);
      ctx.globalAlpha = 0.15;
      ctx.strokeStyle = colors.ink;
      ctx.lineWidth = 1;
      ctx.beginPath();
      ctx.moveTo(plotX, midY);
      ctx.lineTo(plotX + plotW, midY);
      ctx.stroke();
      ctx.restore();

      // Min/max column band: the classic DAW waveform contour (from the cache).
      var cols = Math.max(1, Math.floor(plotW));
      if (miniDirty || miniCols !== cols) computeMiniBand(cols);
      ctx.save();
      ctx.strokeStyle = colors.audio;
      ctx.globalAlpha = 0.85;
      ctx.lineWidth = 1;
      ctx.beginPath();
      for (var col = 0; col < miniCols; col++) {
        var x = plotX + col + 0.5;
        ctx.moveTo(x, midY - (miniMax[col] / 5) * amp);
        ctx.lineTo(x, midY - (miniMin[col] / 5) * amp);
      }
      ctx.stroke();
      ctx.restore();

      if (playT != null) {
        var px = plotX + (playT / totalSec) * plotW;
        if (px <= plotX + plotW) {
          ctx.save();
          ctx.strokeStyle = colors.cv;
          ctx.globalAlpha = 0.8;
          ctx.lineWidth = 1.5;
          ctx.beginPath();
          ctx.moveTo(px, padT);
          ctx.lineTo(px, mini.h - padB);
          ctx.stroke();
          ctx.restore();
        }
      }
    }

    // ---- knob-law strip: T(cv) = 0.001 * 10000^cv on log-time paper -----------
    // An exponential law is a straight line on a log axis. The three markers
    // are the live attack/decay/release CVs; the one being dragged goes coral
    // and slides along the line as you feel the same law under your finger.
    var LAW_TICKS = [
      [0.001, "1 ms"], [0.01, null], [0.1, "100 ms"], [1, null], [10, "10 s"]
    ];

    function drawLaw() {
      var ctx = law.ctx;
      var colors = QV.theme().colors;
      law.clear();
      var padT = 18, padB = 16;
      var plotX = PAD_L, plotW = law.w - PAD_L - PAD_R;
      var plotY = padT, plotH = law.h - padT - padB;
      var xs = QV.logScale([0.001, 10], [plotX, plotX + plotW]);
      var ys = QV.scale([0, 1], [plotY + plotH, plotY]);

      ctx.save();
      ctx.fillStyle = colors.ink;
      ctx.globalAlpha = 0.55;
      ctx.font = "9px " + MONO;
      ctx.textAlign = "left";
      ctx.textBaseline = "top";
      ctx.fillText("THE KNOB LAW — every time input: T(cv) = 0.001 · 10000^cv", plotX, 2);
      ctx.restore();

      ctx.save();
      ctx.lineWidth = 1;
      ctx.font = "9px " + MONO;
      for (var i = 0; i < LAW_TICKS.length; i++) {
        var tx = xs(LAW_TICKS[i][0]);
        ctx.strokeStyle = colors.grid;
        ctx.globalAlpha = 1;
        ctx.beginPath();
        ctx.moveTo(tx, plotY);
        ctx.lineTo(tx, plotY + plotH);
        ctx.stroke();
        if (LAW_TICKS[i][1]) {
          ctx.fillStyle = colors.ink;
          ctx.globalAlpha = 0.6;
          ctx.textAlign = "center";
          ctx.textBaseline = "top";
          ctx.fillText(LAW_TICKS[i][1], tx, plotY + plotH + 3);
        }
      }
      ctx.globalAlpha = 0.35;
      ctx.strokeStyle = colors.ink;
      ctx.strokeRect(plotX, plotY, plotW, plotH);
      ctx.restore();

      // The law itself: exponential in time = a straight line on log-time paper.
      QV.curve(ctx, [[xs(0.001), ys(0)], [xs(10), ys(1)]], {
        color: colors.ink, width: 1.5, alpha: 0.45
      });

      var marks = [
        ["attack", attackCv, "A"],
        ["decay", decayCv, "D"],
        ["release", releaseCv, "R"]
      ];
      for (var m = 0; m < marks.length; m++) {
        var hot = drag && drag.id === marks[m][0];
        var mx = xs(dsp.adsrCvToTime(marks[m][1]));
        var my = ys(marks[m][1]);
        ctx.save();
        ctx.beginPath();
        ctx.arc(mx, my, hot ? 6 : 4.5, 0, 2 * Math.PI);
        ctx.fillStyle = colors.panel;
        ctx.fill();
        ctx.lineWidth = 2;
        ctx.strokeStyle = hot ? colors.cv : colors.mod;
        ctx.stroke();
        ctx.fillStyle = hot ? colors.cv : colors.mod;
        ctx.globalAlpha = 0.9;
        ctx.font = "600 9px " + MONO;
        ctx.textAlign = "center";
        ctx.textBaseline = "bottom";
        ctx.fillText(marks[m][2], mx, my - (hot ? 8 : 7));
        ctx.restore();
      }
    }

    function draw() {
      if (!ready) return;
      drawHero();
      drawMini();
      drawLaw();
    }

    // Coalesced recompute+redraw: a burst of pointermove/scrub events inside
    // one frame costs exactly one envelope rebuild and one paint of all three
    // canvases. Every parameter-changing path funnels through here.
    var render = QV.coalesce(function () {
      if (!ready) return;
      // A parameter edit changes the plotted timeline out from under a live
      // playhead — cancel a one-shot sweep (its audio plays out; the loop
      // sees playT == null on its next frame and pauses itself). A LOOPING
      // note instead keeps going and restarts, debounced, with the new shape.
      if (!noteHandle) playT = null;
      recompute();
      updateReadouts();
      draw();
      if (noteHandle) restartHear();
    });

    // ---- pointer: drag handles on the hero canvas --------------------------------
    function heroPt(ev) {
      var r = hero.el.getBoundingClientRect();
      return { x: ev.clientX - r.left, y: ev.clientY - r.top };
    }

    function hitRadius(ev) {
      return ev && ev.pointerType === "touch" ? HIT_R_TOUCH : HIT_R;
    }

    function hitHandle(px, py, r) {
      var best = null, bestD = r * r;
      for (var i = 0; i < handles.length; i++) {
        var dx = px - handles[i].x, dy = py - handles[i].y;
        var d = dx * dx + dy * dy;
        if (d <= bestD) { best = handles[i].id; bestD = d; }
      }
      return best;
    }

    function cvOf(id) {
      if (id === "attack") return attackCv;
      if (id === "decay") return decayCv;
      if (id === "release") return releaseCv;
      return 0;
    }

    hero.el.addEventListener("pointerdown", function (ev) {
      if (drag) return; // one drag at a time; a second finger must not steal it
      var p = heroPt(ev);
      var id = hitHandle(p.x, p.y, hitRadius(ev));
      if (!id) return;
      drag = { id: id, pointerId: ev.pointerId, x0: p.x, y0: p.y, cv0: cvOf(id), sus0: sustain };
      if (hero.el.setPointerCapture && ev.pointerId != null) {
        try { hero.el.setPointerCapture(ev.pointerId); } catch (e) {}
      }
      hero.el.style.cursor = "grabbing";
      if (ev.cancelable) ev.preventDefault();
      draw();
    });

    hero.el.addEventListener("pointermove", function (ev) {
      var p = heroPt(ev);
      if (drag) {
        if (ev.pointerId != null && drag.pointerId != null && ev.pointerId !== drag.pointerId) {
          return; // not the captured pointer
        }
        var dx = p.x - drag.x0;
        // Time drags move the KNOB CV, not seconds: a pixel is worth a constant
        // slice of the 0..1 knob, so times multiply — fine control when short,
        // broad strokes when long. This is the module's own 1 ms..10 s law.
        if (drag.id === "attack") {
          attackCv = clamp01(drag.cv0 + dx / CV_DRAG_PX);
        } else if (drag.id === "decay") {
          decayCv = clamp01(drag.cv0 + dx / CV_DRAG_PX);
          if (lastL) sustain = clamp01(lastL.ys.invert(p.y) / 10);
        } else if (drag.id === "release") {
          releaseCv = clamp01(drag.cv0 + dx / CV_DRAG_PX);
        } else if (drag.id === "gate") {
          if (lastL) {
            var g = lastL.xs.invert(p.x);
            gateSec = g < GATE_MIN ? GATE_MIN : g > GATE_MAX ? GATE_MAX : g;
            if (gateScrub && gateScrub.qvSet) gateScrub.qvSet(gateSec);
          }
        }
        render(); // coalesced: many moves per frame, one recompute+paint
        if (ev.cancelable) ev.preventDefault();
      } else {
        var id = hitHandle(p.x, p.y, hitRadius(ev));
        if (id !== hoverId) {
          hoverId = id;
          hero.el.style.cursor = id ? "grab" : "default";
          draw();
        }
      }
    });

    // pointerup, pointercancel (phone call, edge-swipe, scroll-steal), and
    // lostpointercapture all release the drag — a handle must never stay
    // glued to a pointer that is gone.
    function endDrag(ev) {
      if (!drag) return;
      if (ev && ev.pointerId != null && drag.pointerId != null &&
          ev.pointerId !== drag.pointerId) {
        return; // some other pointer lifted; the drag goes on
      }
      if (hero.el.releasePointerCapture && ev && ev.pointerId != null) {
        try { hero.el.releasePointerCapture(ev.pointerId); } catch (e) {}
      }
      drag = null;
      hero.el.style.cursor = hoverId ? "grab" : "default";
      draw();
    }
    hero.el.addEventListener("pointerup", endDrag);
    hero.el.addEventListener("pointercancel", endDrag);
    hero.el.addEventListener("lostpointercapture", endDrag);

    // Be polite: silence the loop when the tab is hidden (label stays synced).
    document.addEventListener("visibilitychange", function () {
      if (document.hidden && noteHandle) stopHear();
    });

    // ---- theme + init -------------------------------------------------------------
    QV.onThemeChange(function () { draw(); });
    recompute();
    updateReadouts();
    ready = true;
    draw();
  });
})();
