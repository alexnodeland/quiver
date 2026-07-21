/*
 * docs/viz/oscillators.js — "The Shape of a Wave"
 *
 * The Vco's four waveforms as ONE mental object: the time-domain shape and
 * its harmonic recipe, side by side, computed from the same rendered buffer.
 * The bandlimited toggle is the payoff — switch PolyBLEP off and the naive
 * shapes fill the spectrum floor with inharmonic aliasing hash; the yellow
 * dashed lines mark where the TRUE harmonics n*f0 live, so the hash is
 * visibly *not on the grid*. Press "hear it" and the plotted buffer is the
 * played buffer (volts in, volts out).
 *
 * Two extra viz pieces tie the picture together:
 *   - a PHASOR DIAL (slow visual rate, QV.loop): phase runs around a circle,
 *     and a dot traces the corresponding point on the waveform — the shape is
 *     "what the wave does along the way around".
 *   - THEORY TARGET RINGS on the spectrum: hollow rings at the closed-form
 *     Fourier amplitude of each harmonic, anchored to the measured
 *     fundamental. Bandlimited stems land on the rings; naive ones miss.
 *
 * Bandlimited math is QV.dsp.vcoSample (Vco::tick verbatim); the naive
 * shapes are computed inline here — Vco::tick minus the polyblep/polyblamp
 * corrections.
 *
 * Scheduling: the recompute+FFT+redraw path is rAF-coalesced (QV.coalesce),
 * so a burst of scrub events costs one render; audio-loop restarts while
 * dragging are debounced (QV.debounce) so the loop doesn't machine-gun.
 * Buffers (waveBuf, fftBuf, wave point list) are allocated once and reused.
 *
 * Self-contained ES5 IIFE. Consumes window.QuiverViz (loaded first).
 */
(function () {
  "use strict";
  if (typeof window === "undefined" || !window.QuiverViz) return;
  var QuiverViz = window.QuiverViz;

  var TAU = Math.PI * 2;
  var SR = 44100;           // render rate for spectrum + audio
  var FFT_SIZE = 4096;      // ~10.8 Hz bins at 44.1 kHz
  var CYCLES = 3;           // cycles shown in the time-domain panel
  var WAVE_POINTS = 1200;   // dense evaluation grid for the shape trace
  var FMIN = 20, FMAX = 20000;
  var DB_FLOOR = -80, DB_TOP = 6;
  var PHASOR_HZ = 0.5;      // dial turns per second — slow enough to follow

  // Naive (NOT bandlimited) waveforms: Vco::tick minus the PolyBLEP/BLAMP
  // corrections. One sample in VOLTS (±5), phase in [0,1).
  function naiveSample(shape, phase, pw) {
    if (shape === "sin") return Math.sin(phase * TAU) * 5;
    if (shape === "saw") return (2 * phase - 1) * 5;
    if (shape === "sqr") {
      pw = pw == null ? 0.5 : Math.min(0.95, Math.max(0.05, pw));
      return (phase < pw ? 1 : -1) * 5;
    }
    return (1 - 4 * Math.abs(phase - 0.5)) * 5; // triangle
  }

  // Theoretical Fourier amplitude of harmonic n RELATIVE to the fundamental:
  //   sine      -> n = 1 only
  //   saw       -> 1/n, every n
  //   square    -> |sin(pi n pw)| / (n |sin(pi pw)|) — the general pulse-wave
  //                recipe, which collapses to the classic odd-only 1/n at
  //                pw = 0.5 (the even terms cancel exactly there)
  //   triangle  -> 1/n^2, odd n only (the sign flip is invisible in dB)
  function theoryRel(shape, n, pw) {
    if (n < 1) return 0;
    if (shape === "sin") return n === 1 ? 1 : 0;
    if (shape === "saw") return 1 / n;
    if (shape === "sqr") {
      var d = pw == null ? 0.5 : Math.min(0.95, Math.max(0.05, pw));
      var fund = Math.abs(Math.sin(Math.PI * d));
      if (fund < 1e-9) return 0;
      return Math.abs(Math.sin(Math.PI * n * d)) / (n * fund);
    }
    return n % 2 === 1 ? 1 / (n * n) : 0; // triangle
  }

  QuiverViz.register("oscillators", function (root, QV) {
    // ---- State ---------------------------------------------------------------
    var shape = "saw";
    var pitchV = 0;         // V/Oct, 0 V = C4
    var pw = 0.5;           // pulse width (square only)
    var bandlimited = true; // PolyBLEP/BLAMP on/off
    var freq = QV.dsp.voctToHz(pitchV);
    var spec = null;        // {freqs, db} of the rendered buffer
    var audioHandle = null;
    var phasorPhase = 0;    // [0,1) — the dial's slow-motion phase
    var th = QV.theme();    // cached; refreshed by onThemeChange
    var geom = null;        // cached waveform plot geometry (set on render)

    // Reused buffers — the scrub-drag hot path allocates nothing.
    var waveBuf = new Float32Array(WAVE_POINTS); // dense shape trace (volts)
    var fftBuf = new Float32Array(FFT_SIZE);     // true 44.1 kHz render for the FFT
    var wavePts = [];                            // pixel-space polyline, reused
    for (var wpi = 0; wpi < WAVE_POINTS; wpi++) wavePts.push([0, 0]);

    // ---- DOM shell: controls / phasor row / canvases / readouts / hint --------
    var controls = QV.el("div", "qv-controls", root);
    var phasorRow = QV.el("div", "", root);
    phasorRow.style.display = "flex";
    phasorRow.style.alignItems = "center";
    phasorRow.style.flexWrap = "wrap";
    phasorRow.style.gap = "10px";
    var dialBox = QV.el("div", "", phasorRow);
    dialBox.style.flex = "0 0 auto";
    dialBox.style.width = "140px";
    var phasorSide = QV.el("div", "", phasorRow);
    phasorSide.style.flex = "1 1 160px";
    var phasorCaption = QV.el("div", "qv-caption", phasorSide);
    phasorCaption.textContent =
      "The oscillator's whole state is one number: a phase running around " +
      "this circle. The violet dot on the wave below is the value the shape " +
      "assigns to that point of the circle.";
    var waveWrap = QV.el("div", "", root);
    var specWrap = QV.el("div", "", root);
    var instruction = QV.el("div", "qv-instruction", root);
    instruction.textContent =
      "Pick a shape and drag the pitch in the sentence below the panel. " +
      "Top: the wave in time (volts), traced by the phasor dot. Bottom: the same buffer's " +
      "spectrum — dashed lines mark the harmonics n·f₀, hollow rings the theoretical recipe.";
    var readouts = QV.el("div", "qv-readouts", root);
    var hint = QV.el("div", "qv-hint", root);
    hint.textContent =
      "pick saw, drag the pitch up to +4 V, press ▶ hear it, then switch bandlimited off — " +
      "the spectrum floor fills with hash that sits BETWEEN the dashed harmonic lines. That is aliasing.";

    // ---- Controls -------------------------------------------------------------
    QV.segmented(controls, {
      label: "waveform",
      options: [
        { value: "sin" }, { value: "tri" }, { value: "saw" }, { value: "sqr" }
      ],
      value: shape,
      onChange: function (v) { shape = v; recompute(); }
    });

    var btnRoot = QV.buttons(controls, [
      {
        label: "▶ hear it",
        primary: true,
        title: "Loop the rendered buffer — what you see is what you hear",
        onClick: toggleAudio
      }
    ]);
    var hearBtn = btnRoot.qvButtons["▶ hear it"];

    QV.toggle(controls, {
      label: "bandlimited (PolyBLEP)",
      value: bandlimited,
      onChange: function (v) { bandlimited = v; recompute(); }
    });

    // ---- Canvases -------------------------------------------------------------
    var phasorCv = QV.canvas(dialBox, {
      height: 140,
      onResize: function () { if (spec) drawPhasor(th); }
    });
    var waveCv = QV.canvas(waveWrap, { height: 200, onResize: function () { draw(); } });
    var specCv = QV.canvas(specWrap, { height: 250, onResize: function () { draw(); } });

    // Offscreen layer for the waveform plot: rendered once per recompute /
    // resize / theme change, then blitted each phasor frame so the animated
    // dot costs one drawImage, not a 1200-point re-stroke.
    var waveLayer = document.createElement("canvas");
    var waveLayerCtx = waveLayer.getContext("2d");

    // ---- Phasor play/pause (QV.loop pauses it under reduced motion) ----------
    var phasorBtnRoot = QV.buttons(phasorSide, [
      {
        label: "⏸ pause",
        title: "Spin or pause the phase dial (visual only — not the audio)",
        onClick: togglePhasor
      }
    ]);
    var phasorBtn = phasorBtnRoot.qvButtons["⏸ pause"];

    var phasorLoop = QV.loop(root, function (dt) {
      phasorPhase += dt * PHASOR_HZ;
      phasorPhase -= Math.floor(phasorPhase);
      if (!spec) return;
      drawPhasor(th);
      blitWave(th);
    });

    function syncPhasorBtn() {
      if (phasorBtn) phasorBtn.textContent = phasorLoop.playing ? "⏸ pause" : "▶ spin";
    }
    function togglePhasor() {
      if (phasorLoop.playing) {
        phasorLoop.pause();
      } else {
        phasorLoop.play();
        if (!phasorLoop.playing) {
          // Reduced motion (or offscreen): advance one step instead of animating.
          phasorPhase += 1 / 12;
          phasorPhase -= Math.floor(phasorPhase);
          if (spec) { drawPhasor(th); blitWave(th); }
        }
      }
      syncPhasorBtn();
    }

    // ---- Readouts -------------------------------------------------------------
    var roPitch = QV.readout(readouts, { label: "Pitch CV" });
    var roFreq = QV.readout(readouts, { label: "Frequency" });
    var roNote = QV.readout(readouts, { label: "Nearest note" });
    var roHarm = QV.readout(readouts, { label: "Harmonics < Nyquist" });

    function updateReadouts() {
      roPitch.set(pitchV.toFixed(2) + " V", "voct");
      roFreq.set(
        freq >= 1000 ? (freq / 1000).toFixed(2) + " kHz" : freq.toFixed(1) + " Hz",
        "audio"
      );
      roNote.set(QV.dsp.voctToNote(pitchV), "voct");
      var nyq = SR / 2;
      var nh = shape === "sin" ? 1 : Math.max(1, Math.floor(nyq / freq));
      if (!bandlimited && shape !== "sin") {
        roHarm.set(nh + " + aliases", "cv");
      } else {
        roHarm.set(String(nh), "");
      }
    }

    // ---- Prose scrubs (pitch in volts, pulse width) ---------------------------
    var pitchEl = document.getElementById("qv-oscillators-pitch");
    if (pitchEl) {
      QV.scrub(pitchEl, {
        min: -2, max: 4, step: 0.05, value: pitchV,
        fmt: function (v) { return v.toFixed(2) + " V"; },
        onInput: function (v) { pitchV = v; recompute(); }
      });
    }
    var pwEl = document.getElementById("qv-oscillators-pw");
    if (pwEl) {
      QV.scrub(pwEl, {
        min: 0.05, max: 0.95, step: 0.01, value: pw,
        fmt: function (v) { return v.toFixed(2); },
        onInput: function (v) { pw = v; recompute(); }
      });
    }

    // ---- Rendering pipeline ---------------------------------------------------
    // One render path feeds everything: the spectrum FFTs a true 44.1 kHz
    // buffer of the current settings; the audio loops the same math; the
    // shape trace evaluates the same per-sample function on a dense phase
    // grid (with the REAL dt, so the PolyBLEP transition width is honest).
    function sampleAt(phase, dt) {
      return bandlimited
        ? QV.dsp.vcoSample(shape, phase, dt, pw)
        : naiveSample(shape, phase, pw);
    }

    // Fill an existing buffer with n running-phase samples (like Vco ticking).
    function fillRunning(buf, n) {
      var dt = freq / SR;
      var phase = 0;
      for (var i = 0; i < n; i++) {
        buf[i] = sampleAt(phase, dt);
        phase += dt;
        phase -= Math.floor(phase);
      }
      return buf;
    }

    function recomputeNow() {
      freq = QV.dsp.voctToHz(pitchV);
      var dt = freq / SR;
      for (var i = 0; i < WAVE_POINTS; i++) {
        var ph = (i * CYCLES / WAVE_POINTS) % 1;
        waveBuf[i] = sampleAt(ph, dt);
      }
      fillRunning(fftBuf, FFT_SIZE);
      spec = QV.dsp.spectrumDb(fftBuf, { sampleRate: SR, size: FFT_SIZE });
      updateReadouts();
      if (pwEl) pwEl.style.opacity = shape === "sqr" ? "" : "0.45";
      if (audioHandle) restartAudioDebounced();
      draw();
    }
    // Coalesced entry point for every control/scrub: a burst of input events
    // in one frame costs exactly one recompute+FFT+redraw.
    var recompute = QV.coalesce ? QV.coalesce(recomputeNow) : recomputeNow;

    // ---- Audio: the plotted buffer IS the played buffer. Never autoplays. -----
    function buildAudioBuf() {
      // ~1.5 s trimmed to a whole number of cycles so the loop wrap is seamless.
      var cyc = Math.max(1, Math.round(1.5 * freq));
      var n = Math.max(32, Math.round((cyc / freq) * SR));
      return fillRunning(new Float32Array(n), n);
    }
    function startAudio() {
      audioHandle = QV.audio.play(buildAudioBuf(), SR, { gain: 0.25, loop: true });
      syncAudioBtn();
    }
    function restartAudio() {
      if (!audioHandle) return; // user stopped while a restart was pending
      // QV.audio.play() stops the previous loop before starting the new one.
      audioHandle = QV.audio.play(buildAudioBuf(), SR, { gain: 0.25, loop: true });
    }
    // While a scrub is dragged the visuals track every frame, but the audio
    // loop only rebuilds once the parameter settles (~120 ms of quiet).
    var restartAudioDebounced = QV.debounce ? QV.debounce(restartAudio, 120) : restartAudio;
    function stopAudio() {
      if (audioHandle) {
        audioHandle.stop();
        audioHandle = null;
      }
      syncAudioBtn();
    }
    function toggleAudio() {
      if (audioHandle) stopAudio();
      else startAudio();
    }
    function syncAudioBtn() {
      if (hearBtn) hearBtn.textContent = audioHandle ? "■ stop" : "▶ hear it";
    }
    // Be polite: silence the loop when the tab is hidden.
    document.addEventListener("visibilitychange", function () {
      if (document.hidden && audioHandle) stopAudio();
    });

    // ---- Drawing ---------------------------------------------------------------
    var PAD_L = 46, PAD_R = 12, PAD_T = 12, PAD_B = 26;

    function draw() {
      if (!spec) return; // canvases resize before the first recompute
      renderWaveLayer(th);
      blitWave(th);
      drawPhasor(th);
      drawSpec(th);
    }

    function shapeLabel() {
      return { sin: "sine", tri: "triangle", saw: "sawtooth", sqr: "square" }[shape] || shape;
    }

    function legend(ctx, x, y, parts, inkColor) {
      ctx.save();
      ctx.font = "600 11px var(--mono-font, monospace)";
      ctx.textBaseline = "top";
      ctx.textAlign = "left";
      var cx = x;
      for (var i = 0; i < parts.length; i++) {
        ctx.fillStyle = parts[i][1];
        ctx.globalAlpha = parts[i][1] === inkColor ? 0.6 : 1;
        ctx.fillText(parts[i][0], cx, y);
        cx += ctx.measureText(parts[i][0]).width;
      }
      ctx.restore();
    }

    // ---- Waveform panel (offscreen layer + animated phase dot) ----------------
    function waveGeom() {
      var plotW = waveCv.w - PAD_L - PAD_R;
      var plotH = waveCv.h - PAD_T - PAD_B;
      var totalMs = (CYCLES / freq) * 1000;
      return {
        plotW: plotW,
        plotH: plotH,
        totalMs: totalMs,
        xs: QV.scale([0, totalMs], [PAD_L, PAD_L + plotW]),
        ys: QV.scale([-6, 6], [PAD_T + plotH, PAD_T])
      };
    }

    function renderWaveLayer(t) {
      var c = t.colors;
      waveLayer.width = waveCv.el.width;
      waveLayer.height = waveCv.el.height;
      var ctx = waveLayerCtx;
      ctx.setTransform(waveCv.dpr, 0, 0, waveCv.dpr, 0, 0);
      geom = waveGeom();
      QV.axes(ctx, {
        x: PAD_L, y: PAD_T, w: geom.plotW, h: geom.plotH,
        xscale: geom.xs, yscale: geom.ys, theme: t,
        xlabel: "time (ms)", ylabel: "volts"
      });
      // Zero-volt line, slightly stronger than the grid.
      QV.curve(ctx, [[PAD_L, geom.ys(0)], [PAD_L + geom.plotW, geom.ys(0)]], {
        color: c.ink, width: 1, alpha: 0.25
      });
      // The waveform itself, in the audio color (it is an Audio signal, ±5 V).
      for (var i = 0; i < WAVE_POINTS; i++) {
        wavePts[i][0] = geom.xs((i * geom.totalMs) / WAVE_POINTS);
        wavePts[i][1] = geom.ys(waveBuf[i]);
      }
      QV.curve(ctx, wavePts, { color: c.audio, width: 2 });
      legend(ctx, PAD_L + 6, PAD_T + 4, [
        [shapeLabel(), c.audio],
        ["  ·  ", c.ink],
        bandlimited ? ["PolyBLEP", c.gate] : ["naive (aliases!)", c.cv]
      ], c.ink);
    }

    function blitWave(t) {
      var ctx = waveCv.ctx;
      waveCv.clear();
      ctx.save();
      ctx.setTransform(1, 0, 0, 1, 0, 0);
      ctx.drawImage(waveLayer, 0, 0);
      ctx.restore();
      drawPhaseDot(t);
    }

    // The dot the phasor drags along the trace: same phase, first cycle.
    function drawPhaseDot(t) {
      if (!geom) return;
      var c = t.colors;
      var ctx = waveCv.ctx;
      var idx = Math.floor(phasorPhase * WAVE_POINTS / CYCLES);
      if (idx < 0) idx = 0;
      if (idx >= WAVE_POINTS) idx = WAVE_POINTS - 1;
      var x = geom.xs((phasorPhase / CYCLES) * geom.totalMs);
      var y = geom.ys(waveBuf[idx]);
      ctx.save();
      // Drop line to 0 V ties the dot to "the output voltage right now".
      ctx.strokeStyle = c.mod;
      ctx.globalAlpha = 0.35;
      ctx.lineWidth = 1;
      ctx.beginPath();
      ctx.moveTo(x, y);
      ctx.lineTo(x, geom.ys(0));
      ctx.stroke();
      ctx.globalAlpha = 1;
      ctx.fillStyle = c.mod;
      ctx.beginPath();
      ctx.arc(x, y, 4.5, 0, TAU);
      ctx.fill();
      ctx.restore();
    }

    // ---- Phasor dial: phase runs around a circle at PHASOR_HZ -----------------
    function drawPhasor(t) {
      var c = t.colors;
      var ctx = phasorCv.ctx, w = phasorCv.w, h = phasorCv.h;
      phasorCv.clear();
      var cx = w / 2, cy = h / 2;
      var r = Math.min(w, h) / 2 - 14;
      if (r <= 6) return;
      // Square: shade the slice of the circle spent "high" (fraction pw).
      if (shape === "sqr") {
        var pwc = Math.min(0.95, Math.max(0.05, pw));
        ctx.save();
        ctx.globalAlpha = 0.12;
        ctx.fillStyle = c.audio;
        ctx.beginPath();
        ctx.moveTo(cx, cy);
        ctx.arc(cx, cy, r, 0, -pwc * TAU, true);
        ctx.closePath();
        ctx.fill();
        ctx.restore();
      }
      ctx.save();
      ctx.strokeStyle = c.ink;
      ctx.globalAlpha = 0.35;
      ctx.lineWidth = 1;
      ctx.beginPath();
      ctx.arc(cx, cy, r, 0, TAU);
      ctx.stroke();
      // Phase-zero tick at 3 o'clock, where each cycle begins.
      ctx.beginPath();
      ctx.moveTo(cx + r - 4, cy);
      ctx.lineTo(cx + r + 4, cy);
      ctx.stroke();
      ctx.restore();
      // Rotating radius + tip (counterclockwise, so sine's tip height IS its value).
      var ang = phasorPhase * TAU;
      var px = cx + r * Math.cos(ang);
      var py = cy - r * Math.sin(ang);
      QV.curve(ctx, [[cx, cy], [px, py]], { color: c.mod, width: 2 });
      ctx.save();
      ctx.fillStyle = c.mod;
      ctx.beginPath();
      ctx.arc(px, py, 4, 0, TAU);
      ctx.fill();
      ctx.font = "600 11px var(--mono-font, monospace)";
      ctx.textAlign = "center";
      ctx.textBaseline = "alphabetic";
      ctx.globalAlpha = 0.75;
      ctx.fillStyle = c.ink;
      ctx.fillText("phase " + phasorPhase.toFixed(2), cx, h - 4);
      ctx.restore();
    }

    // ---- Spectrum panel --------------------------------------------------------
    function drawSpec(t) {
      var c = t.colors;
      var ctx = specCv.ctx, w = specCv.w, h = specCv.h;
      specCv.clear();
      var plotW = w - PAD_L - PAD_R;
      var plotH = h - PAD_T - PAD_B;
      var xs = QV.logScale([FMIN, FMAX], [PAD_L, PAD_L + plotW]);
      var ys = QV.scale([DB_FLOOR, DB_TOP], [PAD_T + plotH, PAD_T]);
      QV.axes(ctx, {
        x: PAD_L, y: PAD_T, w: plotW, h: plotH,
        xscale: xs, yscale: ys, theme: t,
        xlabel: "frequency (Hz)", ylabel: "dB"
      });
      // Harmonic grid: dashed verticals at n*f0. With bandlimiting on, every
      // peak sits on one; naive aliases land in the gaps between them.
      var n = 1;
      while (n * freq <= FMAX && n <= 96) {
        var px = xs(n * freq);
        QV.curve(ctx, [[px, PAD_T], [px, PAD_T + plotH]], {
          color: c.voct, width: 1, dash: [3, 4], alpha: n === 1 ? 0.55 : 0.22
        });
        n++;
      }
      // Spectrum of the rendered buffer: filled + stroked in the audio color.
      var pts = [];
      var freqs = spec.freqs, db = spec.db;
      for (var k = 0; k < freqs.length; k++) {
        var f = freqs[k];
        if (f < FMIN || f > FMAX) continue;
        var d = db[k];
        if (d < DB_FLOOR) d = DB_FLOOR;
        if (d > DB_TOP) d = DB_TOP;
        pts.push([xs(f), ys(d)]);
      }
      if (pts.length) {
        ctx.save();
        ctx.globalAlpha = 0.14;
        ctx.fillStyle = c.audio;
        ctx.beginPath();
        ctx.moveTo(pts[0][0], ys(DB_FLOOR));
        for (var j = 0; j < pts.length; j++) ctx.lineTo(pts[j][0], pts[j][1]);
        ctx.lineTo(pts[pts.length - 1][0], ys(DB_FLOOR));
        ctx.closePath();
        ctx.fill();
        ctx.restore();
        QV.curve(ctx, pts, { color: c.audio, width: 1.6 });
      }
      // Theory target rings: the closed-form Fourier recipe of the current
      // shape, anchored to the MEASURED fundamental so theory and FFT share a
      // reference. Bandlimited stems hit the rings; naive ones miss/smear.
      var binHz = SR / FFT_SIZE;
      var kf = Math.round(freq / binHz);
      var fundDb = -Infinity;
      for (var kb = Math.max(1, kf - 2); kb <= Math.min(db.length - 1, kf + 2); kb++) {
        if (db[kb] > fundDb) fundDb = db[kb];
      }
      if (isFinite(fundDb)) {
        ctx.save();
        ctx.strokeStyle = c.voct;
        ctx.lineWidth = 1.4;
        ctx.globalAlpha = 0.95;
        var m = 1;
        while (m * freq <= FMAX && m <= 96) {
          var rel = theoryRel(shape, m, pw);
          if (rel > 1e-4) {
            var tdb = fundDb + 20 * Math.log10(rel);
            if (tdb > DB_FLOOR + 1) {
              if (tdb > DB_TOP) tdb = DB_TOP;
              ctx.beginPath();
              ctx.arc(xs(m * freq), ys(tdb), 3.2, 0, TAU);
              ctx.stroke();
            }
          }
          m++;
        }
        ctx.restore();
      }
      legend(ctx, PAD_L + 6, PAD_T + 4, [
        ["spectrum", c.audio],
        ["  ·  ", c.ink],
        ["○ theory @ n·f₀", c.voct]
      ], c.ink);
    }

    // ---- Theme + init ----------------------------------------------------------
    QV.onThemeChange(function (t) {
      th = t || QV.theme();
      draw();
    });
    recomputeNow();       // synchronous first paint (later updates coalesce)
    phasorLoop.play();    // no-op under prefers-reduced-motion (QV.loop)
    syncPhasorBtn();
  });
})();
