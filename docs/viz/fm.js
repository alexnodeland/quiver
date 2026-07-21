/*
 * docs/viz/fm.js — "Sidebands from Nothing"
 *
 * Two-operator FM, fully touchable. A pure sine carrier (audio blue) is
 * frequency-modulated by a pure sine modulator (mod violet, ghosted in the
 * time plot). The spectrum of the very buffer you hear (audio blue) is
 * overlaid with the PREDICTED sideband positions and heights — stems at
 * fc ± k·fm scaled by |J_k(I)| (voct yellow) — so prediction and measurement
 * sit on the same axes in two colors.
 *
 * Two companion panels close the loop:
 *   · Bessel family — J_0..J_4 versus index I with a cursor at the current I:
 *     the stem heights above are literally these curves sampled at the cursor.
 *     Dots mark J_0's zeros (2.405, 5.520), where the carrier vanishes.
 *   · Index waterfall — a spectrogram strip: while "bloom" breathes the index
 *     it SCROLLS (columns shift left as new ones append, fixed width); the
 *     waterfall button (and bloom under reduced motion) fills it instantly
 *     with x = index sweep. y = log frequency, color = predicted |J_k(I)|.
 *
 * The synthesis is the classic phase-modulation form,
 *   y(t) = 5·sin(2π·fc·t + I·sin(2π·fm·t))  volts,
 * which has exactly the sideband spectrum of sinusoidal linear FM with peak
 * deviation ΔF = I·fm. On Quiver's Vco the linear through-zero path is
 * freq += (fm_lin/5)·base, so a modulator of depth_V volts peak gives
 * ΔF = (depth_V/5)·fc and thus I = (depth_V/5)·(fc/fm). The page derives it.
 *
 * Scheduling: scrub-driven recomputes (synthesis + FFT + Bessel row) are
 * rAF-coalesced via QV.coalesce; the looped audio restart is debounced via
 * QV.debounce so dragging never machine-guns the AudioContext. All hot-path
 * Float32Arrays are allocated once and reused.
 *
 * Self-contained ES5 IIFE. Consumes window.QuiverViz (loaded first).
 */
(function () {
  "use strict";
  if (typeof window === "undefined" || !window.QuiverViz) return;
  var QV = window.QuiverViz;

  var TAU = QV.dsp.TAU;
  var PEAK = QV.dsp.AUDIO_PEAK_V; // ±5 V audio convention
  var SR = 44100;
  var FFT_N = 8192; // analysis buffer & FFT size (~5.4 Hz bins)

  // Shared spectral window (spectrum plot + waterfall strip use the same).
  var FLO = 20;
  var FHI = 20000;
  var DBLO = -80;
  var DBHI = 6;
  var K_MAX = 48; // widest sideband order the widget ever draws

  // ---- Bessel J_k of the first kind, by the ascending series ---------------
  // J_k(x) = Σ_{m=0..25} (−1)^m / (m!·(m+k)!) · (x/2)^{2m+k}, computed with
  // the term recurrence t_{m} = −t_{m−1}·(x/2)²/(m(m+k)) so nothing overflows.
  // Plenty accurate for x ≤ 12, k ≤ 48 (the widget's whole range).
  function besselJ(k, x) {
    k = Math.abs(k);
    var half = x / 2;
    var t = 1;
    for (var i = 1; i <= k; i++) t *= half / i;
    var sum = t;
    for (var m = 1; m <= 25; m++) {
      t *= -(half * half) / (m * (m + k));
      sum += t;
    }
    return sum;
  }

  function gcd(a, b) {
    a = Math.abs(Math.round(a));
    b = Math.abs(Math.round(b));
    while (b) {
      var r = a % b;
      a = b;
      b = r;
    }
    return a || 1;
  }

  // ---- Two-operator FM by phase accumulation (mirrors Vco's tick loop) -----
  // One phase accumulator per operator; the modulator's value bends the
  // carrier's phase. Writes n samples in VOLTS (±5) into the CALLER-OWNED
  // buffer `out` (no allocation in the hot path).
  function renderPM(fcHz, fmHz, index, out, n) {
    var dc = (TAU * fcHz) / SR;
    var dm = (TAU * fmHz) / SR;
    var pc = 0;
    var pm = 0;
    for (var i = 0; i < n; i++) {
      out[i] = Math.sin(pc + index * Math.sin(pm)) * PEAK;
      pc += dc;
      pm += dm;
      if (pc > TAU) pc -= TAU;
      if (pm > TAU) pm -= TAU;
    }
    return out;
  }

  // Parse "#rrggbb" or "rgb()/rgba()" to [r, g, b] for manual alpha columns.
  function colorToRgb(str) {
    str = (str || "").trim();
    var m = /^#?([0-9a-f]{6})$/i.exec(str);
    if (m) {
      var n = parseInt(m[1], 16);
      return [(n >> 16) & 255, (n >> 8) & 255, n & 255];
    }
    var rgb = /rgba?\(([^)]+)\)/.exec(str);
    if (rgb) {
      var parts = rgb[1].split(",");
      return [parseInt(parts[0], 10), parseInt(parts[1], 10), parseInt(parts[2], 10)];
    }
    return [9, 105, 218]; // light-theme audio blue fallback
  }

  QV.register("fm", function (root, QV) {
    // Scheduling helpers (with tiny fallbacks if the engine copy predates them).
    var coalesce = QV.coalesce || function (fn) {
      var scheduled = false;
      return function () {
        if (scheduled) return;
        scheduled = true;
        window.requestAnimationFrame(function () {
          scheduled = false;
          fn();
        });
      };
    };
    var debounce = QV.debounce || function (fn, ms) {
      var timer = null;
      return function () {
        if (timer) clearTimeout(timer);
        timer = setTimeout(function () {
          timer = null;
          fn();
        }, ms || 120);
      };
    };

    // ---- DOM shell: controls / canvases / instruction / readouts / hint ----
    var controls = QV.el("div", "qv-controls", root);
    var timeWrap = QV.el("div", null, root);
    var specWrap = QV.el("div", null, root);
    specWrap.style.marginTop = "6px";
    var besWrap = QV.el("div", null, root);
    besWrap.style.marginTop = "6px";
    var wfWrap = QV.el("div", null, root);
    wfWrap.style.marginTop = "6px";
    var instruction = QV.el("div", "qv-instruction", root);
    instruction.textContent =
      "Scrub the ratio, index, and pitch numbers in the text below. " +
      "Press bloom to make the index breathe 0 ↔ target — sidebands grow " +
      "and recede in Bessel order while the waterfall strip scrolls the sweep. " +
      "Press it again to stop.";
    var readouts = QV.el("div", "qv-readouts", root);
    var hint = QV.el("div", "qv-hint", root);
    hint.textContent =
      "scrub the index to 2.4 — J₀(2.4) ≈ 0 and the carrier vanishes " +
      "from its own spectrum while the sidebands stay. Watch the blue " +
      "curve cross zero on the Bessel panel at the same moment.";

    // ---- State --------------------------------------------------------------
    var ratio = 2; // fm : fc, step 0.25
    var index = 3; // modulation index I (target)
    var pitchV = 0; // carrier V/Oct volts (0 V = C4)
    var animIndex = index; // index actually drawn (bloom tweens this)
    var playing = false;

    function fcHz() {
      return QV.dsp.voctToHz(pitchV);
    }
    function fmHz() {
      return ratio * fcHz();
    }

    // ---- Reused buffers & Bessel caches -------------------------------------
    var abuf = new Float32Array(FFT_N); // analysis tone (volts), reused
    var spec = null; // {freqs, db} of abuf
    var modScratch = new Float32Array(FFT_N); // modulator ghost, reused
    var audioScratch = new Float32Array(0); // grow-only audio render buffer

    // J_k(I) for k = 0..K_MAX at ONE cached I — recomputed only when I moves,
    // shared by the spectrum stems, the Bessel-panel cursor dots, and the
    // waterfall columns (never per stem draw).
    var besCache = new Float32Array(K_MAX + 1);
    var besCacheI = NaN;
    function besselRow(I) {
      if (besCacheI !== I) {
        for (var k = 0; k <= K_MAX; k++) besCache[k] = besselJ(k, I);
        besCacheI = I;
      }
      return besCache;
    }

    // Static family curves J_0..J_4 over I in [0, 12] — computed exactly once.
    var BES_I_MAX = 12;
    var BES_STEP = 0.05;
    var BES_N = Math.round(BES_I_MAX / BES_STEP) + 1;
    var besFamily = [];
    for (var bk = 0; bk <= 4; bk++) {
      var fam = new Float32Array(BES_N);
      for (var bi = 0; bi < BES_N; bi++) fam[bi] = besselJ(bk, bi * BES_STEP);
      besFamily.push(fam);
    }
    var J0_ZEROS = [2.405, 5.52]; // first two zeros of J_0 — carrier blackouts

    function compute() {
      renderPM(fcHz(), fmHz(), animIndex, abuf, FFT_N);
      spec = QV.dsp.spectrumDb(abuf, { sampleRate: SR, size: FFT_N });
      besselRow(animIndex); // warm the cache once per recompute, not per stem
    }

    // ---- Canvases -----------------------------------------------------------
    var ready = false;
    var timeCv = QV.canvas(timeWrap, {
      height: 160,
      onResize: function () {
        if (ready) draw();
      }
    });
    var specCv = QV.canvas(specWrap, {
      height: 250,
      onResize: function () {
        if (ready) draw();
      }
    });
    var besCv = QV.canvas(besWrap, {
      height: 150,
      onResize: function () {
        if (ready) draw();
      }
    });
    var wfCv = QV.canvas(wfWrap, {
      height: 120,
      onResize: function () {
        if (ready) drawWaterfall();
      }
    });
    var wfCaption = QV.el("div", "qv-caption", wfWrap);
    wfCaption.textContent =
      "index waterfall — each column is the predicted spectrum at one I of the sweep";

    // ---- Audio: the plotted math IS the played buffer -----------------------
    // Loop length is trimmed to a whole number of carrier cycles, in a
    // multiple of 4 so the modulator (ratio is a multiple of 0.25) also
    // completes whole cycles — a near-seamless ~1 s loop.
    function renderAudio(atIndex) {
      var fc = fcHz();
      var cycles = Math.max(4, Math.round(fc / 4) * 4);
      var n = Math.round((cycles / fc) * SR);
      if (audioScratch.length < n) audioScratch = new Float32Array(n);
      var view = audioScratch.subarray(0, n);
      renderPM(fc, fmHz(), atIndex, view, n);
      return view;
    }
    function startAudio(atIndex) {
      if (atIndex == null) atIndex = index;
      var handle = QV.audio.play(renderAudio(atIndex), SR, { loop: true, gain: 0.2 });
      playing = !!handle;
      syncHearLabel();
    }
    function stopAudio() {
      QV.audio.stop();
      playing = false;
      syncHearLabel();
    }

    // ---- Bloom: breathe the index 0 ↔ target until toggled off ---------------
    // The index rides a raised cosine 0 → target → 0 (~2.4 s per cycle),
    // CONTINUOUSLY, until the button — now reading "■ bloom" — is pressed
    // again. The target is the scrub's CURRENT value, read live each frame,
    // so scrubbing mid-breathe retargets the oscillation instead of killing
    // it. Visuals animate every frame; while breathing the waterfall becomes
    // a scrolling strip; if audio is playing it re-renders at most every
    // ~150 ms, never per frame.
    var BREATHE_PERIOD = 2.4; // seconds, 0 -> target -> 0
    var breathing = false;
    var breatheT = 0;
    var bloomAudioMs = 0;
    var loopApi = QV.loop(root, function (dt) {
      if (!breathing) {
        loopApi.pause();
        return;
      }
      if (dt > 0.1) dt = 0.1;
      breatheT += dt;
      var u = (breatheT / BREATHE_PERIOD) % 1;
      // raised cosine: 0 -> target -> 0, smooth at both turnarounds
      animIndex = index * 0.5 * (1 - Math.cos(TAU * u));
      compute();
      updateReadouts();
      draw();
      wfAppend(animIndex);
      if (playing) {
        var now = Date.now();
        if (now - bloomAudioMs >= 150) {
          bloomAudioMs = now;
          startAudio(animIndex);
        }
      }
    });

    function bloom() {
      if (loopApi.reduced) {
        // Reduced motion: no animation — settled spectrum + instant waterfall.
        cancelBloom();
        refresh(false);
        wfInstant();
        return;
      }
      if (breathing) {
        stopBreathe();
        return;
      }
      breathing = true;
      breatheT = 0;
      animIndex = 0;
      wfMode = "scroll";
      wfColumns.length = 0;
      wfTarget = index;
      drawWaterfall();
      bloomAudioMs = Date.now();
      syncBloomLabel();
      loopApi.play();
    }
    // Toggle-off: settle everything back on the scrub's target index. The
    // scrolled strip stays frozen on screen.
    function stopBreathe() {
      breathing = false;
      animIndex = index;
      syncBloomLabel();
      refresh(true); // restart the audio loop (if on) at the target index
    }
    function cancelBloom() {
      breathing = false;
      animIndex = index;
      wfMode = "sweep";
      syncBloomLabel();
    }

    // Recompute + repaint after any parameter change. Restarts the audio loop
    // when it is playing so what you hear always matches what you see.
    function refresh(retrigger) {
      compute();
      updateReadouts();
      draw();
      if (retrigger && playing) startAudio();
    }

    // Scrub-driven path: recompute+redraw at most once per animation frame,
    // audio restart only after the drag pauses (~120 ms trailing edge).
    var scheduleRefresh = coalesce(function () {
      compute();
      updateReadouts();
      draw();
    });
    var scheduleAudio = debounce(function () {
      if (playing) startAudio();
    }, 120);
    function paramChanged() {
      // While breathing, the oscillation simply tracks the new target: the
      // loop recomputes everything (audio included) on its own clock, so
      // scheduling a second refresh/audio path here would fight it.
      if (breathing) return;
      scheduleRefresh();
      scheduleAudio();
    }

    // ---- Controls -----------------------------------------------------------
    var HEAR = "▶ hear it";
    var STOP = "■ stop";
    var btnRoot = QV.buttons(controls, [
      {
        label: HEAR,
        primary: true,
        title: "Loop the exact tone in the plots (gain 0.2 — FM gets bright)",
        onClick: function () {
          if (playing) stopAudio();
          else startAudio();
        }
      },
      {
        label: "bloom",
        title: "Breathe the index 0 ↔ its target — sidebands grow and recede until you press again",
        onClick: bloom
      },
      {
        label: "waterfall",
        title: "Fill the waterfall strip instantly from 40 precomputed steps of the sweep",
        onClick: function () {
          cancelBloom();
          refresh(false);
          wfInstant();
        }
      }
    ]);
    var hearBtn = btnRoot.qvButtons[HEAR];
    function syncHearLabel() {
      if (hearBtn) hearBtn.textContent = playing ? STOP : HEAR;
    }
    var bloomBtn = btnRoot.qvButtons["bloom"];
    function syncBloomLabel() {
      if (bloomBtn) bloomBtn.textContent = breathing ? "■ bloom" : "bloom";
    }

    // ---- Readouts -----------------------------------------------------------
    var roFc = QV.readout(readouts, { label: "carrier fc" });
    var roFm = QV.readout(readouts, { label: "modulator fm" });
    var roIdx = QV.readout(readouts, { label: "index I" });
    var roBw = QV.readout(readouts, { label: "Carson BW ≈ 2(I+1)fm" });
    var roChar = QV.readout(readouts, { label: "character" });

    // Ratio is quantized to quarters, so write it as a reduced fraction n/d.
    // Small denominator ⇒ sidebands land on a harmonic series ⇒ "harmonic".
    function ratioFraction() {
      var q = Math.round(ratio * 4); // ratio in quarter units
      var g = gcd(q, 4);
      return { n: q / g, d: 4 / g };
    }
    function updateReadouts() {
      var fc = fcHz();
      var fm = fmHz();
      roFc.set(fc.toFixed(1) + " Hz  (" + QV.dsp.voctToNote(pitchV) + ")", "audio");
      roFm.set(fm.toFixed(1) + " Hz", "mod");
      roIdx.set(animIndex.toFixed(1), "voct");
      roBw.set(QV.fmtHz(2 * (animIndex + 1) * fm) + "Hz", "audio");
      var fr = ratioFraction();
      if (fr.d === 1) roChar.set("harmonic (" + fr.n + ":1)", "gate");
      else if (fr.d === 2) roChar.set("harmonic (" + fr.n + ":2, sub-octave)", "gate");
      else roChar.set("inharmonic (" + fr.n + ":" + fr.d + ")", "cv");
    }

    // ---- Prose scrubs (ratio, index, pitch live inside the sentences) -------
    var ratioEl = document.getElementById("qv-fm-ratio");
    var indexEl = document.getElementById("qv-fm-index");
    var pitchEl = document.getElementById("qv-fm-pitch");
    if (ratioEl) {
      QV.scrub(ratioEl, {
        min: 0.25,
        max: 8,
        step: 0.25,
        value: ratio,
        fmt: function (v) {
          return v.toFixed(2).replace(/\.?0+$/, "") + "×";
        },
        onInput: function (v) {
          ratio = v;
          paramChanged();
        }
      });
    }
    if (indexEl) {
      QV.scrub(indexEl, {
        min: 0,
        max: 12,
        step: 0.1,
        value: index,
        fmt: function (v) {
          return v.toFixed(1);
        },
        onInput: function (v) {
          index = v;
          animIndex = v;
          paramChanged();
        }
      });
    }
    if (pitchEl) {
      QV.scrub(pitchEl, {
        min: -1,
        max: 2,
        step: 0.05,
        value: pitchV,
        fmt: function (v) {
          return v.toFixed(2) + " V";
        },
        onInput: function (v) {
          pitchV = v;
          paramChanged();
        }
      });
    }

    // ---- Drawing -------------------------------------------------------------

    function draw() {
      drawTime();
      drawSpectrum();
      drawBessel();
    }

    // (a) Time domain: the carrier's waveform with the modulator ghosted.
    function drawTime() {
      var ctx = timeCv.ctx;
      var w = timeCv.w;
      var h = timeCv.h;
      var t = QV.theme();
      var c = t.colors;
      timeCv.clear();

      var padL = 40;
      var padR = 10;
      var padT = 8;
      var padB = 20;
      var x0 = padL;
      var x1 = w - padR;
      var y0 = padT;
      var y1 = h - padB;

      // Window: a few periods of whichever oscillator is slower, so both the
      // carrier wiggle and the modulator's slow bend are visible.
      var slower = Math.min(fcHz(), fmHz());
      var winSec = 3 / slower;
      if (winSec < 0.003) winSec = 0.003;
      if (winSec > 0.04) winSec = 0.04;
      var winN = Math.min(FFT_N, Math.round(winSec * SR));
      var winMs = (winN / SR) * 1000;

      var xms = QV.scale([0, winMs], [x0, x1]);
      var ysc = QV.scale([-6, 6], [y1, y0]);
      QV.axes(ctx, {
        x: x0,
        y: y0,
        w: x1 - x0,
        h: y1 - y0,
        xscale: xms,
        yscale: ysc,
        theme: t,
        xfmt: function (v) {
          return Math.round(v * 10) / 10 + "ms";
        }
      });

      var xi = QV.scale([0, winN - 1], [x0, x1]);

      // Modulator ghost (violet, dashed): the pure sine doing the bending.
      // Reuses the module-level scratch buffer — no per-draw allocation.
      var mbuf = modScratch.subarray(0, winN);
      var dm = (TAU * fmHz()) / SR;
      var pm = 0;
      for (var i = 0; i < winN; i++) {
        mbuf[i] = Math.sin(pm) * PEAK;
        pm += dm;
        if (pm > TAU) pm -= TAU;
      }
      QV.wave(ctx, mbuf, { xscale: xi, yscale: ysc, color: c.mod, width: 1.5, dash: [4, 4], alpha: 0.45 });

      // Carrier (blue): the same buffer the spectrum below is computed from.
      if (abuf) {
        QV.wave(ctx, abuf.subarray(0, winN), { xscale: xi, yscale: ysc, color: c.audio, width: 2 });
      }

      drawLegend(ctx, x0 + 6, y0 + 4, [
        ["carrier", c.audio],
        ["   modulator (bends its pitch)", c.mod]
      ], c);
    }

    // (b) Spectrum: measured curve (blue) + predicted |J_k(I)| stems (yellow).
    function drawSpectrum() {
      var ctx = specCv.ctx;
      var w = specCv.w;
      var h = specCv.h;
      var t = QV.theme();
      var c = t.colors;
      specCv.clear();

      var padL = 44;
      var padR = 10;
      var padT = 10;
      var padB = 22;
      var x0 = padL;
      var x1 = w - padR;
      var y0 = padT;
      var y1 = h - padB;

      var xsc = QV.logScale([FLO, FHI], [x0, x1]);
      var ysc = QV.scale([DBLO, DBHI], [y1, y0]);
      QV.axes(ctx, {
        x: x0,
        y: y0,
        w: x1 - x0,
        h: y1 - y0,
        xscale: xsc,
        yscale: ysc,
        ylabel: "dB",
        theme: t
      });

      // Where the carrier sits — the landmark for watching J0(I) hit zero.
      var fc = fcHz();
      if (fc > FLO && fc < FHI) {
        var fx = xsc(fc);
        ctx.save();
        ctx.strokeStyle = c.ink;
        ctx.globalAlpha = 0.3;
        ctx.setLineDash([3, 4]);
        ctx.beginPath();
        ctx.moveTo(fx, y0);
        ctx.lineTo(fx, y1);
        ctx.stroke();
        ctx.setLineDash([]);
        ctx.globalAlpha = 0.6;
        ctx.fillStyle = c.ink;
        ctx.font = "10px var(--mono-font, monospace)";
        ctx.textAlign = "center";
        ctx.textBaseline = "bottom";
        ctx.fillText("fc", fx, y0 - 1);
        ctx.restore();
      }

      // Predicted sidebands: stems at fc ± k·fm with height |J_k(I)|, read
      // from the cached Bessel row (computed once per index change).
      // Components pushed below 0 Hz by through-zero FM reflect to |f|.
      var fm = fmHz();
      var bes = besselRow(animIndex);
      var K = Math.min(K_MAX, Math.ceil(animIndex + 10));
      var stemPts = [];
      for (var k = -K; k <= K; k++) {
        var f = fc + k * fm;
        var ff = Math.abs(f); // negative-frequency reflection
        if (ff < FLO || ff > FHI) continue;
        var amp = Math.abs(bes[k < 0 ? -k : k]);
        var db = 20 * Math.log10(amp + 1e-12);
        if (db <= DBLO) continue;
        stemPts.push([xsc(ff), ysc(DBLO), ysc(Math.min(db, DBHI))]);
      }
      QV.stems(ctx, stemPts, { color: c.voct, width: 2.5, alpha: 0.9 });

      // Measured spectrum of the buffer above (and of what "hear it" plays).
      if (spec) {
        var pts = [];
        for (var i = 1; i < spec.freqs.length; i++) {
          var fq = spec.freqs[i];
          if (fq < FLO || fq > FHI) continue;
          var d = spec.db[i];
          if (d < DBLO) d = DBLO;
          if (d > DBHI) d = DBHI;
          pts.push([xsc(fq), ysc(d)]);
        }
        QV.curve(ctx, pts, { color: c.audio, width: 1.5, alpha: 0.9 });
      }

      drawLegend(ctx, x0 + 6, y0 + 4, [
        ["measured", c.audio],
        ["   predicted fc ± k·fm, height |Jₖ(I)|", c.voct]
      ], c);
    }

    // (c) Bessel family: J_0..J_4 versus I, cursor at the current index.
    // The spectrum's stem heights are these curves sampled at the cursor.
    function drawBessel() {
      var ctx = besCv.ctx;
      var w = besCv.w;
      var h = besCv.h;
      var t = QV.theme();
      var c = t.colors;
      besCv.clear();

      var padL = 44;
      var padR = 10;
      var padT = 8;
      var padB = 18;
      var x0 = padL;
      var x1 = w - padR;
      var y0 = padT;
      var y1 = h - padB;

      var xsc = QV.scale([0, BES_I_MAX], [x0, x1]);
      var ysc = QV.scale([-0.5, 1], [y1, y0]);
      QV.axes(ctx, {
        x: x0,
        y: y0,
        w: x1 - x0,
        h: y1 - y0,
        xscale: xsc,
        yscale: ysc,
        theme: t,
        xfmt: function (v) {
          return String(v);
        }
      });

      // Emphasized zero line — the J0 zero dots live on it.
      var yZero = ysc(0);
      ctx.save();
      ctx.strokeStyle = c.ink;
      ctx.globalAlpha = 0.25;
      ctx.beginPath();
      ctx.moveTo(x0, yZero);
      ctx.lineTo(x1, yZero);
      ctx.stroke();
      ctx.restore();

      // J1..J4 thin, in ink; J0 highlighted in audio blue.
      var pts;
      var i;
      for (var k = 4; k >= 1; k--) {
        pts = [];
        for (i = 0; i < BES_N; i++) pts.push([xsc(i * BES_STEP), ysc(besFamily[k][i])]);
        QV.curve(ctx, pts, { color: c.ink, width: 1, alpha: 0.4 });
      }
      pts = [];
      for (i = 0; i < BES_N; i++) pts.push([xsc(i * BES_STEP), ysc(besFamily[0][i])]);
      QV.curve(ctx, pts, { color: c.audio, width: 2 });

      // J0's zeros: park I here and the carrier's own stem vanishes.
      ctx.save();
      ctx.fillStyle = c.audio;
      ctx.font = "9px var(--mono-font, monospace)";
      ctx.textAlign = "center";
      ctx.textBaseline = "top";
      for (i = 0; i < J0_ZEROS.length; i++) {
        var zx = xsc(J0_ZEROS[i]);
        ctx.beginPath();
        ctx.arc(zx, yZero, 3, 0, TAU);
        ctx.fill();
        ctx.globalAlpha = 0.7;
        ctx.fillText(J0_ZEROS[i].toFixed(3), zx, yZero + 5);
        ctx.globalAlpha = 1;
      }
      ctx.restore();

      // Cursor at the current (animated) index.
      var I = animIndex < 0 ? 0 : animIndex > BES_I_MAX ? BES_I_MAX : animIndex;
      var cx = xsc(I);
      ctx.save();
      ctx.strokeStyle = c.voct;
      ctx.globalAlpha = 0.8;
      ctx.setLineDash([3, 3]);
      ctx.beginPath();
      ctx.moveTo(cx, y0);
      ctx.lineTo(cx, y1);
      ctx.stroke();
      ctx.setLineDash([]);
      // Sample dots: the k = 0..4 stem heights, read off the curves.
      var bes = besselRow(animIndex);
      ctx.fillStyle = c.voct;
      for (var kk = 0; kk <= 4; kk++) {
        var v = bes[kk];
        if (v < -0.5) v = -0.5;
        if (v > 1) v = 1;
        ctx.beginPath();
        ctx.arc(cx, ysc(v), 2.5, 0, TAU);
        ctx.fill();
      }
      ctx.restore();

      drawLegend(ctx, x0 + 6, y0 + 2, [
        ["J₀", c.audio],
        ["  J₁…J₄", c.ink],
        ["  ● stem heights at I", c.voct]
      ], c);
    }

    // (d) Index waterfall: a spectrogram strip of the bloom sweep.
    // x = index (0 → target), y = log frequency, color = predicted |J_k(I)|
    // painted as audio-colored columns at varying alpha. Columns are stored
    // (I-fraction + per-row alphas) so theme changes and resizes replay them.
    var WF_ROWS = 48; // logical rows, independent of pixel size
    var WF_MAX_COLS = 180;
    var wfColumns = []; // [{t: 0..1, a: Float32Array(WF_ROWS)}]
    var wfTarget = index; // the index the recorded sweep runs to
    // "sweep": x = index 0..target (instant fill / reduced motion).
    // "scroll": x = time; the strip holds WF_MAX_COLS columns and shifts
    // left as breathing appends new ones (newest at the right edge).
    var wfMode = "sweep";

    function wfGeom() {
      return { x0: 44, x1: wfCv.w - 10, y0: 6, y1: wfCv.h - 18 };
    }

    // One column of the strip: max-normalized dB of every predicted sideband
    // that lands in each log-frequency row bucket.
    function wfColumnAlphas(I) {
      var a = new Float32Array(WF_ROWS);
      var fc = fcHz();
      var fm = fmHz();
      var bes = besselRow(I);
      var K = Math.min(K_MAX, Math.ceil(I + 10));
      var lgLo = Math.log(FLO);
      var lgSpan = Math.log(FHI) - lgLo;
      for (var k = -K; k <= K; k++) {
        var f = Math.abs(fc + k * fm);
        if (f < FLO || f > FHI) continue;
        var amp = Math.abs(bes[k < 0 ? -k : k]);
        var db = 20 * Math.log10(amp + 1e-12);
        if (db <= DBLO) continue;
        var rel = (Math.log(f) - lgLo) / lgSpan;
        var r = Math.round((1 - rel) * (WF_ROWS - 1));
        if (r < 0) r = 0;
        if (r >= WF_ROWS) r = WF_ROWS - 1;
        var v = (Math.min(db, DBHI) - DBLO) / (DBHI - DBLO);
        if (v > a[r]) a[r] = v;
      }
      return a;
    }

    // Paint columns [from..end] — heatmap-style manual columns in the audio
    // color at varying alpha. Incremental appends paint only the new column.
    function wfPaintColumns(from) {
      var g = wfGeom();
      var plotW = g.x1 - g.x0;
      var plotH = g.y1 - g.y0;
      if (plotW <= 0 || plotH <= 0) return;
      var rgb = colorToRgb(QV.theme().colors.audio);
      var rowH = plotH / WF_ROWS;
      // The axis domain is [0, max(wfTarget, 0.5)]; columns are stored as a
      // fraction of wfTarget, so scale them onto the same axis (otherwise a
      // sweep to I < 0.5 paints past its own tick marks).
      var span = wfTarget > 0 ? wfTarget / Math.max(wfTarget, 0.5) : 0;
      var ctx = wfCv.ctx;
      ctx.save();
      for (var i = from; i < wfColumns.length; i++) {
        var col = wfColumns[i];
        var tPrev = i > 0 ? wfColumns[i - 1].t : 0;
        var xa = g.x0 + tPrev * span * plotW;
        var xb = g.x0 + col.t * span * plotW;
        if (xb - xa < 1) xb = xa + 1;
        if (xb > g.x1) xb = g.x1;
        if (xb <= xa) continue;
        for (var r = 0; r < WF_ROWS; r++) {
          var alpha = col.a[r];
          if (alpha <= 0.02) continue;
          if (alpha > 1) alpha = 1;
          ctx.fillStyle =
            "rgba(" + rgb[0] + "," + rgb[1] + "," + rgb[2] + "," + alpha * 0.95 + ")";
          ctx.fillRect(xa, g.y0 + r * rowH, xb - xa, rowH + 0.5);
        }
      }
      ctx.restore();
    }

    // Full repaint: frame, index ticks (sweep mode only — a scrolling strip
    // has a time axis, not an index axis), log-frequency guides, then columns.
    function drawWaterfall() {
      var ctx = wfCv.ctx;
      var t = QV.theme();
      var c = t.colors;
      wfCv.clear();
      var g = wfGeom();
      if (wfMode === "scroll") {
        QV.axes(ctx, { x: g.x0, y: g.y0, w: g.x1 - g.x0, h: g.y1 - g.y0, theme: t });
      } else {
        var xsc = QV.scale([0, Math.max(wfTarget, 0.5)], [g.x0, g.x1]);
        QV.axes(ctx, {
          x: g.x0,
          y: g.y0,
          w: g.x1 - g.x0,
          h: g.y1 - g.y0,
          xscale: xsc,
          theme: t,
          xfmt: function (v) {
            return "I=" + (Math.round(v * 10) / 10);
          }
        });
      }
      // Log-frequency guides (same window as the spectrum plot above).
      var lgLo = Math.log(FLO);
      var lgSpan = Math.log(FHI) - lgLo;
      var marks = [100, 1000, 10000];
      ctx.save();
      ctx.font = "10px var(--mono-font, monospace)";
      ctx.textAlign = "right";
      ctx.textBaseline = "middle";
      for (var i = 0; i < marks.length; i++) {
        var my = g.y1 - ((Math.log(marks[i]) - lgLo) / lgSpan) * (g.y1 - g.y0);
        ctx.strokeStyle = c.grid;
        ctx.beginPath();
        ctx.moveTo(g.x0, my);
        ctx.lineTo(g.x1, my);
        ctx.stroke();
        ctx.fillStyle = c.ink;
        ctx.globalAlpha = 0.7;
        ctx.fillText(QV.fmtHz(marks[i]), g.x0 - 4, my);
        ctx.globalAlpha = 1;
      }
      ctx.restore();
      if (wfColumns.length) {
        if (wfMode === "scroll") wfPaintScroll();
        else wfPaintColumns(0);
      } else {
        ctx.save();
        ctx.fillStyle = c.ink;
        ctx.globalAlpha = 0.5;
        ctx.font = "11px var(--mono-font, monospace)";
        ctx.textAlign = "center";
        ctx.textBaseline = "middle";
        ctx.fillText(
          "press bloom or waterfall to record the index sweep",
          (g.x0 + g.x1) / 2,
          (g.y0 + g.y1) / 2
        );
        ctx.restore();
      }
    }

    // Scroll-mode paint: uniform-width columns, newest at the right edge.
    function wfPaintScroll() {
      var g = wfGeom();
      var plotW = g.x1 - g.x0;
      var plotH = g.y1 - g.y0;
      if (plotW <= 0 || plotH <= 0) return;
      var rgb = colorToRgb(QV.theme().colors.audio);
      var rowH = plotH / WF_ROWS;
      var colW = plotW / WF_MAX_COLS;
      var ctx = wfCv.ctx;
      ctx.save();
      for (var i = 0; i < wfColumns.length; i++) {
        var xa = g.x1 - (wfColumns.length - i) * colW;
        var xb = xa + (colW < 1 ? 1 : colW);
        if (xb <= g.x0) continue;
        if (xa < g.x0) xa = g.x0;
        var col = wfColumns[i];
        for (var r = 0; r < WF_ROWS; r++) {
          var alpha = col.a[r];
          if (alpha <= 0.02) continue;
          if (alpha > 1) alpha = 1;
          ctx.fillStyle =
            "rgba(" + rgb[0] + "," + rgb[1] + "," + rgb[2] + "," + alpha * 0.95 + ")";
          ctx.fillRect(xa, g.y0 + r * rowH, xb - xa, rowH + 0.5);
        }
      }
      ctx.restore();
    }

    function wfReset(target) {
      wfColumns.length = 0;
      wfTarget = target;
      wfMode = "sweep";
      drawWaterfall();
    }

    // Append one column. Breathing ("scroll") holds a fixed width and shifts
    // everything left — a full repaint each append. The recorded sweep
    // ("sweep") fills left-to-right with incremental paint, and stops full.
    function wfAppend(I) {
      if (wfMode === "scroll") {
        if (wfColumns.length >= WF_MAX_COLS) wfColumns.shift();
        wfColumns.push({ t: 1, a: wfColumnAlphas(I) });
        drawWaterfall();
        return;
      }
      if (wfColumns.length >= WF_MAX_COLS) return;
      var tt = wfTarget > 0 ? I / wfTarget : 1;
      if (tt > 1) tt = 1;
      wfColumns.push({ t: tt, a: wfColumnAlphas(I) });
      // First column: full repaint to clear the empty-state hint text.
      if (wfColumns.length === 1) drawWaterfall();
      else wfPaintColumns(wfColumns.length - 1);
    }

    // Instant waterfall: ~40 precomputed steps, rendered in one pass. Used by
    // the waterfall button and by bloom under prefers-reduced-motion.
    function wfInstant() {
      wfReset(index);
      var STEPS = 40;
      for (var s = 0; s < STEPS; s++) {
        var tt = STEPS === 1 ? 1 : s / (STEPS - 1);
        wfColumns.push({ t: tt, a: wfColumnAlphas(index * tt) });
      }
      besselRow(animIndex); // leave the cache on the live index, not a step
      drawWaterfall(); // full repaint (clears the empty-state hint text)
    }

    function drawLegend(ctx, x, y, parts, colors) {
      ctx.save();
      ctx.font = "600 11px var(--mono-font, monospace)";
      ctx.textAlign = "left";
      ctx.textBaseline = "top";
      var cx = x;
      for (var i = 0; i < parts.length; i++) {
        ctx.fillStyle = parts[i][1];
        ctx.globalAlpha = parts[i][1] === colors.ink ? 0.6 : 1;
        ctx.fillText(parts[i][0], cx, y);
        cx += ctx.measureText(parts[i][0]).width;
      }
      ctx.restore();
    }

    // Be polite: silence the loop when the tab is hidden (label stays synced).
    document.addEventListener("visibilitychange", function () {
      if (document.hidden && playing) stopAudio();
    });

    // ---- Theme + init --------------------------------------------------------
    QV.onThemeChange(function () {
      draw();
      drawWaterfall();
    });
    ready = true;
    refresh(false); // static first paint: never an empty axis
    drawWaterfall();
  });
})();
