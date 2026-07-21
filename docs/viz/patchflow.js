/*
 * docs/viz/patchflow.js — "Follow the Signal"
 *
 * The classic subtractive voice (GATE -> ADSR, VCO -> SVF -> VCA -> OUT) as a
 * live circuit. The patch graph is clickable: selecting any module scopes the
 * signal at that module's output, computed by the same JS DSP that mirrors the
 * Rust modules (QuiverViz.dsp). Audio-rate points can be auditioned: the note
 * LOOPS (the buffer ends in the release's silence, so loop:true reads as a
 * retriggering note) until the button — now "■ stop" — is pressed again. CV
 * and gate points are for eyes, not ears. A strip of mini-scopes under the
 * main scope shows every station at once, and a playhead sweeps the full-note
 * view while the note plays, wrapping each cycle (wall-clock elapsed time
 * modulo the buffer duration — the same period the loop replays, so ear and
 * eye cannot drift).
 *
 * Mirrors examples/first_patch.rs: the same modules, the same cables. The one
 * honest difference: first_patch.rs wires env -> cutoff directly (the Svf
 * clamps its 0..1 cutoff CV, so a 10 V envelope pins it); here the cutoff
 * modulation goes through base + depth scaling, which is what the port's
 * attenuverter is for in a real patch.
 *
 * Performance: this is the heaviest explorable (a ~1 s per-sample SVF chain
 * recomputed on every scrub). Two rates keep it smooth on phones:
 *   - The SCOPE simulates at 22050 Hz (half the work, visually identical at
 *     plot resolution).
 *   - The AUDIO renders lazily at 44100 Hz, only when play is pressed, from
 *     the IDENTICAL params/ENV via the same renderChain() — so what you hear
 *     is still exactly what you see, just sampled twice as densely.
 * Scrub input goes through QV.coalesce (one recompute per frame); implicit
 * play-restarts go through QV.debounce (no machine-gunning the AudioContext).
 *
 * Self-contained IIFE. Consumes window.QuiverViz (loaded first per book.toml).
 */
(function () {
  "use strict";
  if (typeof window === "undefined" || !window.QuiverViz) return;
  var QV = window.QuiverViz;

  var SCOPE_SR = 22050; // simulation rate for everything drawn
  var AUDIO_SR = 44100; // render rate for what is played (lazy, on play)

  var NODE_INFO = {
    gate: {
      label: "GATE",
      kind: "gate",
      blurb: "The key press. 0 V until the note starts, +5 V while it is held. It carries no sound — only the fact of “now.”"
    },
    vco: {
      label: "VCO · saw",
      kind: "audio",
      blurb: "The raw oscillator: every harmonic, full ±5 V, all the time. It has no idea a note is being played."
    },
    adsr: {
      label: "ADSR · env",
      kind: "cv",
      blurb: "The envelope converts the gate’s rectangle into a contour: attack, decay, sustain, release. 0–10 V of pure control."
    },
    vcf: {
      label: "SVF · lp",
      kind: "audio",
      blurb: "The filter, with cutoff pushed by the envelope: bright at the attack, mellowing as the envelope falls. Same waveform, fewer harmonics."
    },
    vca: {
      label: "VCA · out",
      kind: "audio",
      blurb: "The amplifier multiplies the audio by the envelope (gain = cv/10). Now the sound starts and stops — this is what makes it a note."
    },
    out: {
      label: "OUTPUT",
      kind: "audio",
      blurb: "What reaches your speakers: the VCA’s output, patched to both channels."
    }
  };

  // Station order for the mini-scope strip: the signal's journey left to right.
  var STATIONS = ["gate", "vco", "adsr", "vcf", "vca", "out"];
  var STATION_LABELS = { gate: "GATE", vco: "VCO", adsr: "ADSR", vcf: "SVF", vca: "VCA", out: "OUT" };

  var PATCH_SPEC = {
    modules: [
      { id: "vco", label: "VCO", x: 0, y: 0,
        inputs: [{ name: "voct", kind: "voct" }],
        outputs: [{ name: "saw", kind: "audio" }] },
      { id: "gate", label: "GATE", x: 0, y: 3.2,
        inputs: [],
        outputs: [{ name: "out", kind: "gate" }] },
      { id: "vcf", label: "SVF", x: 1, y: 0,
        inputs: [{ name: "in", kind: "audio" }, { name: "cutoff", kind: "cv" }],
        outputs: [{ name: "lp", kind: "audio" }] },
      { id: "adsr", label: "ADSR", x: 1, y: 3.2,
        inputs: [{ name: "gate", kind: "gate" }],
        outputs: [{ name: "env", kind: "cv" }] },
      { id: "vca", label: "VCA", x: 2, y: 0,
        inputs: [{ name: "in", kind: "audio" }, { name: "cv", kind: "cv" }],
        outputs: [{ name: "out", kind: "audio" }] },
      { id: "out", label: "OUTPUT", x: 3, y: 0,
        inputs: [{ name: "left", kind: "audio" }],
        outputs: [] }
    ],
    cables: [
      { from: "vco.saw", to: "vcf.in", kind: "audio" },
      { from: "gate.out", to: "adsr.gate", kind: "gate" },
      { from: "vcf.lp", to: "vca.in", kind: "audio" },
      { from: "adsr.env", to: "vcf.cutoff", kind: "cv" },
      { from: "adsr.env", to: "vca.cv", kind: "cv" },
      { from: "vca.out", to: "out.left", kind: "audio" }
    ]
  };

  // Layout for the mini-scope strip (one row on desktop, two rows of three on
  // phones). Injected once; all class names are qv-patchflow- prefixed.
  function injectStripStyle() {
    if (document.getElementById("qv-patchflow-strip-style")) return;
    var st = document.createElement("style");
    st.id = "qv-patchflow-strip-style";
    st.textContent =
      ".qv-patchflow-strip{display:flex;flex-wrap:wrap;gap:8px;margin-top:10px;}" +
      ".qv-patchflow-mini{flex:1 1 0;min-width:0;padding:0;margin:0;border:0;" +
      "background:transparent;cursor:pointer;border-radius:6px;" +
      "-webkit-tap-highlight-color:transparent;}" +
      "@media (max-width:560px){.qv-patchflow-mini{flex:1 1 30%;}}";
    document.head.appendChild(st);
  }

  QV.register("patchflow", function (root, QV) {
    var dsp = QV.dsp;

    // ---- Parameters (bound to prose scrubs below the widget) ----------------
    var params = {
      pitch: -1.0,   // V/Oct (C3)
      cutoff: 0.15,  // base cutoff CV 0..1
      depth: 0.55,   // envelope -> cutoff amount
      res: 0.25,     // resonance 0..1
      gate: 0.6      // gate length, seconds
    };
    var ENV = { a: 0.01, d: 0.25, s: 0.6, r: 0.35 };

    var selected = "vca";
    var zoomed = true; // audio nodes default to a 30 ms window
    var playHandle = null;
    var PLAY_LABEL = "▶ hear this point";

    // ---- DOM shell -----------------------------------------------------------
    var controls = QV.el("div", "qv-controls", root);
    var graphWrap = QV.el("div", null, root);
    var scopeWrap = QV.el("div", null, root);
    injectStripStyle();
    var stripWrap = QV.el("div", "qv-patchflow-strip", root);
    var instruction = QV.el("div", "qv-instruction", root);
    instruction.textContent = "Click any module in the graph — or any mini-scope in the strip — to scope that point. Scrub the numbers in the text below.";
    var readouts = QV.el("div", "qv-readouts", root);
    var blurbEl = QV.el("div", "qv-hint", root);

    var listenBtns = QV.buttons(controls, [
      {
        label: PLAY_LABEL,
        primary: true,
        onClick: function () { togglePlay(); }
      },
      {
        label: "retrigger",
        title: "Recompute and replay the note",
        onClick: function () {
          compute();
          drawScope();
          drawStrip();
          if (playHandle) { startPlay(); }
        }
      }
    ]);
    var zoomToggle = QV.toggle(controls, {
      label: "zoom to waveform (30 ms)",
      value: true,
      onChange: function (v) {
        zoomed = v;
        drawScope();
      }
    });
    function playBtn() {
      return listenBtns.qvButtons[PLAY_LABEL] || null;
    }

    var roNode = QV.readout(readouts, { label: "scoping" });
    var roFreq = QV.readout(readouts, { label: "vco freq" });
    var roCut = QV.readout(readouts, { label: "base cutoff" });
    var roPeak = QV.readout(readouts, { label: "peak at point" });

    // ---- The patch graph -----------------------------------------------------
    var graph = QV.patchGraph(graphWrap, PATCH_SPEC, {
      animate: true,
      onNodeClick: function (id) {
        select(id);
      }
    });

    // ---- The scope -----------------------------------------------------------
    var scope = QV.canvas(scopeWrap, {
      height: 190,
      onResize: function () { drawScope(); }
    });

    // ---- The mini-scope strip: the whole chain at a glance --------------------
    var minis = [];
    for (var si = 0; si < STATIONS.length; si++) {
      (function (id) {
        var btn = document.createElement("button");
        btn.type = "button";
        btn.className = "qv-patchflow-mini";
        btn.setAttribute("aria-label", "scope the " + STATION_LABELS[id] + " output");
        btn.title = "Scope " + NODE_INFO[id].label;
        stripWrap.appendChild(btn);
        var cell = { id: id, canvas: null };
        cell.canvas = QV.canvas(btn, {
          height: 64,
          onResize: function () { drawMini(cell); }
        });
        btn.addEventListener("click", function () { select(id); });
        minis.push(cell);
      })(STATIONS[si]);
    }

    // ---- Signal computation (the whole voice, mirrored from the Rust) --------
    var bufs = {};        // scope-rate buffers (SCOPE_SR)
    var audioBufs = null; // audio-rate buffers (AUDIO_SR), rendered lazily
    var audioDirty = true;
    var totalSec = 0;

    // Render the full voice at the given sample rate. Both the scope (22050)
    // and the audio (44100) come from this one function with the same params,
    // so "what you hear is what you see" survives the rate split.
    function renderChain(sr) {
      var total = params.gate + ENV.r + 0.15;
      var n = Math.round(total * sr);

      // GATE: 0/5 V rectangle.
      var gateN = Math.round(params.gate * sr);
      var gateBuf = new Float32Array(n);
      for (var i = 0; i < gateN && i < n; i++) gateBuf[i] = dsp.GATE_HIGH_V;

      // ADSR level (0..1); the module's env output is level * 10 V.
      var level = dsp.adsrEnvelope({
        attackSec: ENV.a, decaySec: ENV.d, sustainLevel: ENV.s, releaseSec: ENV.r,
        gateSec: params.gate, totalSec: total, sampleRate: sr
      });
      var envBuf = new Float32Array(n);
      for (var e = 0; e < n; e++) envBuf[e] = level[e] * 10;

      // VCO: bandlimited saw at the scrubbed pitch (±5 V).
      var freq = dsp.voctToHz(params.pitch);
      var vcoBuf = dsp.renderVco("saw", freq, sr, n);

      // SVF: cutoff CV = base + depth * level (what the port's attenuverter
      // does to a 10 V envelope in a real patch), resonance fixed by scrub.
      var filt = dsp.svf(sr);
      var vcfBuf = new Float32Array(n);
      for (var f = 0; f < n; f++) {
        var cv = params.cutoff + params.depth * level[f];
        if (cv > 1) cv = 1;
        if (cv < 0) cv = 0;
        vcfBuf[f] = filt.tick(vcoBuf[f], dsp.cutoffCvToHz(cv), params.res).lp;
      }

      // VCA: out = in * cv/10 (linear response, the module's default).
      var vcaBuf = new Float32Array(n);
      for (var v = 0; v < n; v++) vcaBuf[v] = vcfBuf[v] * (envBuf[v] / 10);

      return {
        gate: gateBuf,
        adsr: envBuf,
        vco: vcoBuf,
        vcf: vcfBuf,
        vca: vcaBuf,
        out: vcaBuf
      };
    }

    function compute() {
      totalSec = params.gate + ENV.r + 0.15;
      bufs = renderChain(SCOPE_SR);
    }

    // Audio buffers are the expensive half; render them only when someone
    // actually presses play, and cache until a param changes.
    function ensureAudioBufs() {
      if (!audioBufs || audioDirty) {
        audioBufs = renderChain(AUDIO_SR);
        audioDirty = false;
      }
    }

    // ---- Drawing -------------------------------------------------------------
    function nodeColor(kind, colors) {
      if (kind === "gate") return colors.gate;
      if (kind === "cv") return colors.cv;
      return colors.audio;
    }

    function drawScope() {
      if (!bufs) return; // canvas onResize fires during construction, pre-compute()
      var t = QV.theme();
      var c = t.colors;
      var info = NODE_INFO[selected];
      var buf = bufs[selected];
      if (!buf) return;
      scope.clear();

      var isAudio = info.kind === "audio";
      var useZoom = isAudio && zoomed;
      var i0 = 0;
      var i1 = buf.length;
      if (useZoom) {
        // 30 ms window centred mid-gate, where the note is in full voice.
        var mid = Math.round(params.gate * 0.5 * SCOPE_SR);
        var half = Math.round(0.015 * SCOPE_SR);
        i0 = Math.max(0, mid - half);
        i1 = Math.min(buf.length, mid + half);
      }

      var mx = 44, my = 12, mb = 26;
      var w = scope.w - mx - 12;
      var h = scope.h - my - mb;
      var yDomain = info.kind === "audio" ? [-6, 6] : [-0.5, 10.5];
      var xs = QV.scale([i0, i1], [mx, mx + w]);
      var ys = QV.scale(yDomain, [my + h, my]);
      var secScale = QV.scale([i0 / SCOPE_SR, i1 / SCOPE_SR], [mx, mx + w]);
      QV.axes(scope.ctx, {
        x: mx, y: my, w: w, h: h,
        xscale: secScale,
        yscale: ys,
        xlabel: useZoom ? "time (s) — 30 ms window" : "time (s)",
        ylabel: "volts",
        theme: t
      });

      // Gate span shading (full view only), so time has structure.
      if (!useZoom) {
        scope.ctx.save();
        scope.ctx.fillStyle = c.gate;
        scope.ctx.globalAlpha = 0.07;
        var gx1 = xs(Math.min(params.gate * SCOPE_SR, i1));
        scope.ctx.fillRect(mx, my, gx1 - mx, h);
        scope.ctx.restore();
      }

      var seg = buf.subarray(i0, i1);
      var segXs = QV.scale([0, seg.length], [mx, mx + w]);
      QV.wave(scope.ctx, seg, {
        xscale: segXs,
        yscale: ys,
        color: nodeColor(info.kind, c),
        width: 1.8
      });

      // Playhead: only meaningful in the full-note view (in the 30 ms zoom the
      // loop pulses the play button instead).
      if (playFrac != null && !useZoom) {
        var phx = mx + playFrac * w;
        scope.ctx.save();
        scope.ctx.strokeStyle = c.ink;
        scope.ctx.globalAlpha = 0.55;
        scope.ctx.lineWidth = 1;
        scope.ctx.beginPath();
        scope.ctx.moveTo(phx, my);
        scope.ctx.lineTo(phx, my + h);
        scope.ctx.stroke();
        scope.ctx.restore();
      }

      // Peak readout for the selected point.
      var peak = 0;
      for (var p = 0; p < buf.length; p++) {
        var a = Math.abs(buf[p]);
        if (a > peak) peak = a;
      }
      roPeak.set(peak.toFixed(2) + " V", QV.kindRole(info.kind));
    }

    // One mini-scope: decimated min/max columns in the station's kind color.
    function drawMini(cell) {
      var buf = bufs && bufs[cell.id];
      var cv = cell.canvas;
      if (!buf || !cv || !cv.w) return;
      var t = QV.theme();
      var c = t.colors;
      var info = NODE_INFO[cell.id];
      var color = nodeColor(info.kind, c);
      var ctx = cv.ctx;
      cv.clear();
      ctx.fillStyle = c.panel;
      ctx.fillRect(0, 0, cv.w, cv.h);

      var yDomain = info.kind === "audio" ? [-6, 6] : [-0.5, 10.5];
      var ys = QV.scale(yDomain, [cv.h - 3, 3]);
      var n = buf.length;
      var cols = Math.max(1, Math.floor(cv.w));
      ctx.strokeStyle = color;
      ctx.lineWidth = 1;
      ctx.globalAlpha = cell.id === selected ? 1 : 0.6;
      ctx.beginPath();
      for (var x = 0; x < cols; x++) {
        var a0 = Math.floor((x * n) / cols);
        var a1 = Math.max(a0 + 1, Math.floor(((x + 1) * n) / cols));
        if (a1 > n) a1 = n;
        var mn = buf[a0], mxv = buf[a0];
        for (var i = a0 + 1; i < a1; i++) {
          var v = buf[i];
          if (v < mn) mn = v;
          else if (v > mxv) mxv = v;
        }
        var yTop = ys(mxv);
        var yBot = ys(mn);
        if (yBot - yTop < 1) { yTop -= 0.5; yBot += 0.5; }
        ctx.moveTo(x + 0.5, yTop);
        ctx.lineTo(x + 0.5, yBot);
      }
      ctx.stroke();
      ctx.globalAlpha = 1;

      // Station name.
      ctx.fillStyle = c.ink;
      ctx.font = "10px var(--mono-font, monospace)";
      ctx.textAlign = "left";
      ctx.textBaseline = "top";
      ctx.fillText(STATION_LABELS[cell.id], 5, 4);

      // Selection highlight (kind-colored frame) vs. quiet hairline.
      if (cell.id === selected) {
        ctx.strokeStyle = color;
        ctx.lineWidth = 2;
        ctx.strokeRect(1, 1, cv.w - 2, cv.h - 2);
      } else {
        ctx.strokeStyle = c.grid;
        ctx.lineWidth = 1;
        ctx.strokeRect(0.5, 0.5, cv.w - 1, cv.h - 1);
      }
    }

    function drawStrip() {
      for (var i = 0; i < minis.length; i++) drawMini(minis[i]);
    }

    function updateReadouts() {
      var info = NODE_INFO[selected];
      roNode.set(info.label, QV.kindRole(info.kind));
      roFreq.set(Math.round(dsp.voctToHz(params.pitch) * 10) / 10 + " Hz · " + dsp.voctToNote(params.pitch), "voct");
      roCut.set(Math.round(dsp.cutoffCvToHz(params.cutoff)) + " Hz", "cv");
      blurbEl.textContent = info.blurb;
      var canHear = info.kind === "audio";
      var btn = playBtn();
      if (btn) {
        btn.disabled = !canHear;
        btn.style.opacity = canHear ? "" : "0.4";
        btn.title = canHear
          ? "Listen to the signal at " + info.label
          : "CV and gates are for machines, not ears — select an audio-rate module to listen";
      }
      zoomToggle.style.display = canHear ? "" : "none";
    }

    function select(id) {
      if (!NODE_INFO[id]) return;
      selected = id;
      graph.setActive(id);
      stopPlay();
      updateReadouts();
      drawScope();
      drawStrip();
    }

    // ---- Audio + playhead ------------------------------------------------------
    var playStartMs = 0;
    var playFrac = null; // null = no playhead on screen

    // While the note plays, sweep a playhead across the full-note scope (or,
    // in the 30 ms zoom where a sweep is meaningless, gently pulse the play
    // button). QV.loop never runs under prefers-reduced-motion, so neither
    // the sweep nor the pulse happens there.
    var playheadLoop = QV.loop(root, function () {
      var btn = playBtn();
      if (!playHandle) {
        playheadLoop.pause();
        var had = playFrac != null;
        playFrac = null;
        if (btn) btn.style.opacity = "";
        if (had) drawScope(); // erase the playhead
        return;
      }
      var elapsed = (performance.now() - playStartMs) / 1000;
      // The playhead wraps each loop cycle: elapsed modulo the note duration.
      var frac = totalSec > 0 ? (elapsed / totalSec) % 1 : 0;
      var info = NODE_INFO[selected];
      if (zoomed && info.kind === "audio") {
        playFrac = null;
        if (btn) btn.style.opacity = String(0.65 + 0.35 * (0.5 + 0.5 * Math.sin(elapsed * 6)));
      } else {
        if (btn) btn.style.opacity = "";
        playFrac = frac;
        drawScope();
      }
    });

    function startPlay() {
      stopPlay();
      var info = NODE_INFO[selected];
      if (info.kind !== "audio") return;
      // Belt and braces against source leaks under rapid retriggering:
      // QV.audio.play() begins with audio.stop(), and we also stop explicitly.
      QV.audio.stop();
      ensureAudioBufs();
      // loop:true — the buffer ends in the release's silence, so the loop
      // reads as a retriggering note. Runs until the button is pressed again.
      playHandle = QV.audio.play(audioBufs[selected], AUDIO_SR, { gain: 0.25, loop: true });
      if (!playHandle) return; // no WebAudio support
      var btn = playBtn();
      if (btn) btn.textContent = "■ stop";
      playStartMs = performance.now();
      playheadLoop.play(); // no-op under reduced motion
      // Restore the button label if the source ends outside stopPlay() (e.g.
      // another widget's play() displaces this loop); the playhead loop
      // notices playHandle === null on its next frame and cleans up.
      var mine = playHandle;
      if (mine.src) {
        var prev = mine.src.onended;
        mine.src.onended = function () {
          if (prev) prev();
          if (playHandle === mine) {
            playHandle = null;
            if (btn) btn.textContent = PLAY_LABEL;
          }
        };
      }
    }
    function stopPlay() {
      if (playHandle) {
        playHandle.stop();
        playHandle = null;
      }
      playheadLoop.pause();
      var hadPlayhead = playFrac != null;
      playFrac = null;
      var btn = playBtn();
      if (btn) {
        btn.textContent = PLAY_LABEL;
        btn.style.opacity = "";
      }
      if (hadPlayhead) drawScope(); // erase the playhead
    }
    function togglePlay() {
      if (playHandle) stopPlay();
      else startPlay();
    }

    // ---- Prose scrubs --------------------------------------------------------
    // The heavy path (per-sample chain + scope + strip) is rAF-coalesced: a
    // burst of scrub events costs one recompute per frame. Readouts are cheap
    // and stay immediate so the numbers track the finger exactly.
    var refresh = QV.coalesce(function () {
      compute();
      drawScope();
      drawStrip();
    });
    // If the note is playing while a param is scrubbed, restart it with the
    // new sound — but debounced, so dragging doesn't machine-gun the
    // AudioContext with restarts.
    var restartIfPlaying = QV.debounce(function () {
      if (playHandle) startPlay();
    }, 180);

    function bindScrub(id, key) {
      var span = document.getElementById(id);
      if (!span) return;
      QV.scrub(span, {
        onInput: function (v) {
          params[key] = v;
          audioDirty = true;   // audio re-renders lazily on next play
          updateReadouts();    // cheap: immediate
          refresh();           // heavy: coalesced to one run per frame
          restartIfPlaying();  // implicit restart: debounced
        }
      });
    }
    bindScrub("qv-patchflow-pitch", "pitch");
    bindScrub("qv-patchflow-cutoff", "cutoff");
    bindScrub("qv-patchflow-depth", "depth");
    bindScrub("qv-patchflow-res", "res");
    bindScrub("qv-patchflow-gate", "gate");

    // Be polite: silence the loop when the tab is hidden (label stays synced).
    document.addEventListener("visibilitychange", function () {
      if (document.hidden && playHandle) stopPlay();
    });

    // ---- Boot ----------------------------------------------------------------
    QV.onThemeChange(function () {
      drawScope();
      drawStrip();
    });
    compute();
    graph.setActive(selected);
    updateReadouts();
    drawScope();
    drawStrip();
  });
})();
