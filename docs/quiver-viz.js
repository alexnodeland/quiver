/*
 * quiver-viz.js — shared infrastructure for the Quiver Explorables.
 *
 * Attaches a single global `window.QuiverViz`. ES5-compatible IIFE: no modules,
 * no build step, no external dependencies, works from file://. Widget scripts
 * (docs/viz/*.js) call QuiverViz.register("name", fn) and consume this API;
 * they MUST NOT duplicate what lives here.
 *
 * Ported from fugue-viz.js (fugue, branch docs/explorables) with the
 * probability math replaced by DSP math. The DSP mirrors Quiver's Rust source
 * EXACTLY — same constants, same coefficient formulas, same voltage
 * conventions — so every plot in the book is the library's real math:
 *
 *   dsp.voctToHz / C4_HZ            <- src/modules/common.rs  voct_to_hz
 *   dsp.polyblep / dsp.polyblamp    <- src/modules/common.rs
 *   dsp.envCoef                     <- src/modules/common.rs  env_coef
 *   dsp.vco* (PolyBLEP/BLAMP osc)   <- src/modules/oscillators.rs  Vco::tick
 *   dsp.svf* (Cytomic TPT/ZDF SVF)  <- src/modules/filters.rs  Svf::tick
 *   dsp.adsr* (linear + one-pole)   <- src/modules/dynamics.rs  Adsr::tick
 *
 * Voltage is the lingua franca, as in the hardware conventions the library
 * models: audio is ±5 V, unipolar CV 0–10 V, gates 0/5 V, pitch 1 V/octave
 * with 0 V = C4. Buffers rendered by dsp.* are in VOLTS; the audio player
 * divides by 5 V to reach the DAC.
 */
(function () {
  "use strict";

  if (typeof window !== "undefined" && window.QuiverViz) {
    return; // already loaded (guard against double-inclusion)
  }

  var TAU = Math.PI * 2;

  // ==========================================================================
  // Deterministic RNG (reproducibility is a Quiver value too — see src/rng.rs)
  // ==========================================================================

  // mulberry32: seed (uint32) -> function returning float in [0, 1).
  function rng(seed) {
    var a = seed >>> 0;
    return function () {
      a |= 0;
      a = (a + 0x6d2b79f5) | 0;
      var t = Math.imul(a ^ (a >>> 15), 1 | a);
      t = (t + Math.imul(t ^ (t >>> 7), 61 | t)) ^ t;
      return ((t ^ (t >>> 14)) >>> 0) / 4294967296;
    };
  }

  // Standard normal via Box–Muller (for noise sources).
  function randn(rand) {
    var u1 = rand();
    var u2 = rand();
    if (u1 < 1e-300) u1 = 1e-300;
    return Math.sqrt(-2.0 * Math.log(u1)) * Math.cos(TAU * u2);
  }

  // ==========================================================================
  // DSP math — mirrors the Rust sources named in the header. Keep in sync.
  // ==========================================================================

  // src/modules/common.rs
  var C4_HZ = 261.6255653005986; // 0 V reference for V/Oct
  var GATE_HIGH_V = 5.0;
  var GATE_THRESHOLD_V = 2.5;
  var AUDIO_PEAK_V = 5.0; // nominal audio amplitude (±5 V)

  function voctToHz(voct) {
    return C4_HZ * Math.pow(2, voct);
  }
  function hzToVoct(hz) {
    return Math.log(hz / C4_HZ) / Math.LN2;
  }

  var NOTE_NAMES = ["C", "C#", "D", "D#", "E", "F", "F#", "G", "G#", "A", "A#", "B"];
  // Nearest equal-tempered note name for a V/Oct voltage (0 V = C4).
  function voctToNote(voct) {
    var semis = Math.round(voct * 12);
    var name = NOTE_NAMES[((semis % 12) + 12) % 12];
    var octave = 4 + Math.floor(semis / 12);
    return name + octave;
  }

  // PolyBLEP residual for a VALUE discontinuity at phase t, step dt.
  // <- src/modules/common.rs polyblep
  function polyblep(t, dt) {
    if (dt <= 0) return 0;
    if (t < dt) {
      var a = t / dt;
      return 2 * a - a * a - 1;
    }
    if (t > 1 - dt) {
      var b = (t - 1) / dt;
      return b * b + 2 * b + 1;
    }
    return 0;
  }

  // PolyBLAMP residual for a SLOPE discontinuity (corner) at phase t, step dt.
  // <- src/modules/common.rs polyblamp
  function polyblamp(t, dt) {
    if (dt <= 0) return 0;
    if (t < dt) {
      var a = t / dt - 1;
      return -(a * a * a) / 3;
    }
    if (t > 1 - dt) {
      var b = (t - 1) / dt + 1;
      return (b * b * b) / 3;
    }
    return 0;
  }

  // One-pole coefficient exp(-1/(time*sr)); 0 for degenerate times.
  // <- src/modules/common.rs env_coef
  function envCoef(timeSeconds, sampleRate) {
    var denom = timeSeconds * sampleRate;
    if (denom <= 0) return 0;
    return Math.exp(-1 / denom);
  }

  // ---- VCO (bandlimited, PolyBLEP/PolyBLAMP) — src/modules/oscillators.rs ---
  //
  // vcoSample(shape, phase, dt, pw) -> one sample in VOLTS (±5), phase in
  // [0,1), dt = freq/sr, pw = pulse width (clamped 0.05–0.95 like the module).
  // shape: "sin" | "tri" | "saw" | "sqr". Mirrors Vco::tick (minus hard sync).
  function vcoSample(shape, phase, dt, pw) {
    var dtAbs = Math.abs(dt);
    if (shape === "sin") {
      return Math.sin(phase * TAU) * AUDIO_PEAK_V;
    }
    if (shape === "saw") {
      var saw = 2 * phase - 1;
      saw -= polyblep(phase, dtAbs);
      return saw * AUDIO_PEAK_V;
    }
    if (shape === "sqr") {
      pw = pw == null ? 0.5 : Math.min(0.95, Math.max(0.05, pw));
      var sqr = phase < pw ? 1 : -1;
      sqr += polyblep(phase, dtAbs);
      var x = phase + (1 - pw);
      x -= Math.floor(x);
      sqr -= polyblep(x, dtAbs);
      return sqr * AUDIO_PEAK_V;
    }
    // triangle
    var tri = 1 - 4 * Math.abs(phase - 0.5);
    var cornerHalf = phase - 0.5;
    if (cornerHalf < 0) cornerHalf += 1;
    tri += 4 * dtAbs * polyblamp(phase, dtAbs);
    tri -= 4 * dtAbs * polyblamp(cornerHalf, dtAbs);
    return tri * AUDIO_PEAK_V;
  }

  // Render n samples of a VCO waveform into a Float32Array (VOLTS, ±5).
  // opts: {phase0, pw}. Returns the buffer.
  function renderVco(shape, freqHz, sampleRate, n, opts) {
    opts = opts || {};
    var out = new Float32Array(n);
    var phase = opts.phase0 || 0;
    var dt = freqHz / sampleRate;
    for (var i = 0; i < n; i++) {
      out[i] = vcoSample(shape, phase, dt, opts.pw);
      phase += dt;
      phase -= Math.floor(phase);
      if (phase < 0) phase += 1;
    }
    return out;
  }

  // ---- SVF (Cytomic TPT / zero-delay feedback) — src/modules/filters.rs ----

  var SVF_K_MIN = 1e-5;
  var SVF_STATE_LIMIT = 8.0;

  function svfSoftClip(x) {
    if (Math.abs(x) <= SVF_STATE_LIMIT) return x;
    return SVF_STATE_LIMIT * Math.tanh(x / SVF_STATE_LIMIT);
  }

  // The module's cutoff-knob law: CV 0..1 -> 20 Hz .. 20 kHz (exponential).
  function cutoffCvToHz(cv) {
    cv = Math.min(1, Math.max(0, cv));
    return 20 * Math.pow(1000, cv);
  }
  function hzToCutoffCv(hz) {
    return Math.log(hz / 20) / Math.log(1000);
  }

  // Damping k = 1/Q from the res knob: res 0 -> k=2 (Q=0.5), res 1 -> k~0.
  function svfK(res) {
    res = Math.min(1, Math.max(0, res));
    return Math.max(2 - 2 * res, SVF_K_MIN);
  }

  // EXACT discrete magnitude response of the TPT SVF at frequency fHz.
  // The ZDF core is the bilinear transform of the analog SVF with prewarped
  // omega0 = g = tan(pi*fc/fs), so evaluating the analog prototype at
  // s = j*tan(pi*f/fs) gives the exact digital response:
  //   H_lp = g^2 / D,  H_bp = g*s / D,  H_hp = s^2 / D,  H_notch = (g^2+s^2)/D
  //   with D = s^2 + k*g*s + g^2       (and H_lp + k*H_bp + H_hp = 1).
  // mode: "lp" | "bp" | "hp" | "notch". Returns linear gain.
  function svfMagnitude(mode, fHz, fcHz, res, sampleRate) {
    var maxFc = 0.49 * sampleRate;
    var g = Math.tan(Math.PI * Math.min(fcHz, maxFc) / sampleRate);
    var k = svfK(res);
    var w = Math.tan(Math.PI * Math.min(fHz, maxFc) / sampleRate); // s = j*w
    // D = (g^2 - w^2) + j*(k*g*w)
    var dr = g * g - w * w;
    var di = k * g * w;
    var dmag = Math.sqrt(dr * dr + di * di);
    if (dmag === 0) return 0;
    var num;
    if (mode === "lp") num = g * g;
    else if (mode === "hp") num = w * w;
    else if (mode === "bp") num = g * w;
    else num = Math.abs(g * g - w * w); // notch: |g^2 + s^2| with s = j*w
    return num / dmag;
  }

  // Stateful SVF processor mirroring Svf::tick sample-for-sample, in VOLTS.
  // Usage: var f = dsp.svf(sr); f.tick(x, fcHz, res) -> {lp, bp, hp, notch}.
  function svf(sampleRate) {
    var ic1eq = 0;
    var ic2eq = 0;
    return {
      tick: function (input, cutoffHz, res) {
        var maxFc = 0.49 * sampleRate;
        var fc = Math.min(Math.min(20000, Math.max(20, cutoffHz)), maxFc);
        var g = Math.tan(Math.PI * fc / sampleRate);
        var k = svfK(res);
        var a1 = 1 / (1 + g * (g + k));
        var a2 = g * a1;
        var a3 = g * a2;
        var v0 = input;
        var v3 = v0 - ic2eq;
        var v1 = a1 * ic1eq + a2 * v3;
        var v2 = ic2eq + a2 * ic1eq + a3 * v3;
        ic1eq = svfSoftClip(2 * v1 - ic1eq);
        ic2eq = svfSoftClip(2 * v2 - ic2eq);
        var low = v2;
        var band = v1;
        var high = v0 - k * v1 - v2;
        return { lp: low, bp: band, hp: high, notch: low + high };
      },
      reset: function () {
        ic1eq = 0;
        ic2eq = 0;
      }
    };
  }

  // ---- ADSR — src/modules/dynamics.rs Adsr::tick ---------------------------

  // The module's time-knob law: CV 0..1 -> 1 ms .. 10 s (exponential).
  function adsrCvToTime(cv) {
    cv = Math.min(1, Math.max(0, cv));
    return 0.001 * Math.pow(10000, cv);
  }
  function adsrTimeToCv(seconds) {
    return Math.log(seconds / 0.001) / Math.log(10000);
  }

  // Render the envelope LEVEL (0..1; the module outputs level*10 V on `env`).
  // opts: {attackSec, decaySec, sustainLevel, releaseSec, gateSec, totalSec,
  //        sampleRate, exp}. Mirrors the stage machine including the
  //        release-rate scaling from the level captured at gate-fall.
  function adsrEnvelope(opts) {
    var a = Math.max(1e-4, opts.attackSec);
    var d = Math.max(1e-4, opts.decaySec);
    var s = Math.min(1, Math.max(0, opts.sustainLevel));
    var r = Math.max(1e-4, opts.releaseSec);
    var sr = opts.sampleRate || 1000; // control-rate is fine for plotting
    var gateN = Math.round(opts.gateSec * sr);
    var totalN = Math.round(opts.totalSec * sr);
    var exp = !!opts.exp;
    var EXP_DONE = 1e-3;

    var out = new Float32Array(totalN);
    var level = 0;
    var stage = "attack";
    var releaseStart = 0;
    var attackRate = 1 / (a * sr);
    var decayRate = (1 - s) / (d * sr);
    var aCoef = envCoef(a, sr);
    var dCoef = envCoef(d, sr);
    var rCoef = envCoef(r, sr);

    for (var i = 0; i < totalN; i++) {
      var gateHigh = i < gateN;
      if (!gateHigh && stage !== "release" && stage !== "idle") {
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
      out[i] = level;
    }
    return out;
  }

  // ---- Spectrum (radix-2 FFT + Hann window) ---------------------------------

  // In-place complex FFT (Cooley–Tukey). re/im are equal-length Float arrays
  // whose length is a power of two.
  function fft(re, im) {
    var n = re.length;
    if (n <= 1) return;
    // bit-reversal permutation
    for (var i = 1, j = 0; i < n; i++) {
      var bit = n >> 1;
      for (; j & bit; bit >>= 1) j ^= bit;
      j ^= bit;
      if (i < j) {
        var tr = re[i]; re[i] = re[j]; re[j] = tr;
        var ti = im[i]; im[i] = im[j]; im[j] = ti;
      }
    }
    for (var len = 2; len <= n; len <<= 1) {
      var ang = -TAU / len;
      var wr = Math.cos(ang), wi = Math.sin(ang);
      for (var k = 0; k < n; k += len) {
        var cr = 1, ci = 0;
        for (var m = 0; m < len / 2; m++) {
          var ur = re[k + m], ui = im[k + m];
          var vr = re[k + m + len / 2] * cr - im[k + m + len / 2] * ci;
          var vi = re[k + m + len / 2] * ci + im[k + m + len / 2] * cr;
          re[k + m] = ur + vr; im[k + m] = ui + vi;
          re[k + m + len / 2] = ur - vr; im[k + m + len / 2] = ui - vi;
          var ncr = cr * wr - ci * wi;
          ci = cr * wi + ci * wr;
          cr = ncr;
        }
      }
    }
  }

  // Magnitude spectrum in dB of `samples` (VOLTS). opts: {sampleRate, size}.
  // Hann-windowed, normalized so a full-scale (±5 V) sine peaks at ~0 dB.
  // Returns {freqs: Float32Array, db: Float32Array} (bins up to Nyquist).
  function spectrumDb(samples, opts) {
    opts = opts || {};
    var size = opts.size || 2048;
    var sr = opts.sampleRate || 44100;
    var re = new Float32Array(size);
    var im = new Float32Array(size);
    var winSum = 0;
    for (var i = 0; i < size; i++) {
      var w = 0.5 - 0.5 * Math.cos(TAU * i / (size - 1)); // Hann
      winSum += w;
      re[i] = (i < samples.length ? samples[i] / AUDIO_PEAK_V : 0) * w;
    }
    fft(re, im);
    var half = size / 2;
    var freqs = new Float32Array(half);
    var db = new Float32Array(half);
    // Coherent gain: a unit sine yields winSum/2 in its bin.
    var norm = 2 / winSum;
    for (var k = 0; k < half; k++) {
      var mag = Math.sqrt(re[k] * re[k] + im[k] * im[k]) * norm;
      freqs[k] = (k * sr) / size;
      db[k] = 20 * Math.log10(mag + 1e-12);
    }
    return { freqs: freqs, db: db };
  }

  var dsp = {
    TAU: TAU,
    C4_HZ: C4_HZ,
    GATE_HIGH_V: GATE_HIGH_V,
    GATE_THRESHOLD_V: GATE_THRESHOLD_V,
    AUDIO_PEAK_V: AUDIO_PEAK_V,
    voctToHz: voctToHz,
    hzToVoct: hzToVoct,
    voctToNote: voctToNote,
    polyblep: polyblep,
    polyblamp: polyblamp,
    envCoef: envCoef,
    vcoSample: vcoSample,
    renderVco: renderVco,
    cutoffCvToHz: cutoffCvToHz,
    hzToCutoffCv: hzToCutoffCv,
    svfK: svfK,
    svfMagnitude: svfMagnitude,
    svf: svf,
    adsrCvToTime: adsrCvToTime,
    adsrTimeToCv: adsrTimeToCv,
    adsrEnvelope: adsrEnvelope,
    fft: fft,
    spectrumDb: spectrumDb
  };

  // ==========================================================================
  // Theming — semantic signal colors shared by prose, plots, and patch graphs.
  //   audio (blue) · cv (coral) · gate/trigger/clock (green) · voct (yellow)
  //   mod (violet, for LFO/secondary modulation)
  // Quiver's book default theme is `rust` (light), so LIGHT is the fallback.
  // ==========================================================================

  var DARK_THEMES = { coal: 1, navy: 1, ayu: 1, dark: 1 };

  var LIGHT_COLORS = {
    audio: "#0969DA",
    cv: "#CF222E",
    gate: "#1A7F37",
    voct: "#9A6700",
    mod: "#8250DF",
    ink: "rgba(31,35,40,0.9)",
    grid: "rgba(31,35,40,0.08)",
    panel: "rgba(175,184,193,0.12)"
  };
  var DARK_COLORS = {
    audio: "#58A6FF",
    cv: "#FF7B72",
    gate: "#56D364",
    voct: "#F2CC60",
    mod: "#BC8CFF",
    ink: "rgba(230,237,243,0.9)",
    grid: "rgba(230,237,243,0.08)",
    panel: "rgba(110,118,129,0.08)"
  };

  function isDark() {
    if (typeof document === "undefined") return false;
    // Trust the page's actual rendered ground, not the theme class name:
    // mdbook stamps whatever default-theme names, valid or not, so class
    // whitelists mislabel the broken case. Luminance can't.
    try {
      var bg = getComputedStyle(document.documentElement).getPropertyValue("--bg").trim();
      var m = bg.match(/^#([0-9a-f]{6})$/i);
      if (m) {
        var n = parseInt(m[1], 16);
        var lum = 0.2126 * ((n >> 16) & 255) + 0.7152 * ((n >> 8) & 255) + 0.0722 * (n & 255);
        return lum < 128;
      }
      m = bg.match(/rgba?\(\s*(\d+)\s*,\s*(\d+)\s*,\s*(\d+)/);
      if (m) {
        return 0.2126 * m[1] + 0.7152 * m[2] + 0.0722 * m[3] < 128;
      }
      m = bg.match(/hsla?\(\s*[\d.]+\s*,\s*[\d.]+%\s*,\s*([\d.]+)%/);
      if (m) {
        return parseFloat(m[1]) < 50;
      }
    } catch (e) { /* fall through to class heuristic */ }
    var cls = document.documentElement.className || "";
    var names = cls.split(/\s+/);
    for (var i = 0; i < names.length; i++) {
      if (DARK_THEMES[names[i]]) return true;
    }
    return false; // book default (`rust`) is light
  }

  function readColor(name, fallback) {
    if (typeof getComputedStyle === "undefined") return fallback;
    try {
      var v = getComputedStyle(document.documentElement).getPropertyValue("--qv-" + name);
      v = v && v.trim();
      return v || fallback;
    } catch (e) {
      return fallback;
    }
  }

  function theme() {
    var dark = isDark();
    var base = dark ? DARK_COLORS : LIGHT_COLORS;
    return {
      dark: dark,
      colors: {
        audio: readColor("audio", base.audio),
        cv: readColor("cv", base.cv),
        gate: readColor("gate", base.gate),
        voct: readColor("voct", base.voct),
        mod: readColor("mod", base.mod),
        ink: readColor("ink", base.ink),
        grid: readColor("grid", base.grid),
        panel: readColor("panel", base.panel)
      }
    };
  }

  var themeListeners = [];
  var themeObserver = null;
  function onThemeChange(fn) {
    themeListeners.push(fn);
    if (!themeObserver && typeof MutationObserver !== "undefined" && typeof document !== "undefined") {
      var last = isDark();
      themeObserver = new MutationObserver(function () {
        var now = isDark();
        if (now !== last) {
          last = now;
          var t = theme();
          for (var i = 0; i < themeListeners.length; i++) {
            try {
              themeListeners[i](t);
            } catch (e) {}
          }
        }
      });
      themeObserver.observe(document.documentElement, { attributes: true, attributeFilter: ["class"] });
    }
  }

  // Map a SignalKind (as spelled in patch specs) to a semantic color role.
  var KIND_ROLE = {
    audio: "audio",
    voct: "voct",
    "v/oct": "voct",
    voltperoctave: "voct",
    gate: "gate",
    trigger: "gate",
    clock: "gate",
    cv: "cv",
    cvbipolar: "cv",
    cvunipolar: "cv",
    "cv-bipolar": "cv",
    "cv-unipolar": "cv",
    mod: "mod"
  };
  function kindRole(kind) {
    if (!kind) return "audio";
    return KIND_ROLE[String(kind).toLowerCase()] || "audio";
  }

  // ==========================================================================
  // Canvas scaffolding
  // ==========================================================================

  function canvas(parentEl, opts) {
    opts = opts || {};
    var height = opts.height || 300;
    var el = document.createElement("canvas");
    el.className = "qv-canvas";
    el.style.display = "block";
    el.style.width = "100%";
    el.style.height = height + "px";
    parentEl.appendChild(el);
    var ctx = el.getContext("2d");

    var api = { ctx: ctx, el: el, w: 0, h: 0, dpr: 1, clear: clear };

    function resize() {
      var dpr = window.devicePixelRatio || 1;
      var rect = el.getBoundingClientRect();
      var cssW = Math.max(1, rect.width || parentEl.clientWidth || 300);
      var cssH = height;
      el.width = Math.round(cssW * dpr);
      el.height = Math.round(cssH * dpr);
      api.w = cssW;
      api.h = cssH;
      api.dpr = dpr;
      ctx.setTransform(dpr, 0, 0, dpr, 0, 0);
      if (opts.onResize) opts.onResize(api);
    }

    function clear() {
      ctx.setTransform(1, 0, 0, 1, 0, 0);
      ctx.clearRect(0, 0, el.width, el.height);
      ctx.setTransform(api.dpr, 0, 0, api.dpr, 0, 0);
    }

    resize();
    if (typeof ResizeObserver !== "undefined") {
      var ro = new ResizeObserver(function () {
        resize();
      });
      ro.observe(parentEl);
      api._ro = ro;
    } else if (typeof window !== "undefined") {
      window.addEventListener("resize", resize);
    }
    return api;
  }

  // Linear scale: maps domain [d0,d1] -> range [r0,r1]. Returns fn with
  // .invert, .domain, .range.
  function scale(domain, range) {
    var d0 = domain[0], d1 = domain[1], r0 = range[0], r1 = range[1];
    var dspan = d1 - d0 || 1;
    var f = function (x) {
      return r0 + ((x - d0) / dspan) * (r1 - r0);
    };
    f.invert = function (y) {
      return d0 + ((y - r0) / (r1 - r0 || 1)) * dspan;
    };
    f.domain = domain;
    f.range = range;
    return f;
  }

  // Log-frequency scale (base 2), for spectra and filter responses.
  function logScale(domain, range) {
    var d0 = Math.log(domain[0]), d1 = Math.log(domain[1]);
    var r0 = range[0], r1 = range[1];
    var dspan = d1 - d0 || 1;
    var f = function (x) {
      return r0 + ((Math.log(x) - d0) / dspan) * (r1 - r0);
    };
    f.invert = function (y) {
      return Math.exp(d0 + ((y - r0) / (r1 - r0 || 1)) * dspan);
    };
    f.domain = domain;
    f.range = range;
    f.log = true;
    return f;
  }

  function niceTicks(lo, hi, count) {
    count = count || 5;
    var span = hi - lo;
    if (span <= 0 || !isFinite(span)) return [lo];
    var step = Math.pow(10, Math.floor(Math.log(span / count) / Math.LN10));
    var err = (span / count) / step;
    if (err >= 7.5) step *= 10;
    else if (err >= 3.5) step *= 5;
    else if (err >= 1.5) step *= 2;
    var start = Math.ceil(lo / step) * step;
    var out = [];
    for (var v = start; v <= hi + step * 1e-6; v += step) {
      out.push(Math.abs(v) < step * 1e-6 ? 0 : v);
    }
    return out;
  }

  // 1-2-5 decade ticks for log-frequency axes (20, 50, 100, 200, ... 20k).
  function logTicks(lo, hi) {
    var out = [];
    var decade = Math.pow(10, Math.floor(Math.log(lo) / Math.LN10));
    var mults = [1, 2, 5];
    for (; decade <= hi; decade *= 10) {
      for (var i = 0; i < mults.length; i++) {
        var v = decade * mults[i];
        if (v >= lo * 0.999 && v <= hi * 1.001) out.push(v);
      }
    }
    return out;
  }

  function fmtTick(v) {
    if (v === 0) return "0";
    var a = Math.abs(v);
    if (a >= 1e5 || a < 1e-3) return v.toExponential(0);
    return String(Math.round(v * 1000) / 1000);
  }

  function fmtHz(v) {
    if (v >= 1000) return Math.round(v / 100) / 10 + "k";
    return String(Math.round(v));
  }

  // Draws axes/gridlines/labels. opts: {x, y, w, h, xscale, yscale,
  // xlabel, ylabel, theme, xfmt, yfmt}. x,y = pixel origin of the plot area.
  // Log x-scales (from logScale) get 1-2-5 decade ticks automatically.
  function axes(ctx, opts) {
    var t = opts.theme || theme();
    var c = t.colors;
    var x0 = opts.x != null ? opts.x : 0;
    var y0 = opts.y != null ? opts.y : 0;
    var w = opts.w, h = opts.h;
    ctx.save();
    ctx.lineWidth = 1;
    ctx.strokeStyle = c.grid;
    ctx.fillStyle = c.ink;
    ctx.font = "11px var(--mono-font, monospace)";
    ctx.textBaseline = "top";
    ctx.textAlign = "center";

    if (opts.xscale) {
      var xfmt = opts.xfmt || (opts.xscale.log ? fmtHz : fmtTick);
      var xt = opts.xscale.log
        ? logTicks(opts.xscale.domain[0], opts.xscale.domain[1])
        : niceTicks(opts.xscale.domain[0], opts.xscale.domain[1], 6);
      for (var i = 0; i < xt.length; i++) {
        var px = opts.xscale(xt[i]);
        ctx.beginPath();
        ctx.moveTo(px, y0);
        ctx.lineTo(px, y0 + h);
        ctx.stroke();
        ctx.fillText(xfmt(xt[i]), px, y0 + h + 4);
      }
    }
    if (opts.yscale) {
      var yfmt = opts.yfmt || fmtTick;
      ctx.textAlign = "right";
      ctx.textBaseline = "middle";
      var yt = niceTicks(opts.yscale.domain[0], opts.yscale.domain[1], 5);
      for (var j = 0; j < yt.length; j++) {
        var py = opts.yscale(yt[j]);
        ctx.beginPath();
        ctx.moveTo(x0, py);
        ctx.lineTo(x0 + w, py);
        ctx.stroke();
        ctx.fillText(yfmt(yt[j]), x0 - 4, py);
      }
    }
    // Axis frame
    ctx.strokeStyle = c.ink;
    ctx.globalAlpha = 0.35;
    ctx.strokeRect(x0, y0, w, h);
    ctx.globalAlpha = 1;

    if (opts.xlabel) {
      ctx.textAlign = "center";
      ctx.textBaseline = "bottom";
      ctx.fillText(opts.xlabel, x0 + w / 2, y0 + h + 24);
    }
    if (opts.ylabel) {
      ctx.save();
      ctx.translate(x0 - 30, y0 + h / 2);
      ctx.rotate(-Math.PI / 2);
      ctx.textAlign = "center";
      ctx.textBaseline = "top";
      ctx.fillText(opts.ylabel, 0, 0);
      ctx.restore();
    }
    ctx.restore();
  }

  // Polyline through pts (array of [px, py] in PIXEL coords). opts: {color,
  // width, dash, alpha}.
  function curve(ctx, pts, opts) {
    opts = opts || {};
    if (!pts || pts.length === 0) return;
    ctx.save();
    ctx.strokeStyle = opts.color || theme().colors.ink;
    ctx.lineWidth = opts.width || 2;
    if (opts.alpha != null) ctx.globalAlpha = opts.alpha;
    ctx.lineJoin = "round";
    ctx.lineCap = "round";
    if (opts.dash) ctx.setLineDash(opts.dash);
    ctx.beginPath();
    var started = false;
    for (var i = 0; i < pts.length; i++) {
      var p = pts[i];
      if (!p || !isFinite(p[0]) || !isFinite(p[1])) {
        started = false;
        continue;
      }
      if (!started) {
        ctx.moveTo(p[0], p[1]);
        started = true;
      } else {
        ctx.lineTo(p[0], p[1]);
      }
    }
    ctx.stroke();
    ctx.restore();
  }

  // Plot a sample buffer as a waveform. opts: {xscale (index->px),
  // yscale (volts->px), color, width, every}. Decimates to ~2 pts/px.
  function wave(ctx, samples, opts) {
    opts = opts || {};
    var xs = opts.xscale, ys = opts.yscale;
    if (!xs || !ys || !samples) return;
    var n = samples.length;
    var pxSpan = Math.abs(xs.range[1] - xs.range[0]) || 1;
    var stride = Math.max(1, Math.floor(n / (pxSpan * 2)));
    var pts = [];
    for (var i = 0; i < n; i += stride) {
      pts.push([xs(i), ys(samples[i])]);
    }
    curve(ctx, pts, opts);
  }

  // Vertical stem plot (for discrete spectra). pts: [[x, y0, y1], ...].
  function stems(ctx, pts, opts) {
    opts = opts || {};
    ctx.save();
    ctx.strokeStyle = opts.color || theme().colors.audio;
    ctx.lineWidth = opts.width || 2;
    ctx.lineCap = "round";
    if (opts.alpha != null) ctx.globalAlpha = opts.alpha;
    ctx.beginPath();
    for (var i = 0; i < pts.length; i++) {
      var p = pts[i];
      if (!isFinite(p[0]) || !isFinite(p[1]) || !isFinite(p[2])) continue;
      ctx.moveTo(p[0], p[1]);
      ctx.lineTo(p[0], p[2]);
    }
    ctx.stroke();
    ctx.restore();
  }

  function hexToRgb(hex) {
    hex = (hex || "").trim();
    var m = /^#?([0-9a-f]{6})$/i.exec(hex);
    if (m) {
      var n = parseInt(m[1], 16);
      return [(n >> 16) & 255, (n >> 8) & 255, n & 255];
    }
    var rgb = /rgba?\(([^)]+)\)/.exec(hex);
    if (rgb) {
      var parts = rgb[1].split(",");
      return [parseInt(parts[0], 10), parseInt(parts[1], 10), parseInt(parts[2], 10)];
    }
    return [128, 128, 128];
  }

  // Heatmap of scalar field f(x, y) (data coords). opts: {xscale, yscale, w, h,
  // colormap (role name or hex), step}. Normalized to the sampled max.
  function heatmap(ctx, f, opts) {
    opts = opts || {};
    var xs = opts.xscale, ys = opts.yscale;
    var w = opts.w, h = opts.h;
    var t = theme();
    var colHex = t.colors[opts.colormap] || opts.colormap || t.colors.audio;
    var rgb = hexToRgb(colHex);
    var step = opts.step || 4;
    var cols = Math.ceil(w / step);
    var rows = Math.ceil(h / step);
    var vals = new Array(cols * rows);
    var maxv = -Infinity;
    for (var iy = 0; iy < rows; iy++) {
      for (var ix = 0; ix < cols; ix++) {
        var dx = xs.invert(ix * step + step / 2);
        var dy = ys.invert(iy * step + step / 2);
        var v = f(dx, dy);
        if (!isFinite(v)) v = 0;
        vals[iy * cols + ix] = v;
        if (v > maxv) maxv = v;
      }
    }
    if (!isFinite(maxv) || maxv <= 0) maxv = 1;
    ctx.save();
    for (var jy = 0; jy < rows; jy++) {
      for (var jx = 0; jx < cols; jx++) {
        var a = vals[jy * cols + jx] / maxv;
        if (a <= 0.002) continue;
        if (a > 1) a = 1;
        ctx.fillStyle = "rgba(" + rgb[0] + "," + rgb[1] + "," + rgb[2] + "," + a + ")";
        ctx.fillRect(jx * step, jy * step, step, step);
      }
    }
    ctx.restore();
  }

  // ==========================================================================
  // Controls (all keyboard-accessible; return the root element)
  // ==========================================================================

  function el(tag, cls, parent) {
    var e = document.createElement(tag);
    if (cls) e.className = cls;
    if (parent) parent.appendChild(e);
    return e;
  }

  function slider(parentEl, o) {
    o = o || {};
    var root = el("label", "qv-control", parentEl);
    var lab = el("span", "qv-control-label", root);
    lab.textContent = o.label || "";
    var input = el("input", "qv-range", root);
    input.type = "range";
    input.min = o.min;
    input.max = o.max;
    input.step = o.step != null ? o.step : "any";
    input.value = o.value != null ? o.value : o.min;
    var out = el("span", "qv-control-value", root);
    var fmt = o.fmt || function (v) { return String(v); };
    function render(v) {
      out.textContent = fmt(v);
    }
    render(parseFloat(input.value));
    input.addEventListener("input", function () {
      var v = parseFloat(input.value);
      render(v);
      if (o.onInput) o.onInput(v);
    });
    root.qvSet = function (v) {
      input.value = v;
      render(parseFloat(input.value));
    };
    root.qvGet = function () {
      return parseFloat(input.value);
    };
    return root;
  }

  function buttons(parentEl, specs) {
    var root = el("div", "qv-buttons", parentEl);
    root.qvButtons = {};
    for (var i = 0; i < specs.length; i++) {
      (function (spec) {
        var b = el("button", "qv-btn" + (spec.primary ? " qv-primary" : ""), root);
        b.type = "button";
        b.textContent = spec.label;
        if (spec.title) b.title = spec.title;
        b.addEventListener("click", function () {
          if (spec.onClick) spec.onClick();
        });
        root.qvButtons[spec.label] = b;
      })(specs[i]);
    }
    return root;
  }

  // Segmented single-choice control (e.g. waveform or filter-mode pickers).
  // o: {label, options: [{value, label}], value, onChange}.
  function segmented(parentEl, o) {
    o = o || {};
    var root = el("div", "qv-control", parentEl);
    var lab = el("span", "qv-control-label", root);
    lab.textContent = o.label || "";
    var row = el("div", "qv-seg", root);
    var current = o.value;
    var btns = {};
    function select(v) {
      current = v;
      for (var key in btns) {
        if (btns.hasOwnProperty(key)) {
          btns[key].className = "qv-seg-btn" + (key === String(v) ? " qv-seg-active" : "");
        }
      }
    }
    for (var i = 0; i < o.options.length; i++) {
      (function (opt) {
        var b = el("button", "qv-seg-btn", row);
        b.type = "button";
        b.textContent = opt.label != null ? opt.label : opt.value;
        b.addEventListener("click", function () {
          select(opt.value);
          if (o.onChange) o.onChange(opt.value);
        });
        btns[String(opt.value)] = b;
      })(o.options[i]);
    }
    select(current);
    root.qvSet = select;
    root.qvGet = function () {
      return current;
    };
    return root;
  }

  function toggle(parentEl, o) {
    o = o || {};
    var root = el("label", "qv-control qv-toggle", parentEl);
    var input = el("input", "qv-checkbox", root);
    input.type = "checkbox";
    input.checked = !!o.value;
    var lab = el("span", "qv-control-label", root);
    lab.textContent = o.label || "";
    input.addEventListener("change", function () {
      if (o.onChange) o.onChange(input.checked);
    });
    root.qvSet = function (v) {
      input.checked = !!v;
    };
    root.qvGet = function () {
      return input.checked;
    };
    return root;
  }

  function readout(parentEl, o) {
    o = o || {};
    var root = el("div", "qv-readout", parentEl);
    var lab = el("span", "qv-readout-label", root);
    lab.textContent = o.label || "";
    var val = el("span", "qv-readout-value", root);
    val.textContent = "—";
    return {
      el: root,
      set: function (txt, colorRole) {
        val.textContent = txt;
        val.style.color = colorRole ? "var(--qv-" + colorRole + ")" : "";
      }
    };
  }

  // Victor-style draggable number. Binds to a <span>. Drag horizontally to
  // change (ew-resize cursor, coral while active); also arrow-key steppable
  // when focused. onInput(value) fires on change. Returns the span, w/ .qvSet.
  function scrub(spanEl, o) {
    o = o || {};
    var min = o.min != null ? o.min : parseFloat(spanEl.getAttribute("data-min"));
    var max = o.max != null ? o.max : parseFloat(spanEl.getAttribute("data-max"));
    var step = o.step != null ? o.step : (parseFloat(spanEl.getAttribute("data-step")) || 1);
    var value = o.value != null ? o.value : (parseFloat(spanEl.getAttribute("data-value")) || min || 0);
    var fmt = o.fmt || function (v) { return String(v); };
    var decimals = (String(step).split(".")[1] || "").length;

    spanEl.className = (spanEl.className ? spanEl.className + " " : "") + "qv-scrub";
    spanEl.setAttribute("tabindex", "0");
    spanEl.setAttribute("role", "slider");
    spanEl.setAttribute("aria-valuemin", min);
    spanEl.setAttribute("aria-valuemax", max);

    function clamp(v) {
      if (min != null && v < min) v = min;
      if (max != null && v > max) v = max;
      var q = Math.round(v / step) * step;
      return decimals ? parseFloat(q.toFixed(decimals)) : q;
    }
    function render() {
      spanEl.textContent = fmt(value);
      spanEl.setAttribute("aria-valuenow", value);
    }
    function emit() {
      render();
      if (o.onInput) o.onInput(value);
    }

    var dragging = false, startX = 0, startVal = 0, dragPointer = null;
    var range = (max - min) || 1;

    // Unified pointer events with capture: one code path for mouse, touch,
    // and pen; capture keeps the drag alive when the finger leaves the span.
    // A second finger landing mid-drag (different pointerId) is ignored — it
    // must not reset startX/startVal and make the value jump.
    function onDown(e) {
      if (dragging && e.pointerId != null && e.pointerId !== dragPointer) return;
      dragging = true;
      dragPointer = e.pointerId != null ? e.pointerId : null;
      startX = e.clientX;
      startVal = value;
      spanEl.classList.add("qv-scrub-active");
      if (e.cancelable) e.preventDefault();
      if (spanEl.setPointerCapture && e.pointerId != null) {
        try { spanEl.setPointerCapture(e.pointerId); } catch (err) {}
      }
    }
    function onMove(e) {
      if (!dragging) return;
      if (dragPointer != null && e.pointerId != null && e.pointerId !== dragPointer) return;
      var dx = e.clientX - startX;
      // Coarse pointers (fingers) get a longer runway for the same range so
      // values don't fly past; ~200px mouse, ~280px touch.
      var runway = e.pointerType === "touch" ? 280 : 200;
      value = clamp(startVal + (dx / runway) * range);
      emit();
      if (e.cancelable) e.preventDefault();
    }
    function onUp(e) {
      if (dragging && dragPointer != null && e && e.pointerId != null &&
          e.pointerId !== dragPointer) {
        return; // some other pointer lifted; the drag goes on
      }
      dragging = false;
      dragPointer = null;
      spanEl.classList.remove("qv-scrub-active");
    }
    if (window.PointerEvent) {
      spanEl.addEventListener("pointerdown", onDown);
      spanEl.addEventListener("pointermove", onMove);
      spanEl.addEventListener("pointerup", onUp);
      spanEl.addEventListener("pointercancel", onUp);
    } else {
      // Legacy fallback (pre-pointer-events browsers)
      spanEl.addEventListener("mousedown", function (e) {
        onDown(e);
        window.addEventListener("mousemove", onMove);
        window.addEventListener("mouseup", function mu() {
          onUp();
          window.removeEventListener("mousemove", onMove);
          window.removeEventListener("mouseup", mu);
        });
      });
    }
    spanEl.addEventListener("keydown", function (e) {
      // stopPropagation as well as preventDefault: mdbook's book.js binds
      // ArrowLeft/ArrowRight on the document for chapter navigation, and it
      // would otherwise flip the page out from under a focused scrub.
      var d = 0;
      if (e.key === "ArrowLeft" || e.key === "ArrowDown") d = -1;
      else if (e.key === "ArrowRight" || e.key === "ArrowUp") d = 1;
      else if (e.key === "Home") { value = clamp(min); emit(); e.preventDefault(); e.stopPropagation(); return; }
      else if (e.key === "End") { value = clamp(max); emit(); e.preventDefault(); e.stopPropagation(); return; }
      if (d !== 0) {
        var m = e.shiftKey ? 10 : 1;
        value = clamp(value + d * step * m);
        emit();
        e.preventDefault();
        e.stopPropagation();
      }
    });

    value = clamp(value);
    render();
    spanEl.qvSet = function (v) {
      value = clamp(v);
      render();
    };
    spanEl.qvGet = function () {
      return value;
    };
    return spanEl;
  }

  // ==========================================================================
  // Scheduling helpers — smoothness under rapid input
  // ==========================================================================

  // coalesce(fn): returns a wrapper that runs fn at most once per animation
  // frame, with the latest arguments. Use for expensive recompute+redraw
  // paths driven by scrubs/drags: 30 input events in one frame = 1 render.
  function coalesce(fn) {
    var scheduled = false;
    var lastArgs = null;
    var lastThis = null;
    return function () {
      lastArgs = arguments;
      lastThis = this;
      if (scheduled) return;
      scheduled = true;
      window.requestAnimationFrame(function () {
        scheduled = false;
        fn.apply(lastThis, lastArgs);
      });
    };
  }

  // debounce(fn, ms): trailing-edge debounce. Use for side effects that must
  // not machine-gun (e.g. restarting audio while a scrub is dragged).
  function debounce(fn, ms) {
    var timer = null;
    return function () {
      var args = arguments;
      var self = this;
      if (timer) clearTimeout(timer);
      timer = setTimeout(function () {
        timer = null;
        fn.apply(self, args);
      }, ms || 120);
    };
  }

  // ==========================================================================
  // Animation loop
  // ==========================================================================

  var reduceMotion = false;
  if (typeof window !== "undefined" && window.matchMedia) {
    try {
      var mq = window.matchMedia("(prefers-reduced-motion: reduce)");
      reduceMotion = mq.matches;
      if (mq.addEventListener) mq.addEventListener("change", function (e) { reduceMotion = e.matches; });
    } catch (e) {}
  }

  function loop(widgetRootEl, tickFn) {
    var raf = null;
    var playing = false;
    var onscreen = true;
    var lastT = 0;
    var api = { play: play, pause: pause, step: step, playing: false, get reduced() { return reduceMotion; } };

    function frame(now) {
      if (!playing) return;
      var dt = lastT ? (now - lastT) / 1000 : 0;
      lastT = now;
      try {
        tickFn(dt);
      } catch (e) {
        pause();
        throw e;
      }
      raf = window.requestAnimationFrame(frame);
    }
    function play() {
      if (reduceMotion) return; // no autoplaying animation under reduced-motion
      if (playing) return;
      if (!onscreen || (typeof document !== "undefined" && document.hidden)) return;
      playing = true;
      api.playing = true;
      lastT = 0;
      raf = window.requestAnimationFrame(frame);
    }
    function pause() {
      playing = false;
      api.playing = false;
      if (raf) window.cancelAnimationFrame(raf);
      raf = null;
    }
    function step() {
      tickFn(0);
    }

    // Auto-pause offscreen.
    if (typeof IntersectionObserver !== "undefined" && widgetRootEl) {
      var io = new IntersectionObserver(function (entries) {
        for (var i = 0; i < entries.length; i++) {
          onscreen = entries[i].isIntersecting;
          if (!onscreen && playing) {
            pause();
            api._wasPlaying = true;
          } else if (onscreen && api._wasPlaying && !reduceMotion) {
            api._wasPlaying = false;
            play();
          }
        }
      }, { threshold: 0.01 });
      io.observe(widgetRootEl);
    }
    // Auto-pause when tab hidden.
    if (typeof document !== "undefined") {
      document.addEventListener("visibilitychange", function () {
        if (document.hidden && playing) {
          pause();
          api._wasPlaying = true;
        } else if (!document.hidden && api._wasPlaying && onscreen && !reduceMotion) {
          api._wasPlaying = false;
          play();
        }
      });
    }
    return api;
  }

  // ==========================================================================
  // Audio preview — what you see is what you hear. The plotted buffer IS the
  // played buffer: widgets render VOLTS with dsp.*, and play() divides by 5 V.
  // The AudioContext is created lazily on the first play() (a user gesture,
  // per autoplay policy). One context, one master gain, polite default level.
  // ==========================================================================

  var _actx = null;
  var _master = null;
  var _live = [];

  function audioCtx() {
    var AC = window.AudioContext || window.webkitAudioContext;
    if (!AC) return null;
    if (!_actx) {
      _actx = new AC();
      _master = _actx.createGain();
      _master.gain.value = 1;
      _master.connect(_actx.destination);
    }
    if (_actx.state === "suspended") {
      try { _actx.resume(); } catch (e) {}
    }
    return _actx;
  }

  var audio = {
    supported: typeof window !== "undefined" && !!(window.AudioContext || window.webkitAudioContext),

    // Play a Float32Array of VOLTS (±5 nominal) at sampleRate. opts: {gain
    // (default 0.25), loop, fadeSec (default 0.008)}. Returns {stop()} or null.
    play: function (samples, sampleRate, opts) {
      opts = opts || {};
      var ctx = audioCtx();
      if (!ctx || !samples || !samples.length) return null;
      audio.stop();
      var buf = ctx.createBuffer(1, samples.length, sampleRate);
      var ch = buf.getChannelData(0);
      for (var i = 0; i < samples.length; i++) {
        var v = samples[i] / AUDIO_PEAK_V;
        ch[i] = v > 1 ? 1 : v < -1 ? -1 : v;
      }
      var src = ctx.createBufferSource();
      src.buffer = buf;
      src.loop = !!opts.loop;
      var g = ctx.createGain();
      var level = opts.gain != null ? opts.gain : 0.25;
      var fade = opts.fadeSec != null ? opts.fadeSec : 0.008;
      src.connect(g);
      g.connect(_master);
      var dur = samples.length / sampleRate;
      // The envelope + source start are deferred until the context actually
      // runs: on iOS the first user tap finds the context "suspended", and a
      // source scheduled against the frozen clock sounds late (or seems dead).
      // play() still returns the handle synchronously; stop() before the
      // deferred start simply cancels it.
      var started = false;
      var cancelled = false;
      function begin() {
        if (started || cancelled) return;
        started = true;
        var t0 = ctx.currentTime;
        g.gain.setValueAtTime(0, t0);
        g.gain.linearRampToValueAtTime(level, t0 + fade);
        if (!opts.loop) {
          g.gain.setValueAtTime(level, t0 + Math.max(fade, dur - fade));
          g.gain.linearRampToValueAtTime(0, t0 + dur);
        }
        try {
          src.start();
          if (!opts.loop) src.stop(t0 + dur + 0.02);
        } catch (e) {}
      }
      if (ctx.state === "suspended") {
        // Chain the start on resume(), with a timeout fallback in case the
        // promise never settles (some WebKit builds).
        var fallback = setTimeout(begin, 300);
        var chained = false;
        try {
          var p = ctx.resume();
          if (p && p.then) {
            chained = true;
            p.then(
              function () { clearTimeout(fallback); begin(); },
              function () { clearTimeout(fallback); begin(); }
            );
          }
        } catch (e) {}
        if (!chained) { clearTimeout(fallback); begin(); }
      } else {
        begin();
      }
      var handle = {
        src: src,
        gainNode: g,
        stop: function () {
          cancelled = true;
          try {
            if (started) {
              var t = ctx.currentTime;
              g.gain.cancelScheduledValues(t);
              g.gain.setValueAtTime(g.gain.value, t);
              g.gain.linearRampToValueAtTime(0, t + fade);
              src.stop(t + fade + 0.01);
            } else {
              g.disconnect(); // never started; make sure it never will sound
            }
          } catch (e) {}
          var idx = _live.indexOf(handle);
          if (idx >= 0) _live.splice(idx, 1);
        }
      };
      src.onended = function () {
        var idx = _live.indexOf(handle);
        if (idx >= 0) _live.splice(idx, 1);
      };
      _live.push(handle);
      return handle;
    },

    stop: function () {
      var live = _live.slice();
      for (var i = 0; i < live.length; i++) {
        live[i].stop();
      }
      _live.length = 0;
    },

    playing: function () {
      return _live.length > 0;
    }
  };

  // ==========================================================================
  // Patch graph — SVG renderer for module/cable circuits, hardware-modular
  // style. Nodes are modules with typed ports; cables are bezier curves
  // colored by SignalKind (the same color algebra as the plots and prose).
  //
  // spec = {
  //   modules: [{ id, label, x, y, inputs: [{name, kind}], outputs: [...] }],
  //   cables:  [{ from: "vco.saw", to: "vcf.in", kind? }],   // kind inferred
  //   caption?: string
  // }
  // x = column index; y = vertical offset in 32-px slots. Cable kind defaults
  // to the source port's kind. Returns {el, svg, setActive(id), onNodeClick}.
  // ==========================================================================

  var NODE_W = 132;
  var HEAD_H = 26;
  var PORT_ROW = 20;
  var NODE_PAD_B = 8;
  var COL_GAP = 72;
  var Y_UNIT = 32;
  var MARGIN = 12;

  var SVG_NS = "http://www.w3.org/2000/svg";

  function svgEl(tag, attrs, parent) {
    var e = document.createElementNS(SVG_NS, tag);
    if (attrs) {
      for (var k in attrs) {
        if (attrs.hasOwnProperty(k)) e.setAttribute(k, attrs[k]);
      }
    }
    if (parent) parent.appendChild(e);
    return e;
  }

  function patchGraph(parentEl, spec, opts) {
    opts = opts || {};
    var wrap = el("div", "qv-patchgraph", parentEl);
    var svg = svgEl("svg", { class: "qv-patchgraph-svg" }, wrap);

    var layout = {}; // id -> {x, y, w, h, inPorts: {name:{x,y,kind}}, outPorts}
    var maxX = 0, maxY = 0;

    // ---- Layout ------------------------------------------------------------
    var mods = spec.modules || [];
    for (var i = 0; i < mods.length; i++) {
      var m = mods[i];
      var ins = m.inputs || [];
      var outs = m.outputs || [];
      var rows = Math.max(ins.length, outs.length);
      var h = HEAD_H + rows * PORT_ROW + (rows ? NODE_PAD_B : 0);
      var px = MARGIN + m.x * (NODE_W + COL_GAP);
      var py = MARGIN + m.y * Y_UNIT;
      var L = { m: m, x: px, y: py, w: NODE_W, h: h, inPorts: {}, outPorts: {} };
      for (var a = 0; a < ins.length; a++) {
        L.inPorts[ins[a].name] = {
          x: px,
          y: py + HEAD_H + a * PORT_ROW + PORT_ROW / 2,
          kind: ins[a].kind
        };
      }
      for (var b = 0; b < outs.length; b++) {
        L.outPorts[outs[b].name] = {
          x: px + NODE_W,
          y: py + HEAD_H + b * PORT_ROW + PORT_ROW / 2,
          kind: outs[b].kind
        };
      }
      layout[m.id] = L;
      if (px + NODE_W > maxX) maxX = px + NODE_W;
      if (py + h > maxY) maxY = py + h;
    }

    // ---- Cables (under the nodes) -------------------------------------------
    var cableLayer = svgEl("g", { class: "qv-cables" }, svg);
    var cables = spec.cables || [];
    for (var c = 0; c < cables.length; c++) {
      var cab = cables[c];
      var fp = cab.from.split(".");
      var tp = cab.to.split(".");
      var fromL = layout[fp[0]];
      var toL = layout[tp[0]];
      if (!fromL || !toL) continue;
      var p1 = fromL.outPorts[fp[1]];
      var p2 = toL.inPorts[tp[1]];
      if (!p1 || !p2) continue;
      var kind = cab.kind || p1.kind || "audio";
      var role = kindRole(kind);
      var dx = Math.max(40, Math.abs(p2.x - p1.x) / 2);
      var sag = 14 + Math.abs(p2.y - p1.y) * 0.08; // patch-cable gravity
      var d = "M" + p1.x + "," + p1.y +
        " C" + (p1.x + dx) + "," + (p1.y + sag) +
        " " + (p2.x - dx) + "," + (p2.y + sag) +
        " " + p2.x + "," + p2.y;
      svgEl("path", { d: d, class: "qv-cable qv-k-" + role }, cableLayer);
      if (opts.animate !== false) {
        svgEl("path", { d: d, class: "qv-cable-flow qv-k-" + role }, cableLayer);
      }
    }

    // ---- Nodes ---------------------------------------------------------------
    var nodeLayer = svgEl("g", { class: "qv-nodes" }, svg);
    var nodeEls = {};
    for (var ni = 0; ni < mods.length; ni++) {
      (function (m) {
        var L = layout[m.id];
        var g = svgEl("g", { class: "qv-node", "data-module": m.id }, nodeLayer);
        svgEl("rect", {
          x: L.x, y: L.y, width: L.w, height: L.h, rx: 7,
          class: "qv-node-body"
        }, g);
        svgEl("line", {
          x1: L.x, y1: L.y + HEAD_H - 4, x2: L.x + L.w, y2: L.y + HEAD_H - 4,
          class: "qv-node-headline"
        }, g);
        var title = svgEl("text", {
          x: L.x + L.w / 2, y: L.y + HEAD_H / 2 + 1,
          class: "qv-node-title",
          "text-anchor": "middle",
          "dominant-baseline": "middle"
        }, g);
        title.textContent = m.label || m.id;

        var ins = m.inputs || [];
        for (var a = 0; a < ins.length; a++) {
          var p = L.inPorts[ins[a].name];
          svgEl("circle", {
            cx: p.x, cy: p.y, r: 4.2,
            class: "qv-port qv-k-" + kindRole(ins[a].kind)
          }, g);
          var lt = svgEl("text", {
            x: p.x + 9, y: p.y + 1,
            class: "qv-port-label",
            "dominant-baseline": "middle"
          }, g);
          lt.textContent = ins[a].name;
        }
        var outs = m.outputs || [];
        for (var b = 0; b < outs.length; b++) {
          var q = L.outPorts[outs[b].name];
          svgEl("circle", {
            cx: q.x, cy: q.y, r: 4.2,
            class: "qv-port qv-k-" + kindRole(outs[b].kind)
          }, g);
          var rt = svgEl("text", {
            x: q.x - 9, y: q.y + 1,
            class: "qv-port-label",
            "text-anchor": "end",
            "dominant-baseline": "middle"
          }, g);
          rt.textContent = outs[b].name;
        }
        nodeEls[m.id] = g;
        if (opts.onNodeClick) {
          g.style.cursor = "pointer";
          g.addEventListener("click", function () {
            opts.onNodeClick(m.id);
          });
        }
      })(mods[ni]);
    }

    var W = maxX + MARGIN;
    var H = maxY + MARGIN;
    svg.setAttribute("viewBox", "0 0 " + W + " " + H);
    svg.setAttribute("preserveAspectRatio", "xMidYMid meet");
    // Cap on-screen size at natural pixel size; shrink fluidly below it.
    svg.style.maxWidth = W + "px";

    if (spec.caption) {
      var cap = el("div", "qv-caption", wrap);
      cap.textContent = spec.caption;
    }

    return {
      el: wrap,
      svg: svg,
      layout: layout,
      setActive: function (id) {
        for (var k in nodeEls) {
          if (nodeEls.hasOwnProperty(k)) {
            if (id && k === id) nodeEls[k].classList.add("qv-node-active");
            else nodeEls[k].classList.remove("qv-node-active");
          }
        }
      }
    };
  }

  // ==========================================================================
  // Widget lifecycle — lazy init via IntersectionObserver
  // ==========================================================================

  var registry = {};
  var lifecycleObserver = null;

  function ensureObserver() {
    if (lifecycleObserver || typeof IntersectionObserver === "undefined") return;
    lifecycleObserver = new IntersectionObserver(function (entries) {
      for (var i = 0; i < entries.length; i++) {
        var entry = entries[i];
        if (entry.isIntersecting) {
          maybeInit(entry.target);
        }
      }
    }, { rootMargin: "200px" });
  }

  function maybeInit(root) {
    if (!root || root._qvInited) return;
    var name = root.getAttribute("data-viz");
    var initFn = registry[name];
    if (!initFn) return; // widget script not loaded yet
    root._qvInited = true;
    if (lifecycleObserver) lifecycleObserver.unobserve(root);
    try {
      initFn(root, QuiverViz);
    } catch (e) {
      root._qvInited = false;
      if (typeof console !== "undefined") console.error("[QuiverViz] init failed for '" + name + "'", e);
    }
  }

  function scan() {
    if (typeof document === "undefined") return;
    var nodes = document.querySelectorAll(".quiver-explorable[data-viz]");
    ensureObserver();
    for (var i = 0; i < nodes.length; i++) {
      var node = nodes[i];
      if (node._qvInited || node._qvObserved) continue;
      if (lifecycleObserver) {
        node._qvObserved = true;
        lifecycleObserver.observe(node);
      } else {
        maybeInit(node); // no IO support: init eagerly
      }
    }
  }

  function register(name, initFn) {
    registry[name] = initFn;
    // A matching element may already be scrolled into view; try to init it.
    if (typeof document !== "undefined") {
      var nodes = document.querySelectorAll('.quiver-explorable[data-viz="' + name + '"]');
      for (var i = 0; i < nodes.length; i++) {
        var node = nodes[i];
        if (node._qvObserved || node._qvInited) {
          maybeInit(node);
        } else {
          ensureObserver();
          if (lifecycleObserver) {
            node._qvObserved = true;
            lifecycleObserver.observe(node);
          } else {
            maybeInit(node);
          }
        }
      }
    }
  }

  // ==========================================================================
  // Built-in widget: "patchgraph" — declarative circuit diagrams anywhere in
  // the book. Drop this into any page:
  //
  //   <div class="quiver-explorable" data-viz="patchgraph">
  //   <script type="application/json">
  //   { "modules": [...], "cables": [...], "caption": "..." }
  //   </script>
  //   </div>
  // ==========================================================================

  function readJsonChild(root) {
    var s = root.querySelector('script[type="application/json"]');
    if (!s) return null;
    try {
      return JSON.parse(s.textContent);
    } catch (e) {
      if (typeof console !== "undefined") console.error("[QuiverViz] bad patchgraph JSON", e);
      return null;
    }
  }

  // ==========================================================================
  // Public API
  // ==========================================================================

  var QuiverViz = {
    register: register,
    theme: theme,
    onThemeChange: onThemeChange,
    kindRole: kindRole,
    rng: rng,
    randn: randn,
    dsp: dsp,
    canvas: canvas,
    scale: scale,
    logScale: logScale,
    niceTicks: niceTicks,
    logTicks: logTicks,
    fmtHz: fmtHz,
    axes: axes,
    curve: curve,
    wave: wave,
    stems: stems,
    heatmap: heatmap,
    slider: slider,
    buttons: buttons,
    segmented: segmented,
    toggle: toggle,
    readout: readout,
    scrub: scrub,
    coalesce: coalesce,
    debounce: debounce,
    loop: loop,
    audio: audio,
    patchGraph: patchGraph,
    readJsonChild: readJsonChild,
    el: el,
    svgEl: svgEl,
    _registry: registry,
    _scan: scan
  };

  register("patchgraph", function (root, QV) {
    var spec = readJsonChild(root) || { modules: [], cables: [] };
    root.classList.add("qv-bare"); // graphs sit flush, without the panel chrome
    QV.patchGraph(root, spec, { animate: spec.animate !== false });
  });

  if (typeof document !== "undefined") {
    if (document.readyState === "loading") {
      document.addEventListener("DOMContentLoaded", scan);
    } else {
      scan();
    }
    if (typeof window !== "undefined") {
      window.addEventListener("load", scan);
    }
  }

  if (typeof window !== "undefined") window.QuiverViz = QuiverViz;
  if (typeof module !== "undefined" && module.exports) module.exports = QuiverViz;
})();
