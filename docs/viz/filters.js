/*
 * docs/viz/filters.js — "Sculpting the Spectrum"
 *
 * One hero plot, three layers on a shared log-frequency axis:
 *
 *   1. INPUT — the spectrum of a raw sawtooth (QV.dsp.vcoSample +
 *      QV.dsp.spectrumDb), drawn ghosted in the audio color.
 *   2. RESPONSE — the filter's magnitude curve from QV.dsp.svfMagnitude,
 *      which is the EXACT discrete response of Quiver's TPT SVF
 *      (src/modules/filters.rs), not a textbook approximation. Bold, cv color,
 *      with a marker dot at the cutoff.
 *   3. OUTPUT — the same saw pushed sample-by-sample through QV.dsp.svf(44100)
 *      (the stateful mirror of Svf::tick), then FFT'd. Solid audio color.
 *      Output = input × response, visibly.
 *
 * Plus, below the spectrum, a slim TIME-DOMAIN strip: ~3 cycles of the input
 * saw (ghosted) overlaid with the filtered output (solid) — the corners
 * melting is the same story the spectrum tells, in time. It reuses the
 * already-filtered buffer; no extra DSP.
 *
 * Direct manipulation: drag on the canvas — horizontal moves the cutoff,
 * vertical changes the resonance (deliberately less sensitive on touch, so a
 * diagonal finger swipe doesn't slam res to 1). Cutoff/res/pitch live as
 * prose scrubs in the page text. "▶ hear it" loops the FILTERED buffer
 * (never autoplays); any parameter change re-filters and restarts the loop.
 *
 * Scheduling: the full re-filter (8192 warmup + 8192) + FFT is expensive, so
 * scrubs/drags take a split path — the response curve + marker redraw is
 * rAF-coalesced (QV.coalesce) and tracks the finger, while the output-spectrum
 * re-filter is trailing-debounced (QV.debounce ~100 ms). Audio restarts are
 * debounced at ~120 ms. Buffers are pre-allocated once and reused.
 *
 * The only animation loop is the "sweep" gesture (QV.loop): cutoff CV bounces
 * 0.05 → 0.95 → 0.05 CONTINUOUSLY (~2.5 s per cycle) until the button — now
 * reading "■ sweep" — is pressed again, leaving fading ghost copies of the
 * response curve — a long-exposure photo of the classic filter sweep. While
 * it runs, the swept cutoff IS the effective cutoff: response + marker +
 * readouts track it every frame, the output spectrum and time strip re-filter
 * every ~150 ms, and ONE full up-down cycle of audio with the cutoff
 * following the same trajectory per-sample plays with loop:true (displacing
 * the loop audio, which resumes after). Ear and eye share one clock: the
 * visual phase is wall-clock elapsed time modulo the cycle period — exactly
 * the period the looped buffer replays — so they cannot drift apart. The
 * user's own cutoff scrub is never written to and everything returns to it
 * when the sweep is cancelled — any direct cutoff/res/pitch input (including
 * grabbing the plot) cancels it immediately. Under prefers-reduced-motion it
 * renders 5 static ghosts and a single audio pass instead (sound is not
 * motion). Otherwise: redraw on input and on theme change only.
 *
 * Self-contained ES5 IIFE. Consumes window.QuiverViz (loaded per book.toml).
 */
(function () {
  "use strict";
  if (typeof window === "undefined" || !window.QuiverViz) return;
  var QuiverViz = window.QuiverViz;

  QuiverViz.register("filters", function (root, QV) {
    var SR = 44100;
    var FFT_SIZE = 8192; // ~5.4 Hz bins: resolves the 110 Hz harmonic comb
    var WARMUP = 8192;   // let the filter ring past its transient before FFT
    var TOTAL = WARMUP + FFT_SIZE;
    var F_LO = 20, F_HI = 20000;
    var DB_LO = -60, DB_HI = 24;
    var WAVE_V = 8;      // time-strip voltage window (SVF states clip at ±8 V)
    var MODES = { lp: "lowpass", bp: "bandpass", hp: "highpass", notch: "notch" };

    // ---- State ---------------------------------------------------------------
    var mode = "lp";
    var cutoffCv = 0.5;      // the knob CV, 0..1 -> 20·1000^cv Hz
    var res = 0.2;           // resonance knob, 0..1 -> k = 2 - 2·res
    var pitchSemis = -15;    // semitones from C4: -15 = A2 = 110 Hz

    var inSpec = null;       // {freqs, db} of the raw saw
    var outSpec = null;      // {freqs, db} of the filtered saw
    var geom = null;         // plot geometry from the last draw (for dragging)

    function pitchHz() {
      return QV.dsp.voctToHz(pitchSemis / 12);
    }
    function cutoffHz() {
      return QV.dsp.cutoffCvToHz(effCutoffCv());
    }
    // While the sweep runs, its cutoff is the EFFECTIVE cutoff for everything
    // (spectra, marker, readouts, audio); the user's scrub value is untouched.
    function effCutoffCv() {
      return sweepCv != null ? sweepCv : cutoffCv;
    }
    function clamp01(v) {
      return v < 0 ? 0 : v > 1 ? 1 : v;
    }

    // ---- DOM shell: controls / canvases / instruction / readouts / hint ------
    var controls = document.createElement("div");
    controls.className = "qv-controls";
    root.appendChild(controls);

    var canvasWrap = document.createElement("div");
    root.appendChild(canvasWrap);

    var waveWrap = document.createElement("div");
    root.appendChild(waveWrap);

    var instruction = document.createElement("div");
    instruction.className = "qv-instruction";
    instruction.textContent = "Drag on the plot: horizontal moves the cutoff, vertical changes the resonance. Scrub pitch, cutoff and res in the text below — or press sweep for the classic gesture.";
    root.appendChild(instruction);

    var readouts = document.createElement("div");
    readouts.className = "qv-readouts";
    root.appendChild(readouts);

    var hint = document.createElement("div");
    hint.className = "qv-hint";
    hint.textContent = "switch to BP, push res above 0.95, and drag the cutoff onto one harmonic — the filter hands you a single pure partial.";
    root.appendChild(hint);

    var cv = QV.canvas(canvasWrap, {
      height: 360,
      onResize: function () { drawSpectrum(); }
    });
    var wv = QV.canvas(waveWrap, {
      height: 110,
      onResize: function () { drawWave(); }
    });

    // ---- DSP: render the saw, run it through the SVF, take spectra -----------
    // The filter here is QV.dsp.svf — a sample-for-sample mirror of Svf::tick
    // (same prewarp, same k, same soft-clipped integrators), working in VOLTS.
    // Buffers are allocated ONCE and reused across every recompute; the saw is
    // only re-rendered (and the input spectrum only re-FFT'd) when the pitch
    // actually changes.
    var sawBuf = new Float32Array(TOTAL);
    var outBuf = new Float32Array(TOTAL);
    var sawFreq = 0;             // pitch the saw buffer was rendered at (0 = never)
    var filt = QV.dsp.svf(SR);   // one persistent SVF, reset per recompute
    var specVersion = 0;         // bumped per rebuild; keys the polyline cache

    function renderSawInto(buf, freq) {
      var phase = 0;
      var dt = freq / SR;
      for (var i = 0; i < buf.length; i++) {
        buf[i] = QV.dsp.vcoSample("saw", phase, dt);
        phase += dt;
        phase -= Math.floor(phase);
      }
    }

    function rebuildSpectra() {
      var freq = pitchHz();
      if (sawFreq !== freq) {
        renderSawInto(sawBuf, freq);
        sawFreq = freq;
        inSpec = QV.dsp.spectrumDb(sawBuf.subarray(WARMUP), { sampleRate: SR, size: FFT_SIZE });
      }
      filt.reset();
      var fc = cutoffHz();
      for (var i = 0; i < TOTAL; i++) {
        outBuf[i] = filt.tick(sawBuf[i], fc, res)[mode];
      }
      outSpec = QV.dsp.spectrumDb(outBuf.subarray(WARMUP), { sampleRate: SR, size: FFT_SIZE });
      specVersion++;
    }

    // Filtered loop buffer for the audio preview: a whole number of saw periods
    // (~1 s), so the loop point is phase-continuous. What you hear is exactly
    // what the solid curve shows.
    function buildLoopBuffer() {
      var freq = pitchHz();
      var periods = Math.max(1, Math.round(freq)); // ≈ 1 second of loop
      var n = Math.max(1, Math.round((periods * SR) / freq));
      var saw = QV.dsp.renderVco("saw", freq, SR, WARMUP + n);
      var f = QV.dsp.svf(SR);
      var fc = cutoffHz();
      var out = new Float32Array(n);
      for (var i = 0; i < WARMUP + n; i++) {
        var y = f.tick(saw[i], fc, res)[mode];
        if (i >= WARMUP) out[i - WARMUP] = y;
      }
      return out;
    }

    // ---- Audio: "▶ hear it" loops the FILTERED buffer; param changes restart --
    var playing = false;

    function startAudio() {
      if (!playing) return; // a trailing debounce fire after stop is a no-op
      sweepAudioHandle = null; // the loop replaces any swept one-shot
      var handle = QV.audio.play(buildLoopBuffer(), SR, { loop: true, gain: 0.25 });
      if (!handle) {
        playing = false; // no WebAudio: the button must not read "stop"
        syncPlayLabel();
      }
    }
    // Debounce re-filter+restart during scrub drags (trailing ~120 ms).
    var scheduleAudio = QV.debounce(startAudio, 120);
    function stopAudio() {
      playing = false;
      QV.audio.stop();
      syncPlayLabel();
    }
    function togglePlay() {
      if (playing) {
        playing = false; // before endSweep, so it doesn't resume the loop
        if (sweeping) endSweep();
        stopAudio();
      } else {
        if (sweeping) endSweep(); // ear and eye share one clock: no sweep visual over loop audio
        playing = true; // never autoplay: only ever reached from the button
        startAudio();
        syncPlayLabel();
      }
    }

    // ---- Controls row: mode picker + hear-it + sweep buttons ------------------
    QV.segmented(controls, {
      label: "mode",
      value: mode,
      options: [
        { value: "lp", label: "LP" },
        { value: "bp", label: "BP" },
        { value: "hp", label: "HP" },
        { value: "notch", label: "Notch" }
      ],
      onChange: function (v) {
        mode = v;
        update(true);
      }
    });

    var btnRoot = QV.buttons(controls, [
      {
        label: "▶ hear it",
        title: "Loop the filtered saw — exactly the buffer behind the solid curve",
        primary: true,
        onClick: togglePlay
      },
      {
        label: "◠ sweep",
        title: "Ride the cutoff 0.05 → 0.95 and back, leaving a long-exposure trail of response curves",
        onClick: toggleSweep
      }
    ]);
    var playBtn = btnRoot.qvButtons["▶ hear it"];
    var sweepBtn = btnRoot.qvButtons["◠ sweep"];
    function syncPlayLabel() {
      if (playBtn) playBtn.textContent = playing ? "■ stop" : "▶ hear it";
    }
    function syncSweepLabel() {
      if (sweepBtn) sweepBtn.textContent = sweeping ? "■ sweep" : "◠ sweep";
    }

    // ---- Readouts: cutoff Hz, Q, mode -----------------------------------------
    var roCutoff = QV.readout(readouts, { label: "Cutoff" });
    var roQ = QV.readout(readouts, { label: "Q (= 1/k)" });
    var roMode = QV.readout(readouts, { label: "Mode" });

    function fmtHzLong(v) {
      if (v >= 1000) return (v / 1000).toFixed(2) + " kHz";
      return Math.round(v) + " Hz";
    }
    function updateReadouts() {
      roCutoff.set(fmtHzLong(cutoffHz()), "cv");
      var q = 1 / QV.dsp.svfK(res); // k = 2 - 2·res, floored at 1e-5
      roQ.set(q >= 1000 ? "→ ∞ (self-osc)" : q.toFixed(2), "cv");
      roMode.set(MODES[mode], "audio");
    }

    // ---- Prose scrubs (pitch, cutoff CV, res live inside the sentences) ------
    var cutoffScrub = null, resScrub = null, pitchScrub = null;
    var cEl = document.getElementById("qv-filters-cutoff");
    var rEl = document.getElementById("qv-filters-res");
    var pEl = document.getElementById("qv-filters-pitch");
    if (cEl) cutoffScrub = QV.scrub(cEl, {
      min: 0, max: 1, step: 0.01, value: cutoffCv,
      fmt: function (v) { return v.toFixed(2); },
      onInput: function (v) { cutoffCv = v; update(); }
    });
    if (rEl) resScrub = QV.scrub(rEl, {
      min: 0, max: 1, step: 0.01, value: res,
      fmt: function (v) { return v.toFixed(2); },
      onInput: function (v) { res = v; update(); }
    });
    if (pEl) pitchScrub = QV.scrub(pEl, {
      min: -24, max: 12, step: 1, value: pitchSemis,
      fmt: function (v) {
        var voct = v / 12;
        return QV.dsp.voctToNote(voct) + " · " + Math.round(QV.dsp.voctToHz(voct)) + " Hz";
      },
      onInput: function (v) { pitchSemis = v; update(); }
    });

    // ---- Scheduling: light path at 60 fps, heavy path trailing-debounced ------
    // Light: response curve + cutoff marker (and readouts) — cheap, coalesced
    // to one render per animation frame no matter how many input events land.
    // Heavy: 16384-sample re-filter + FFT — debounced so the spectra catch up
    // ~100 ms after the finger pauses, instead of on every pointermove.
    var drawFrame = QV.coalesce(function () { draw(); });
    var heavyUpdate = QV.debounce(function () {
      rebuildSpectra();
      drawFrame();
    }, 100);

    function update(immediate) {
      if (sweeping) endSweep(); // a real parameter edit takes over from the demo
      updateReadouts();
      if (immediate) {
        rebuildSpectra();
        draw();
      } else {
        drawFrame();
        heavyUpdate();
      }
      scheduleAudio();
    }

    // ---- Direct manipulation: drag the plot itself ----------------------------
    // touch-action: none — without it, mobile browsers hijack the drag for page
    // scrolling and fire pointercancel mid-gesture (the "buggy" feel).
    var dragging = false;
    var dragPointerId = null;
    var dragStartY = 0;
    var dragStartRes = 0;
    var dragIsTouch = false;

    function applyDrag(e) {
      if (!geom) return;
      var rect = cv.el.getBoundingClientRect();
      var px = e.clientX - rect.left;
      var py = e.clientY - rect.top;
      // Horizontal: the pointer's frequency IS the cutoff (grab-the-dot feel).
      var f = geom.xs.invert(px);
      if (f < F_LO) f = F_LO;
      if (f > F_HI) f = F_HI;
      cutoffCv = clamp01(QV.dsp.hzToCutoffCv(f));
      // Vertical: relative delta from where the drag started; up = more res.
      // Touch gets a dead zone + a longer runway so a diagonal finger swipe
      // nudges the resonance instead of slamming it to 1.
      var dy = dragStartY - py;
      var dead = dragIsTouch ? 12 : 0;
      if (dy > dead) dy -= dead;
      else if (dy < -dead) dy += dead;
      else dy = 0;
      var runway = dragIsTouch ? geom.plotH * 1.8 : geom.plotH;
      res = clamp01(dragStartRes + dy / runway);
      if (cutoffScrub) cutoffScrub.qvSet(cutoffCv);
      if (resScrub) resScrub.qvSet(res);
      update();
    }
    function onPointerMove(e) {
      if (!dragging) return;
      if (dragPointerId != null && e.pointerId != null && e.pointerId !== dragPointerId) {
        return; // a second finger must not steer the drag
      }
      applyDrag(e);
      if (e.cancelable) e.preventDefault();
    }
    function onPointerUp(e) {
      if (dragging && dragPointerId != null && e && e.pointerId != null &&
          e.pointerId !== dragPointerId) {
        return; // some other pointer lifted; the drag goes on
      }
      dragging = false;
      dragPointerId = null;
      window.removeEventListener("pointermove", onPointerMove);
      window.removeEventListener("pointerup", onPointerUp);
      window.removeEventListener("pointercancel", onPointerUp);
    }
    cv.el.addEventListener("pointerdown", function (e) {
      if (!geom) return;
      if (dragging) return; // a second finger must not restart the drag
      dragging = true;
      dragPointerId = e.pointerId != null ? e.pointerId : null;
      dragIsTouch = e.pointerType === "touch";
      var rect = cv.el.getBoundingClientRect();
      dragStartY = e.clientY - rect.top;
      dragStartRes = res;
      if (cv.el.setPointerCapture && e.pointerId != null) {
        try { cv.el.setPointerCapture(e.pointerId); } catch (err) {}
      }
      window.addEventListener("pointermove", onPointerMove);
      window.addEventListener("pointerup", onPointerUp);
      window.addEventListener("pointercancel", onPointerUp);
      applyDrag(e);
      if (e.cancelable) e.preventDefault();
    });
    cv.el.style.cursor = "crosshair";
    cv.el.style.touchAction = "none";

    // ---- Sweep: the classic filter-sweep gesture, seen AND heard --------------
    // Rides a swept cutoff (never the user's scrub) 0.05 → 0.95 → 0.05 over
    // ~2.5 s. The swept value is the EFFECTIVE cutoff while it runs: the
    // response curve + marker + readouts track it every frame, the output
    // spectrum + time strip re-filter every ~150 ms (stepped, not frozen),
    // ghosts of the response curve fade behind it, and ONE full up-down cycle
    // of audio with the cutoff following the same trajectory per-sample loops
    // along (loop:true). When the sweep is cancelled everything returns to
    // the user's cutoff, and the audio loop (if it was on) resumes.
    var SWEEP_DUR = 2.5;      // seconds, out and back (one cycle of the loop)
    var GHOST_EVERY = 0.05;   // seconds between ghost drops
    var GHOST_FADE = 0.55;    // ghost alpha time constant (seconds)
    var GHOST_ALPHA = 0.45;   // alpha a ghost is born with
    var REFILTER_EVERY = 0.15; // seconds between heavy re-filter+FFT steps
    var sweeping = false;
    var sweepStartMs = 0;     // wall-clock start: the sweep's (and loop's) t=0
    var sweepCv = null;       // swept cutoff CV while sweeping (null = user's)
    var ghostTimer = 0;
    var refilterTimer = 0;
    var ghosts = [];          // [{cv, alpha}], newest last
    var sweepAudioHandle = null; // the swept audio, while it sounds

    // The sweep's cutoff trajectory (u in 0..1): out and back.
    function sweepCvAt(u) {
      var pos = u < 0.5 ? u * 2 : (1 - u) * 2;
      return 0.05 + 0.9 * pos;
    }

    // One-shot render of the sweep: the same saw through the same SVF, with
    // the cutoff following the sweep trajectory PER-SAMPLE — what patchflow
    // does with its envelope→cutoff, applied to the sweep gesture.
    function buildSweepBuffer() {
      var freq = pitchHz();
      var n = Math.round(SWEEP_DUR * SR);
      var saw = QV.dsp.renderVco("saw", freq, SR, n);
      var f = QV.dsp.svf(SR);
      var out = new Float32Array(n);
      for (var i = 0; i < n; i++) {
        var fc = QV.dsp.cutoffCvToHz(sweepCvAt(i / n));
        out[i] = f.tick(saw[i], fc, res)[mode];
      }
      return out;
    }

    // Play the swept audio (an explicit button press — autoplay-safe). The
    // continuous sweep loops its one full up-down cycle (loop:true); under
    // reduced motion it is a single pass. QV.audio.play() displaces whatever
    // is live (the loop included); when the swept audio ends on its own, the
    // loop resumes if it is supposed to be on.
    function playSweepAudio(loop) {
      var h = QV.audio.play(buildSweepBuffer(), SR, { gain: 0.25, loop: !!loop });
      sweepAudioHandle = h;
      if (h && h.src) {
        var prev = h.src.onended;
        h.src.onended = function () {
          if (prev) prev();
          if (sweepAudioHandle === h) {
            sweepAudioHandle = null;
            if (playing) startAudio();
          }
        };
      }
    }

    function endSweep() {
      sweeping = false;
      sweepCv = null;
      if (sweepAudioHandle) {
        var h = sweepAudioHandle;
        sweepAudioHandle = null;
        h.stop(); // no-op if the audio already finished on its own
        if (playing) startAudio(); // resume the loop the sweep displaced
      }
      // Restore the user's cutoff everywhere the sweep borrowed it.
      rebuildSpectra();
      updateReadouts();
      drawWave();
      syncSweepLabel();
      // ghosts keep fading via the loop until they're gone
    }

    function sweepTick(dt) {
      if (dt > 0.1) dt = 0.1; // clamp tab-switch jumps
      var decay = Math.exp(-dt / GHOST_FADE);
      var live = [];
      for (var i = 0; i < ghosts.length; i++) {
        ghosts[i].alpha *= decay;
        if (ghosts[i].alpha >= 0.03) live.push(ghosts[i]);
      }
      ghosts = live;
      if (sweeping) {
        // Ear and eye share one clock: the visual phase is wall-clock elapsed
        // time modulo the cycle period — exactly the period the looped buffer
        // replays — so they cannot drift apart across cycles.
        var u = (((performance.now() - sweepStartMs) / 1000) / SWEEP_DUR) % 1;
        sweepCv = sweepCvAt(u);
        ghostTimer += dt;
        if (ghostTimer >= GHOST_EVERY) {
          ghostTimer = 0;
          ghosts.push({ cv: sweepCv, alpha: GHOST_ALPHA });
        }
        // Heavy step: output spectrum + time strip follow the sweep, in
        // ~150 ms steps (16-ish re-filters per cycle).
        refilterTimer += dt;
        if (refilterTimer >= REFILTER_EVERY) {
          refilterTimer = 0;
          rebuildSpectra();
          updateReadouts();
          drawWave();
        }
      }
      drawSpectrum();
      if (!sweeping && ghosts.length === 0) {
        sweepLoop.pause();
        drawSpectrum(); // final clean frame at the user's cutoff
      }
    }
    var sweepLoop = QV.loop(root, sweepTick);

    function toggleSweep() {
      if (sweepLoop.reduced) {
        // Reduced motion: no animation — toggle 5 static ghost snapshots.
        // The swept AUDIO still plays on press: sound is not motion.
        if (ghosts.length) {
          ghosts = [];
          if (sweepAudioHandle) {
            var h = sweepAudioHandle;
            sweepAudioHandle = null;
            h.stop();
            if (playing) startAudio();
          }
        } else {
          var cvs = [0.1, 0.3, 0.5, 0.7, 0.9];
          ghosts = [];
          for (var i = 0; i < cvs.length; i++) {
            ghosts.push({ cv: cvs[i], alpha: 0.14 + 0.07 * i });
          }
          playSweepAudio();
        }
        drawSpectrum();
        return;
      }
      if (sweeping) {
        endSweep(); // loop stays alive to fade the remaining ghosts
        return;
      }
      sweeping = true;
      sweepStartMs = performance.now();
      ghostTimer = 0;
      refilterTimer = REFILTER_EVERY; // first tick re-filters immediately
      sweepCv = sweepCvAt(0);
      syncSweepLabel();
      playSweepAudio(true); // one full up-down cycle, looped until cancelled
      sweepLoop.play();
      if (sweepLoop.playing) {
        sweepTick(0); // first frame lands NOW, in the same gesture
      }
    }

    // ---- Drawing ---------------------------------------------------------------
    function clampDb(d) {
      if (!isFinite(d) || d < DB_LO) return DB_LO;
      if (d > DB_HI) return DB_HI;
      return d;
    }

    // Spectrum bins -> pixel polyline, clipped to the 20 Hz–20 kHz window.
    function specPts(spec, xs, ys) {
      var pts = [];
      for (var i = 1; i < spec.freqs.length; i++) {
        var f = spec.freqs[i];
        if (f < F_LO) continue;
        if (f > F_HI) break;
        pts.push([xs(f), ys(clampDb(spec.db[i]))]);
      }
      return pts;
    }

    // Spectrum polylines are cached across frames (keyed by data version and
    // canvas size) so sweep frames don't rebuild two ~4000-point arrays each.
    var ptsCache = { key: "", inPts: null, outPts: null };

    // The exact discrete response of the Rust SVF, in dB along the log axis.
    function responsePts(xs, ys, fc, n) {
      var N = n || 256;
      var pts = [];
      for (var i = 0; i <= N; i++) {
        var f = F_LO * Math.pow(F_HI / F_LO, i / N);
        var mag = QV.dsp.svfMagnitude(mode, f, fc, res, SR);
        pts.push([xs(f), ys(clampDb(20 * Math.log10(mag + 1e-12)))]);
      }
      return pts;
    }

    function drawLegend(ctx, x, y, colors) {
      ctx.save();
      ctx.font = "600 11px var(--mono-font, monospace)";
      ctx.textBaseline = "top";
      ctx.textAlign = "left";
      var parts = [
        ["input", colors.audio, 0.45],
        [" × ", colors.ink, 0.6],
        ["response", colors.cv, 1],
        [" → ", colors.ink, 0.6],
        ["output", colors.audio, 1]
      ];
      var cx = x;
      for (var i = 0; i < parts.length; i++) {
        ctx.fillStyle = parts[i][1];
        ctx.globalAlpha = parts[i][2];
        ctx.fillText(parts[i][0], cx, y);
        cx += ctx.measureText(parts[i][0]).width;
      }
      ctx.restore();
    }

    function drawSpectrum() {
      if (!inSpec || !outSpec) return; // first canvas resize fires pre-init
      var w = cv.w, h = cv.h, ctx = cv.ctx;
      var t = QV.theme();
      var colors = t.colors;
      cv.clear();

      var padL = 44, padR = 14, padT = 14, padB = 34;
      var plotW = w - padL - padR;
      var plotH = h - padT - padB;
      if (plotW < 40 || plotH < 40) return;

      var xs = QV.logScale([F_LO, F_HI], [padL, padL + plotW]);
      var ys = QV.scale([DB_LO, DB_HI], [padT + plotH, padT]);
      geom = { xs: xs, ys: ys, plotTop: padT, plotH: plotH };

      QV.axes(ctx, {
        x: padL, y: padT, w: plotW, h: plotH,
        xscale: xs, yscale: ys,
        xlabel: "frequency (Hz)",
        theme: t
      });
      // dB unit tag, top-left of the axis frame.
      ctx.save();
      ctx.fillStyle = colors.ink;
      ctx.globalAlpha = 0.6;
      ctx.font = "11px var(--mono-font, monospace)";
      ctx.textAlign = "left";
      ctx.textBaseline = "bottom";
      ctx.fillText("dB", padL + 2, padT - 2);
      ctx.restore();

      // Everything cutoff-shaped follows the sweep while it runs; the user's
      // scrub value itself is never written to.
      var fc = QV.dsp.cutoffCvToHz(effCutoffCv());

      // 0 dB reference line (unity gain).
      QV.curve(ctx, [[padL, ys(0)], [padL + plotW, ys(0)]], {
        color: colors.ink, width: 1, alpha: 0.25, dash: [4, 4]
      });

      // Layer 1 — INPUT spectrum, ghosted (cached polyline).
      var key = specVersion + ":" + w + "x" + h;
      if (ptsCache.key !== key) {
        ptsCache.key = key;
        ptsCache.inPts = specPts(inSpec, xs, ys);
        ptsCache.outPts = specPts(outSpec, xs, ys);
      }
      QV.curve(ctx, ptsCache.inPts, {
        color: colors.audio, width: 1.5, alpha: 0.25
      });

      // Sweep trail — fading ghost copies of the response curve, oldest first.
      for (var gi = 0; gi < ghosts.length; gi++) {
        var g = ghosts[gi];
        QV.curve(ctx, responsePts(xs, ys, QV.dsp.cutoffCvToHz(g.cv), 96), {
          color: colors.cv, width: 1.5, alpha: g.alpha
        });
      }

      // Cutoff guide: faint vertical in the cv color (the drag handle's rail).
      QV.curve(ctx, [[xs(fc), padT], [xs(fc), padT + plotH]], {
        color: colors.cv, width: 1, alpha: 0.3, dash: [3, 4]
      });

      // Layer 2 — the filter's exact magnitude response, bold.
      QV.curve(ctx, responsePts(xs, ys, fc), { color: colors.cv, width: 2.5 });

      // Layer 3 — OUTPUT spectrum, solid: input × response, audibly and visibly.
      QV.curve(ctx, ptsCache.outPts, { color: colors.audio, width: 2 });

      // Marker dot at the cutoff, sitting on the response curve.
      var fcMag = QV.dsp.svfMagnitude(mode, fc, fc, res, SR);
      var fcDb = clampDb(20 * Math.log10(fcMag + 1e-12));
      ctx.save();
      ctx.fillStyle = colors.cv;
      ctx.beginPath();
      ctx.arc(xs(fc), ys(fcDb), 4.5, 0, 2 * Math.PI);
      ctx.fill();
      ctx.strokeStyle = colors.panel;
      ctx.lineWidth = 1.5;
      ctx.stroke();
      ctx.restore();

      drawLegend(ctx, padL + 8, padT + 6, colors);
    }

    // Time-domain strip: ~3 cycles of the raw saw (ghosted) under the filtered
    // output (solid) — "the corners melt", visible in time. Both traces come
    // straight from the buffers the spectrum was computed from; zero extra DSP.
    function drawWave() {
      if (!outSpec || !sawFreq) return; // pre-init resize
      var w = wv.w, h = wv.h, ctx = wv.ctx;
      var t = QV.theme();
      var colors = t.colors;
      wv.clear();

      var padL = 44, padR = 14, padT = 6, padB = 6;
      var plotW = w - padL - padR;
      var plotH = h - padT - padB;
      if (plotW < 40 || plotH < 20) return;

      var n = Math.round(3 * SR / sawFreq); // ~3 cycles at the rendered pitch
      if (n < 16) n = 16;
      if (n > FFT_SIZE) n = FFT_SIZE;
      var xs = QV.scale([0, n - 1], [padL, padL + plotW]);
      var ys = QV.scale([-WAVE_V, WAVE_V], [padT + plotH, padT]);

      // frame + 0 V line
      ctx.save();
      ctx.strokeStyle = colors.ink;
      ctx.globalAlpha = 0.35;
      ctx.lineWidth = 1;
      ctx.strokeRect(padL, padT, plotW, plotH);
      ctx.restore();
      QV.curve(ctx, [[padL, ys(0)], [padL + plotW, ys(0)]], {
        color: colors.ink, width: 1, alpha: 0.2, dash: [4, 4]
      });

      // Clip to the frame: high-res output can ring past the ±8 V window.
      ctx.save();
      ctx.beginPath();
      ctx.rect(padL, padT, plotW, plotH);
      ctx.clip();
      QV.wave(ctx, sawBuf.subarray(WARMUP, WARMUP + n), {
        xscale: xs, yscale: ys, color: colors.audio, width: 1.5, alpha: 0.25
      });
      QV.wave(ctx, outBuf.subarray(WARMUP, WARMUP + n), {
        xscale: xs, yscale: ys, color: colors.audio, width: 2
      });
      ctx.restore();

      ctx.save();
      ctx.fillStyle = colors.ink;
      ctx.globalAlpha = 0.55;
      ctx.font = "10px var(--mono-font, monospace)";
      ctx.textAlign = "left";
      ctx.textBaseline = "top";
      ctx.fillText("time · 3 cycles — input (ghost) vs output (solid)", padL + 6, padT + 4);
      ctx.restore();
    }

    function draw() {
      drawSpectrum();
      drawWave();
    }

    // Be polite: silence the loops when the tab is hidden (labels stay synced).
    document.addEventListener("visibilitychange", function () {
      if (!document.hidden) return;
      if (playing) stopAudio(); // first, so endSweep doesn't resume it
      if (sweeping) endSweep();
    });

    // ---- Theme + init -----------------------------------------------------------
    QV.onThemeChange(function () { draw(); });
    rebuildSpectra();
    updateReadouts();
    draw();
  });
})();
