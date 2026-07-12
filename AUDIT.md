# Quiver Ecosystem Audit — 2026-07-10

> Multi-agent audit of the entire Quiver ecosystem (Rust core, WASM bindings, TypeScript
> packages, browser demo, benchmarks, documentation, and repository hygiene).

## Method

- **27 specialist auditors** ran in parallel across 8 dimensions: mathematical soundness
  (10 DSP domains), correctness (4 subsystems), performance (2), API/internal elegance (3),
  completeness (3), usability (3), ecosystem positioning (1), and repo hygiene (1).
- Auditors produced **198 raw findings**, deduplicated to **181 unique findings**.
- The **45 critical/high findings** were each handed to an independent adversarial verifier
  instructed to refute them from the code: **44 confirmed, 1 refuted, 0 uncertain**.
  Verifiers recalibrated severities where the original score was inflated.
- The remaining 136 medium/low findings (plus 11 high findings past the verification cap)
  are reported unverified; each is re-checked during remediation.
- Ground truth at audit time: `cargo fmt --check` clean, `cargo clippy --all-features -D warnings`
  clean, `cargo test --all-features` **576 passed / 0 failed** (but all 8 doc-tests ignored).

## Executive summary

**What is genuinely good.** The codebase is far better than a typical 0.1.0: formatting,
clippy, and 576 tests are all clean; the ParametricEq matches the RBJ cookbook exactly;
Supersaw's PolyBLEP, the Voss–McCartney pink noise, the Freeverb port, the compressor gain
computer, the delay-line interpolation math, xoroshiro128+ and the lock-free AtomicF64 are
all verified correct. The combinator algebra is small and internally sound (reset/sample-rate
propagate through every combinator, Feedback uses a correct causal unit delay). The port/
builder API is pleasant, the mdbook is internally consistent, and the module vocabulary
(~58 modules) is unusually broad.

**Where it breaks.** The problems concentrate in the connective tissue rather than the DSP
leaf math:

1. **The real-time guarantee is false in the graph engine** — `Patch::tick()` clones the
   execution order and allocates two HashMap-backed `PortValues` per module *every sample*,
   plus an O(modules × cables) cable rescan (Q105, Q107); PolyPatch multiplies this by
   voices × unison (Q110). No denormal protection exists anywhere (Q108).
2. **PolyPatch cannot function as a polyphonic synth** — per-voice V/Oct, gate, trigger,
   velocity, and detune are written into detached structs that are never part of any voice
   graph (Q063), and releasing voices are freed one sample after note-off, truncating every
   release tail (Q064).
3. **Persistence is broken end-to-end** — 10 of 12 shipped presets panic and 2 more error
   (Q083, Q084); save/load silently loses every module parameter (Q086); malformed patch
   JSON panics instead of returning `Err` (Q180).
4. **Two stability blow-ups produce NaN in normal use** — the SVF's self-oscillation mode
   uses negative damping with no state bounding (Q009), and AnalogVco's HF-rolloff coefficient
   exceeds the one-pole stability bound for every note below ~3.8 kHz (Q043).
5. **Advertised features that don't exist** — the `simd` feature contains no SIMD (Q055);
   the Arrow 'Layer 1' cannot compose with any real DSP module (Q050); the Distortion tone
   control is a static gain (Q025); Clock's divided outputs don't divide (Q035); the
   documented no_std build does not compile (Q136).
6. **The web story has never shipped** — zero npm publishes, a publish CI that would fail
   (Q174), package entry points that never include the documented helper API (Q172), a
   helper that wires the wrong engine and yields permanent silence (Q173), and a demo that
   bypasses the package's own AudioWorklet path (Q176).
7. **Onboarding fails its one job** — no example produces audible or file audio (Q167), and
   the README quick start doesn't compile (Q151).

**Remediation — COMPLETE (2026-07-11).** All 180 findings are resolved (see the
Remediation field per finding): 166 fixed, 6 implemented (new capability), 8 documented
as intended behavior. The one refuted finding is recorded for honesty. The remediation
was itself adversarially reviewed (10 domain reviewers + per-finding verification); the
25 defects that review confirmed were fixed in the same branch. Items requiring owner
credentials (npm/crates.io publish) are prepared to the point of a single command.

## Finding counts

| | critical | high | medium | low | total |
|---|---|---|---|---|---|
| Confirmed (adversarially verified) | 1 | 19 | 20 | 4 | 44 |
| Unverified | 0 | 11 | 78 | 47 | 136 |
| Refuted | | | | | 1 |

## Dimension verdicts

### Mathematical soundness — Oscillators

Oscillator DSP is a mixed bag. Genuinely correct: the Supersaw polyBLEP residual (2t−t²−1 and (t/dt+1)²) is textbook-correct for both discontinuity edges; the pink-noise Voss-McCartney update (changed_bits = ntz(index)+1 regenerating rows 0..=ntz) is exactly the classic algorithm with proper /16 normalization; the FormantOsc resonator is a valid RBJ 0dB-peak bandpass (b0=α, b2=−α) correctly realized in DF2-transposed; Wavetable Fourier series and normalizers are right; V/Oct = base·2^V is correct everywhere and set_sample_rate rescales phase increments. The weak spots are real and audible: the flagship Vco emits fully naive (aliasing) saw/square/triangle while other oscillators are band-limited; the Supersaw "sub-octave" output is mathematically not an octave down; Karplus-Strong re-excites every sample during a multi-sample trigger and has a systematic tuning error; Wavetable has no mipmapping so it aliases above ~F6. Nothing crashes or corrupts state, but several outputs are behaviorally wrong.

### Mathematical soundness — Filters

ParametricEq is genuinely solid: I verified all three coefficient sets (peaking, low-shelf, high-shelf) against the RBJ Audio EQ Cookbook — A=10^(dB/40), alpha=sin(w0)/(2Q) for peaking and sin(w0)/2*sqrt(2) (S=1) for shelves — plus correct a0 normalization and a proper Transposed-Direct-Form-II update; frequencies are clamped to [20, 0.45*fs] so no NaN at DC/Nyquist. The two ladder/SVF cores are weaker. The SVF has a critical defect: its self-oscillation mode uses negative damping (q<0), giving state-matrix det>1 and unbounded growth to inf/NaN, because the safety soft-clip is applied only to outputs, never folded back into state — and there is no stable sustained-oscillation regime at all. The SVF cutoff is also frozen above ~fs/6, and the DiodeLadder is mistuned (naive forward-Euler one-pole instead of TPT, plus a unit-delay resonance path). Denormal protection is absent throughout.

### Mathematical soundness — Envelopes & dynamics

The dynamics/envelope detectors share a sound, conventional feed-forward peak-follower topology: correct rectification (fabs), one-pole attack/release ballistics with coef=exp(-1/(t·fs)), and the Compressor's gain computer is mathematically correct (20·log10 amplitude domain, hard-knee gain reduction = over_dB·(1−1/ratio), 10^(−GR/20), smoothing applied to the detector not the audio, makeup applied post). NoiseGate has proper open/close hysteresis (0.7 ratio). Audio is internally ±5V (VCO line 78), so the ×5 threshold scalings are consistent and reachable. The main defects: the Limiter's default soft mode is not brick-wall — it asymptotes to 2×threshold (+6 dB) and readily exceeds threshold and the ±5V range; the ADSR's decay/release parameters do not equal the actual segment durations (they scale with sustain/current level) and segments are linear; NoiseGate couples its gate ramp to the detector coefficients; and detectors leave denormal tails at silence.

### Mathematical soundness — Time-based effects

The delay/interpolation core is sound: read_interpolated uses correct wrap arithmetic (read_pos = write_pos - delay_int, mod len) with proper linear interpolation, read-before-write ordering gives exactly D samples of delay, and feedback is clamped below 1 so the lossy (lowpass) interpolation guarantees stability. Tremolo depth math is correct (unipolar, no phase inversion at depth=1), and the Reverb is a faithful Freeverb (classic mutually-prime comb tunings, sample-rate-scaled lengths, damping lowpass, 0.5 allpass feedback, predelay). Two genuine math errors stand out: (1) the Phaser's "allpass" helper is not an allpass at all — magnitude drops to 0.2 at Nyquist — so it produces moving lowpass coloration instead of phaser notches; (2) the Chorus base delay (7 ms) is smaller than its modulation depth (12.5 ms at the default depth of 0.5), so the modulated delay goes negative and is hard-clamped to 1 sample over part of every LFO cycle, distorting the sweep at default settings.

### Mathematical soundness — Nonlinear & spectral

The nonlinear/spectral processors are functional in spirit but mathematically rough. Genuinely good: PitchShifter's two-grain Hann overlap at 50% satisfies COLA (0.5(1-cos2πφ)+0.5(1+cos2πφ)=1, true unity gain); RingModulator's 4-quadrant multiply with /5 scaling is correct; Vocoder uses correct log-spaced band centers with constant-Q (bandwidth ∝ fc). Weak spots: the Distortion "tone" control is algebraically a static gain, not a filter (documented feature absent); its soft_clip Padé tanh is unbounded (grows ~x/9, exceeds ±1 for |x|>3) and input is never normalized, so level staging is inconsistent with the ±5V convention used by Bitcrusher/RingModulator; foldback uses a variable-time while-loop violating the RT guarantee; Bitcrusher's downsampler truncates fractional ratios to integer periods; Granular normalizes by sqrt(active_count) producing zipper ripple. None oversample; aliasing is undocumented across all shapers.

### Mathematical soundness — Utilities, logic & sequencing

The utility/logic/math modules are mostly simple and correct: Mixer, Offset, Attenuverter (level/5), Multiple, Min/Max (fmin/fmax), Rectifier, PrecisionAdder, VcSwitch, Comparator core compare, Crossfader (valid equal-power sqrt law), SampleAndHold, SlewLimiter (rise/fall symmetric, sample-rate-scaled), ChordMemory (correct inversion octave logic), and the plain Quantizer (correct floor-based octave + round-to-nearest with explicit +12 wrap check). However several modules have genuine defects. The most serious: ScaleQuantizer has an octave-wrap bug that maps top-of-octave notes ~11 semitones down; Clock's div2/div4 "clock divider" outputs don't actually divide (phase wraps every main cycle); and BernoulliGate's "latched" gate outputs never latch because they read a fresh per-tick output buffer. Additional real issues: Euclidean's pulses control is inert at constant step count, Clock's documented 120-BPM default actually yields ~27 BPM, gate/trigger edge thresholds are inconsistent across modules (0.5V vs 2.5V, no hysteresis), and the Arpeggiator never releases held notes.

### Mathematical soundness — Analog modeling

The saturation library is mostly sound (soft_clip is C1-continuous and bounded; tanh_sat is well-guarded against div-by-zero; component tolerances and temp coefficients are physically plausible ~100ppm/degC). But the analog modeling has one severe defect and several ad-hoc/non-physical models. Critically, HighFrequencyRolloff.apply makes a one-pole filter coefficient exceed the stability bound (a>=2) for every note below ~3.8kHz, so the AnalogVco `sin` output diverges to +/-inf within a few hundred samples for essentially all normal playing. Existing tests mask it (single-tick or inf<=inf comparisons). Secondary issues: cubic_sat has a discontinuity from a wrong knee (2/3 vs 1); the V/Oct drift is a dt-scaled (not sqrt(dt)) random walk that is both sample-rate-dependent and effectively frozen (~microcents/step); tracking error uses abs() giving a non-physical V-shape; tanh_sat has >1 origin gain. AnalogVco is a standalone reimplementation, so no double-application with base Vco.

### Mathematical soundness — Combinators & Arrow laws

The combinator algebra is small, clean, and internally correct: Chain/Parallel/Fanout/First/Second/Map/Contramap/Feedback each tick every sub-module exactly once in a fixed order, so `>>>` associativity, `first` distribution, and `&&&`/`***` consistency genuinely hold even for stateful modules. Notably, the classic \"forgot to propagate\" bug is absent — reset() and set_sample_rate() are correctly forwarded through every combinator, including Feedback (which also uses a sound causal unit delay). The weaknesses are in claims, not math. Two are significant: (1) the entire Arrow \"Layer 1\" is disconnected from the real engine — all 58 DSP modules implement GraphModule, none implement combinator::Module, no adapter exists, and the layer is used nowhere in src/ or examples/, so the documented `vco.then(vcf)` cannot compile and \"three composable layers\" is false; (2) the advertised \"type-safe signal flow\" checks only f64=f64, never SignalKind, so Audio can silently feed a V/Oct input. The category-theory framing is largely decorative marketing.

### Mathematical soundness — SIMD & RNG

rng.rs is solid: a correctly implemented xoroshiro128+ (period 2^128-1, correct jump polynomial), proper 53-bit mantissa float scaling giving true [0,1), and correct [-1,1) bipolar and probability mapping. Two real defects: it is mislabeled 'Xorshift128+' in the docs, and next_bool extracts the generator's statistically weakest low bit. There is no Gaussian code to audit. simd.rs is the weaker file. The headline problem is honesty: `simd = []` is an empty feature and no intrinsics/portable_simd exist anywhere — the 'SIMD feature' is 4x manually-unrolled bounds-checked scalar loops, and the buffer advertised as 'SIMD-aligned' is a plain 8-byte-aligned Vec<f64>. The scalar and pseudo-simd paths are numerically equivalent, so audio output is correct. Secondary real-time concerns: no denormal flushing in feedback/ring-buffer paths, and modulo (not masking) plus an unguarded new(0).write() panic. Tests assert numeric values well but lack RNG known-answer vectors and simd/scalar equivalence checks.

### Mathematical soundness — Polyphony

The allocator core is reasonable: retrigger dedup (l.234-239), LRU free-voice search, and note/V-Oct math (midi_note_to_voct, l.143) are correct, and priority/steal modes are plausible. But PolyPatch — the actual polyphonic engine — is broken. The per-voice control signals (V/Oct, gate, trigger, velocity, unison detune) are written into standalone VoiceInput structs that are never members of the voice_patches graphs, so patch.tick() never receives them: allocation state does not drive the DSP at all (critical). Separately, releasing voices are auto-freed after one sample because envelope_level is never updated inside PolyPatch, truncating all release tails. Unison detune spread is 2x the documented cents, the constant-power pan law is applied unconditionally (-3 dB on the default single-voice path and mishandles stereo voice output), and there is no gain compensation across simultaneously sounding notes. set_sample_rate does not propagate to voices.

### Correctness — Patch graph engine

The graph engine is structurally sound for the simple feed-forward case: Kahn's-algorithm topological sort is correct, cycles are detected and rejected, cable removal on node deletion prevents dangling references, slotmap keys avoid node-id reuse, and inserting a cable correctly breaks a normalled connection. However several correctness and guarantee issues exist. Most serious: tick() clones execution_order and allocates HashMap PortValues per node every sample, directly violating the advertised zero-allocation real-time guarantee (high). Input mixing sums every cable regardless of SignalKind, so two 5V gates become 10V — is_summable() exists but is never used (high). CableIds are unstable Vec indices that shift on disconnect, silently addressing the wrong cable (high). Graph mutation after compile silently freezes output, feedback patches cannot compile at all, read_output hardcodes ports 0/1, normalled inputs read the output buffer namespace, and evaluation order is HashMap-nondeterministic. Modulation math also mismatches volt-scale CV against a normalized base.

### Correctness — Serialization & presets

Persistence/introspection has solid scaffolding (validation, catalog/search, JSON schema, registry with all 63 module types) but the actual save/load and preset paths are substantially broken. Empirically, 11 of 12 shipped presets fail to build: 10 PANIC and 2 error; only "Basic Subtractive" works. Root causes are wrong port names in presets (ADSR output is "env" not "out"; Mixer inputs are "ch0.." not "in1") and non-existent type_ids ("ring_modulator"/"noise_generator" vs registered "ring_mod"/"noise"). Worse, from_def reaches these through NodeHandle::in_/out which panic! on unknown ports, so loading any hand-edited JSON crashes instead of returning PatchError. Separately, round-trip fidelity is illusory: no module overrides serialize_state (always None) and from_def ignores both module state and the parameters map, so all knob/waveform/step state is lost on save/load. Registry completeness and validation logic are genuinely good; the write/read implementation and preset data are not.

### Correctness — WASM/JS bridge

The bridge is broadly plausible — the worklet correctly instantiates WASM inside the audio thread (worklet.ts handleInit), messages are queued until ready, block processing avoids per-sample postMessage, and observer/subscription serde tags (`type`, snake_case) match the hand-written TS. But there are real correctness gaps. The most serious is a SignalKind enum serialization drift: Rust emits PascalCase ("Audio") while every TS consumer expects "audio", so cable coloring and check_compatibility silently break. A second public audio entry point (createAudioContext in index.ts) drives a main-thread engine that is never the one producing sound, yielding silence. process_block allocates a Float32Array every render quantum, violating the documented zero-allocation-in-audio-path guarantee. React hooks never call wasm-bindgen `.free()`, leaking engines, and the MIDI API is wired to nothing. Severity is concentrated in type/lifecycle drift, not memory-unsafety; the core audio path in audio.ts+worklet.ts is sound.

### Correctness — Real-time I/O & observability

The lock-free primitives are sound: AtomicF64 is a correct AtomicU64 bit-cast, no compare-exchange loops, nothing is a Mutex-in-disguise, and Send/Sync are all auto-derived (no unsafe impls) — Relaxed ordering is acceptable for the independent scalar params it's used for. The OSC layer is pure-safe-Rust construction with no raw wire-format byte parser, so there is no buffer-overrun surface. The real weaknesses are on the observability side. Two are significant: (1) the observer captures only one sample per audio block, so scope/spectrum are aliased and the spectrum's frequency axis is mislabeled by the block-size factor; (2) the same collect path runs on the AudioWorklet thread yet allocates freely (Vec/String clones, O(n) retain/remove(0)) and runs an O(n^2) DFT, contradicting the documented zero-allocation, predictable-time real-time guarantees. Cross-thread MidiState note handoff is also unsynchronized (Relaxed, no release/acquire). Remaining issues are minor polish.

### Performance — Audio tick path

The graph audio path flatly violates the advertised "zero-allocation" guarantee (README.md:36, CLAUDE.md:7,258). Patch::tick clones the execution-order Vec and allocates two HashMap-backed PortValues per module every single sample, plus a quadratic cable scan; PolyPatch multiplies this by voices×unison. There is no denormal protection anywhere despite feedback filters/reverb/delay. What is genuinely good: individual DSP modules keep their vec! allocations in constructors (not tick), biquads use efficient transposed-DF-II state, the topological sort and output buffers are precomputed at compile(), and coefficient math is generally correct. But the core engine (graph.rs) and the container types (port.rs PortValues = HashMap) are architecturally unsuited to real-time; per-sample allocation and hashing dominate. The block-processing infrastructure (simd.rs, process_block) exists but is not wired into Patch::tick and its default path also allocates. Severity is high-to-critical for the graph layer.

### Performance — Benchmarks & validation

The benchmark file is broad and superficially thorough (sample rates 44.1–192k, buffers 16–512, throughput/RTF budget commentary, correct Throughput config, reasonable black_box on inputs/returned outputs). But the validation story is largely hollow. Two headline claims are unvalidated or false: (1) polyphony benches run empty voice patches, so all 'up to 128 voices' and 'max throughput' numbers measure voice-management overhead on silent graphs, not DSP; (2) the `simd` feature is a disconnected island never called by the graph/module tick path, is never even compiled by any bench invocation, and is fake SIMD (manual 4× unrolling, no intrinsics/simd128). Real-time compliance is only eprintln'd, never asserted, and CI runs `--test`/`--no-run` so no timing is ever produced, stored, or gated. Zero-allocation is actively violated (per-sample clone + HashMap allocs) with no allocation bench to catch it. Benches profile native opt-3 while production ships size-optimized wasm without SIMD128. Expensive modules (reverb/granular/pitch/delay) are untested.

### Elegance — Public API design

Quiver's public API is clean and readable in isolation, with genuine strengths: the `PortDef`/`PortSpec` builder chain (`with_default`, `with_attenuverter`, `normalled_to`) is elegant, `SignalKind` carries useful semantic metadata, `PatchError` has a proper `Display`+`std::error::Error` impl, and the combinator layer faithfully models Arrow laws with zero-cost monomorphized structs. But the headline 'three composable layers' story is not real: Layer 1 combinators (`Module`) and Layers 2–3 (`GraphModule`) are disjoint trait universes with no adapter — no shipped DSP module implements `Module`, so combinator chains cannot enter a patch. Ergonomics lean on stringly-typed, panic-on-typo port lookups (`in_`/`out`) with unwrap()-heavy examples and no discovery helper. `set_output` hides a fragile port-id 0/1 contract, the parameter API is a near-universal no-op, two divergent compatibility functions coexist, and `tick()` allocates every sample, contradicting the zero-alloc guarantee. Solid primitives, incoherent composition story.

### Elegance — Internal code quality

src/modules.rs is functionally rich (58 hand-written modules) and does use named per-module constants (scales, comb tunings, delay maxes) and shared f64::consts for PI — genuinely good. But it is a 9915-line monolith with pervasive un-abstracted duplication and one real behavioral inconsistency. port.rs already defines the canonical SignalKind::gate_threshold() = 2.5V, yet modules.rs hardcodes thresholds 73× as `> 2.5` and several modules use `> 0.5`, so trigger/clock modules disagree on when a gate is "high." Core DSP idioms are copy-pasted: V/Oct→Hz (261.63 literal 13×), envelope coefficient exp(-1/(t·sr)) (9×), and an identical read_interpolated delay reader (4×). The Module Development Kit (mdk.rs) is never referenced by modules.rs — its ModuleTestHarness and AudioAnalysis go unused while 172 inline tests reimplement zero-crossing/RMS by hand. Highest-leverage fixes: a shared dsp/constants+helpers module and adopting the mdk harness internally.

### Elegance — Feature gates & platform

The feature-gate architecture is well-organized on paper (std implies alloc, wasm implies alloc, simd is orthogonal, prelude items are correctly #[cfg]-gated per tier with no obvious std/alloc mismatch in the gating logic itself), and Cargo.toml dependency hygiene is mostly good (serde/slotmap use default-features=false with minimal feature sets). However, the headline no_std/alloc-only claim in src/lib.rs's own doc comment is false and directly falsifiable: `cargo check --no-default-features` and `--no-default-features --features alloc` both fail to compile today (verified, reproduced with isolated target dirs), because 11 call sites in modules.rs/introspection.rs/observer.rs use std-only f64 methods instead of the libm shim used consistently elsewhere. This is unsurprising given that literally no CI job or Makefile target ever builds those tiers (all use --all-features; only the wasm feature, which happens to compile through an unrelated dependency-graph side effect, gets exercised without std). Secondary hygiene gaps: no rust-version field despite an MSRV-1.78 CI job, no [package.metadata.docs.rs] section (so wasm API is invisible on docs.rs), and an unconditional cdylib crate-type that both compounds the no_std errors and burdens ordinary builds.

### Completeness — Domain coverage

Quiver's core synthesis/patching vocabulary is genuinely broad and well-organized: ~55 modules covering oscillators (VCO, wavetable, formant, supersaw, Karplus-Strong), filters, envelopes, dynamics, a real stereo-aware Reverb and Chorus, sequencing (Euclidean, arpeggiator, step sequencer), logic/CV utilities, and a properly first-class MIDI parsing layer (io.rs MidiState, extended_io.rs MidiBuffer) - not example-only. Polyphony (polyphony.rs) and preset/serialization systems are mature. The most consequential gaps are architectural rather than missing knobs: the patch graph's per-sample tick() clones a Vec every call (an actual heap allocation contradicting the documented zero-alloc guarantee) and never uses the SIMD/AudioBlock infrastructure that already exists but sits disconnected. Domain-wise, sample playback, oversampling/anti-aliasing for nonlinear stages, WAV export, and microtuning are the clearest 'missing and important' items; mono/stereo treatment is inconsistent across time-based effects; and a couple of nice abstractions (ModulatedParam, Wavefolder) exist but are unused or orphaned. Modulation-matrix and macro-control absence is arguably fine given the Eurorack-style patch-cable model already substitutes for both.

### Completeness — Documentation

The mdbook (docs/src) is generally well-maintained and internally consistent: every SUMMARY.md entry resolves to an existing file and every file is linked (no dead/orphan pages), and its getting-started/tutorial code snippets (Patch::new(sample_rate), patch.add, .out()/.in_(), .compile(), .tick()) match the real graph.rs API verified by reading source. CLAUDE.md's module-type list also matches the actual 58 structs in modules.rs plus AnalogVco almost exactly. However, two user-facing surfaces have drifted badly: README.md's Quick Start snippet uses an entirely obsolete Patch API that will not compile against today's graph.rs, and reference/ module-catalog docs give type_id examples that don't match the real lowercase type_id() strings. Additionally, the module-reference section (reference/*.md), which is supposed to give users port names and param ranges, is missing roughly a third of implemented modules entirely (no mention in any doc page), a genuine "documented in code but undiscoverable via docs" gap. CHANGELOG is a stale placeholder and a linked DEVELOPMENT.md doesn't exist.

### Completeness — Test coverage

Test volume is high (172 tests in modules.rs alone, plus 200+ across combinator/graph/port/polyphony/serialize/simd) and some tests are genuinely rigorous -- test_vco_frequency uses zero-crossing counting for real frequency verification, and there's a deliberate '_bounded'/'_default_reset_sample_rate' pattern applied broadly across ~45 of 58 modules. However, coverage is uneven and often shallow. Six modules (Tremolo, Vibrato, Distortion, Supersaw, KarplusStrong, Euclidean, plus ScaleQuantizer) have literally zero tests. A recurring anti-pattern is asserting only 'is_finite()' or 'is_some()' on filter/EQ tests whose names imply real frequency-response verification (SVF cutoff, EQ mid-cut/high-boost using DC input) -- these are vacuous. Mid-stream set_sample_rate, NaN/Inf robustness, negative-CV quantizer paths, the documented to_def/from_def round-trip, WASM-side Rust tests, polyphony stress at realistic voice counts, and property-based testing are all absent. Most loops run only 100-1000 samples, not the 10k+ needed to catch feedback/denormal instability.

### Usability — Examples & onboarding

The Rust getting-started path is technically solid (builds clean in ~10s with all-features, only 2 trivial unused-var warnings; a coherent tutorial progression subtractive→envelope→filter_mod→fm→polyphony→sequenced_bass; a reasonable custom-module example with ~40 lines of trait boilerplate) but fails the single most important job of an audio library's onboarding: no example ever produces sound. quick_taste and every tutorial print only numeric peak/RMS statistics to the console; there is no wav-file writer or speaker playback anywhere in examples/, and no hound/cpal/rodio dependency exists in the crate. A newcomer can run every example in the repo and never hear a note - the only way to actually hear Quiver is to leave Rust entirely for the separate Node/Vite browser demo. Compounding this, the README's own Quick Start snippet uses an API that no longer exists (Patch::new() with no args, add_module(), 4-arg connect()) and would fail to compile if pasted verbatim. Getting-started/simple_patch are near-duplicate examples adding no new concept.

### Usability — TypeScript/web packages

The TypeScript/web layer looks polished on the surface (strict tsconfigs, a workspaces root, an exports map for @quiver/types/react, a CI publish workflow, a visually rich demo) but is not actually usable by an outside JS developer today. Nothing has ever been published (0 git tags, registry 404s for @quiver/wasm and @quiver/types), and the publish CI itself would fail (npm ci with no root lockfile). Worse, @quiver/wasm's package.json points main/module/types at the raw wasm-bindgen glue file, not at the hand-written helper API (createEngine, createAudioContext, createQuiverAudioNode) in src/ — that source is never compiled or wired in, so the documented entry points are unreachable, and even the in-repo @quiver/react consumer would hit a runtime TypeError importing them. The flagship browser demo doesn't use the package's own AudioWorklet path at all, instead reaching across a relative path into a sibling package and driving audio with a deprecated ScriptProcessorNode, contradicting its own CLAUDE.md. React hooks are reasonably idiomatic but re-declare a parallel QuiverEngine type instead of importing the real one, and lack "use client" for RSC frameworks. Net: good bones, broken wiring end-to-end.

### Usability — Diagnostics & failure UX

Quiver's diagnostics are inconsistent: the JSON-serialization error path (serialize.rs ValidationError/PatchError::CompilationFailed) is genuinely good, with named modules/ports. But the hot path a user actually exercises when wiring a patch by hand is weak: connect() failures (PatchError::InvalidNode/InvalidPort) carry zero identifying information, tick() before/after compile() silently returns (0.0, 0.0) forever instead of erroring, and NodeHandle::out()/in_() panic on a typo'd port name — the latter is also reachable from from_def() when loading untrusted patch JSON, defeating its Result<Self, PatchError> contract entirely. Signal-kind mismatch is silent by default (ValidationMode::None), directly contradicting the project's own how-to doc, which additionally shows several APIs (PatchError::PortNotFound, disconnect_port, cables_to) that do not exist in the crate, all inside ```rust,ignore``` blocks the doc-test harness never compiles. Debug impls are missing on the top-level Patch type. Outside these hot paths, unwrap()/panic!/expect() density in non-test code is low and mostly benign.

### Usefulness — Ecosystem positioning

Quiver has a genuinely appealing pitch (typed Eurorack voltage semantics + Rust safety + WASM/React pipeline) and unusually thorough docs/module inventory for a 0.1.0 project, but on inspection the pitch does not hold together as sold and adoption is blocked by several concrete gaps. The flagship "three-layer architecture" is largely aspirational: the Layer-1 Arrow combinators (Chain/Parallel/Fanout) implement a `Module` trait that has no bridge to the `GraphModule` trait Layer-3's `Patch` actually requires, so the two headline APIs cannot be composed together despite being marketed as unified. The README's own quick-start snippet doesn't match the real API. Nothing in the repo (examples, crate deps) produces audible output or a WAV file — "hear Quiver in action" is not literally true for the core crate — and there is no cpal/rodio/portaudio integration, meaning real usage requires users to build their own audio backend from scratch, unlike fundsp/dasp/kira which integrate or ship with output paths. The npm packages are unpublished (404) and the crate isn't verifiably on crates.io either. GitHub's own contributor list shows only the owner and an automated "claude" account — there is no external community. The category-theory framing is intellectually interesting but is not accompanied by comparisons to fundsp (the closest real competitor, which already does typed Arrow-audio combinators and ships to crates.io) or any other named alternative, so a prospective adopter has no way to see why they'd pick this over an established, published library. Net: promising design ideas, unconvincing as something to depend on today.

### Repo hygiene & ground truth

Ground truth: cargo fmt --check passed clean (no diff). cargo clippy --all-features -- -D warnings passed clean (no warnings/errors). cargo test --all-features: 576 passed, 0 failed, 0 ignored (unit/integration), plus doc-tests: 0 passed, 0 failed, 8 ignored (all doc examples are marked `ignore` and never actually execute, undermining the 'doc tests are part of the test suite' claim in CLAUDE.md). CI workflows genuinely match CLAUDE.md's claims for MSRV (1.78, main-branch only) and coverage (80% line threshold via cargo-llvm-cov, main-branch only) - no drift there. The main real hygiene defect is packages/@quiver/wasm/dist/: it's a stale, untracked build directory (dated Jan 2026, containing index.js/audio.js/worklet chunks from a different bundler) that doesn't match the package.json 'files'/'main' fields (which point to quiver.js/quiver.d.ts/quiver_bg.wasm produced by 'make wasm' via wasm-pack) or the current .gitignore (which ignores the root-level wasm-pack outputs but not dist/). Versions are coherent (0.1.0 everywhere). LICENSE (MIT) matches Cargo.toml and all three package.json license fields. git-cliff config and Makefile changelog target work; .github/CHANGELOG.md exists. Pre-commit hook setup works correctly since .githooks/pre-commit exists (the Makefile's inline echo fallback is unreachable in practice, so not a real bug). Cargo.toml is missing 'readme' and 'documentation' fields, a minor crates.io publish-readiness gap.

## Finding index

| ID | Sev | Status | Location | Title |
|---|---|---|---|---|
| [Q083](#q083--) | critical | confirmed | `src/presets.rs:383` | 10 of 12 built-in presets panic on build(); 2 more error — nearly the entire preset librar |
| [Q002](#q002--) | high | confirmed | `src/modules.rs:2357` | Karplus-Strong re-excites every sample while the trigger gate is high (no edge detection) |
| [Q009](#q009--) | high | confirmed | `src/modules.rs:294` | SVF self-oscillation drives internal state to infinity/NaN (clip never fed back into state |
| [Q034](#q034--) | high | confirmed | `src/modules.rs:2448` | ScaleQuantizer maps top-of-octave notes ~11 semitones down (octave-wrap bug) |
| [Q043](#q043--) | high | confirmed | `src/analog.rs:444` | AnalogVco sin output diverges to infinity: HF rolloff one-pole coefficient exceeds stabili |
| [Q063](#q063--) | high | confirmed | `src/polyphony.rs:619` | Per-voice control signals never reach the voice patches (PolyPatch produces uncontrolled a |
| [Q064](#q064--) | high | confirmed | `src/polyphony.rs:124` | Release tails truncated: releasing voices freed one sample after note-off |
| [Q075](#q075--) | high | confirmed | `src/graph.rs:382` | CableId is an unstable Vec index; disconnect/remove shift indices and silently invalidate  |
| [Q084](#q084--) | high | confirmed | `src/presets.rs:627` | Two presets reference non-existent module type_ids (ring_modulator, noise_generator) |
| [Q086](#q086--) | high | confirmed | `src/serialize.rs:1230` | Round-trip loses ALL module parameter state: serialize_state always None and from_def igno |
| [Q091](#q091--) | high | confirmed | `src/port.rs:22` | SignalKind serializes PascalCase in Rust but all TS expects snake_case |
| [Q107](#q107--) | high | confirmed | `src/graph.rs:692` | gather_inputs rescans all cables for every input port every sample |
| [Q112](#q112--) | high | confirmed | `benches/audio_performance.rs:434` | Polyphony benchmarks run EMPTY voice patches — the entire polyphony/max-voices story measu |
| [Q136](#q136--) | high | confirmed | `src/modules.rs:2368` | no_std / alloc-only build documented in lib.rs does not compile |
| [Q137](#q137--) | high | confirmed | `.github/workflows/ci.yml:114` | CI and Makefile never exercise the documented no_std/alloc-only feature tiers |
| [Q151](#q151--) | high | confirmed | `README.md:132` | README Quick Start example uses a Patch API that no longer exists |
| [Q172](#q172--) | high | confirmed | `packages/@quiver/wasm/package.json:5` | @quiver/wasm's documented helper API (createEngine/createAudioContext) is never built or w |
| [Q173](#q173--) | high | confirmed | `packages/@quiver/wasm/src/index.ts:85` | createAudioContext() wires the caller to the wrong engine instance and will never produce  |
| [Q174](#q174--) | high | confirmed | `.github/workflows/publish-npm.yml:54` | Zero packages ever published; the publish CI is itself broken |
| [Q180](#q180--) | high | confirmed | `src/serialize.rs:1327` | from_def() can panic on malformed patch JSON instead of returning a Result |
| [Q000](#q000--) | medium | confirmed | `src/modules.rs:81` | Vco produces fully naive, aliasing saw/square/triangle (no PolyBLEP) |
| [Q001](#q001--) | medium | confirmed | `src/modules.rs:2259` | Supersaw sub-oscillator is not an octave down; it is the fundamental with a DC-offset half |
| [Q014](#q014--) | medium | confirmed | `src/modules.rs:1213` | Limiter soft mode (the default) is not brick-wall: output asymptotes to 2×threshold |
| [Q020](#q020--) | medium | confirmed | `src/modules.rs:1745` | Phaser "allpass" stage is not an allpass (magnitude not flat) |
| [Q021](#q021--) | medium | confirmed | `src/modules.rs:1074` | Chorus modulation depth exceeds base delay → negative delay hard-clamped at default settin |
| [Q025](#q025--) | medium | confirmed | `src/modules.rs:2123` | Distortion "tone" control is a static gain, not a filter |
| [Q026](#q026--) | medium | confirmed | `src/modules.rs:2053` | soft_clip is unbounded and input is never normalized (level staging) |
| [Q035](#q035--) | medium | confirmed | `src/modules.rs:3478` | Clock div2/div4 outputs do not actually divide the clock |
| [Q036](#q036--) | medium | confirmed | `src/modules.rs:4237` | BernoulliGate latched gate outputs never latch |
| [Q050](#q050--) | medium | confirmed | `src/combinator.rs:103` | Arrow/combinator "Layer 1" is disconnected from the real engine; documented composition is |
| [Q093](#q093--) | medium | confirmed | `src/wasm/engine.rs:469` | process_block allocates a Float32Array every render quantum (violates zero-alloc guarantee |
| [Q099](#q099--) | medium | confirmed | `src/observer.rs:426` | Observer decimates scope/spectrum to one sample per block, aliasing all audio-rate signals |
| [Q100](#q100--) | medium | confirmed | `src/observer.rs:428` | collect_from_patch allocates on the audio worklet thread, violating zero-alloc guarantee |
| [Q105](#q105--) | medium | confirmed | `src/graph.rs:668` | Patch::tick clones the entire execution-order Vec every sample |
| [Q108](#q108--) | medium | confirmed | `src/modules.rs:295` | No denormal protection in feedback DSP paths |
| [Q114](#q114--) | medium | confirmed | `.github/workflows/ci.yml:126` | Real-time compliance is only printed, never asserted; CI never measures or stores timings |
| [Q116](#q116--) | medium | confirmed | `benches/audio_performance.rs:625` | SIMD benchmarks never enable the simd feature, and the impl is fake SIMD |
| [Q121](#q121--) | medium | confirmed | `src/graph.rs:746` | set_output/read_output hardcode output port ids 0 and 1, breaking the codebase's own numbe |
| [Q129](#q129--) | medium | confirmed | `src/modules.rs:2627` | Gate/trigger threshold inconsistent across modules; canonical port.rs helper ignored |
| [Q167](#q167--) | medium | confirmed | `examples/quick_taste.rs:32` | No example produces actual audio (file or speakers) - critical for an audio library |
| [Q051](#q051--) | low | confirmed | `src/combinator.rs:4` | Combinators claim compile-time signal-type safety but carry no SignalKind |
| [Q055](#q055--) | low | confirmed | `src/simd.rs:142` | `simd` feature provides no actual SIMD — just bounds-checked scalar loops |
| [Q065](#q065--) | low | confirmed | `src/polyphony.rs:399` | Unison detune spread is double the documented cents value |
| [Q122](#q122--) | low | confirmed | `src/graph.rs:239` | Port access is stringly-typed and panics on typo; no ergonomic discovery path |
| [Q142](#q142--) | high | unverified | `src/modules.rs:1` | No sample playback / sampler module - synthesis-only despite 'software synth library' clai |
| [Q152](#q152--) | high | unverified | `docs/src/reference:1` | ~20 implemented modules have zero coverage in the module reference docs |
| [Q157](#q157--) | high | unverified | `src/modules.rs:1825` | Six DSP modules (out of ~58) have zero unit tests in modules.rs |
| [Q158](#q158--) | high | unverified | `src/modules.rs:8221` | Frequency-domain / filter-response claims are asserted with vacuous DC or finite-only chec |
| [Q160](#q160--) | high | unverified | `src/modules.rs:9712` | No test feeds NaN or Infinity into any module despite feedback state that can be permanent |
| [Q162](#q162--) | high | unverified | `src/serialize.rs:1282` | Patch::to_def / Patch::from_def (documented save/load round-trip) has zero test coverage |
| [Q175](#q175--) | high | unverified | `packages/@quiver/react/package.json:48` | @quiver/react's workspace:* dependency is incompatible with the plain-npm monorepo and wil |
| [Q176](#q176--) | high | unverified | `demos/browser/src/main.ts:3` | Flagship browser demo bypasses @quiver/wasm's AudioWorklet path entirely, contradicting it |
| [Q181](#q181--) | high | unverified | `src/graph.rs:667` | tick() before/after compile() silently returns (0.0, 0.0) forever with no error |
| [Q182](#q182--) | high | unverified | `src/graph.rs:581` | connect() validation errors omit module/port names and valid alternatives |
| [Q183](#q183--) | high | unverified | `src/graph.rs:286` | Docs claim default validation mode is Warn; code defaults to None (fully silent) |
| [Q003](#q003--) | medium | unverified | `src/modules.rs:2366` | Karplus-Strong systematic tuning error from buffer length and fractional-delay tap placeme |
| [Q004](#q004--) | medium | unverified | `src/modules.rs:2373` | Karplus-Strong DC excitation never decays (loop DC gain = 1) |
| [Q005](#q005--) | medium | unverified | `src/modules.rs:4839` | Wavetable has no mipmapping: fixed harmonic count aliases at high pitch and is dull at low |
| [Q006](#q006--) | medium | unverified | `src/modules.rs:2255` | Supersaw center saw bypasses PolyBLEP, reintroducing aliasing when mix<1 |
| [Q010](#q010--) | medium | unverified | `src/modules.rs:280` | SVF cutoff frozen above ~fs/6 (~7.3 kHz at 44.1 kHz); documented 20 kHz unreachable |
| [Q011](#q011--) | medium | unverified | `src/modules.rs:449` | DiodeLadder one-pole is naive forward-Euler (state=y), not TPT — cutoff mistuned high |
| [Q012](#q012--) | medium | unverified | `src/modules.rs:443` | DiodeLadder resonance feedback uses previous-sample output (unit-delay, non-ZDF) |
| [Q015](#q015--) | medium | unverified | `src/modules.rs:578` | ADSR decay/release parameter times do not equal actual segment durations |
| [Q016](#q016--) | medium | unverified | `src/modules.rs:1313` | NoiseGate: gate open/close ramp reuses detector coefficients and lacks a hold time |
| [Q022](#q022--) | medium | unverified | `src/modules.rs:935` | Delay time changes are not smoothed → zipper/click on modulation (DelayLine) |
| [Q027](#q027--) | medium | unverified | `src/modules.rs:6035` | Vocoder top bands collapse to f=0.99 clamp (mistuned/degenerate) |
| [Q028](#q028--) | medium | unverified | `src/modules.rs:6371` | Granular normalizes by sqrt(active_count) → amplitude zipper |
| [Q029](#q029--) | medium | unverified | `src/modules.rs:1574` | Bitcrusher fractional downsample truncates to integer periods |
| [Q030](#q030--) | medium | unverified | `src/modules.rs:2071` | Foldback distortion uses a variable-time while-loop in the audio path |
| [Q031](#q031--) | medium | unverified | `src/modules.rs:6312` | Granular pitch range doc/impl mismatch and extreme-speed buffer overrun |
| [Q037](#q037--) | medium | unverified | `src/modules.rs:2616` | Euclidean pulses control is inert unless step count changes |
| [Q038](#q038--) | medium | unverified | `src/modules.rs:3443` | Clock default CV yields ~27 BPM, not the documented 120 BPM |
| [Q040](#q040--) | medium | unverified | `src/modules.rs:5484` | Arpeggiator never releases held notes; reset input does not clear them |
| [Q044](#q044--) | medium | unverified | `src/analog.rs:88` | cubic_sat has a discontinuity at the knee (wrong threshold 2/3 instead of 1) |
| [Q045](#q045--) | medium | unverified | `src/analog.rs:369` | V/Oct drift random walk is sample-rate dependent and effectively frozen |
| [Q046](#q046--) | medium | unverified | `src/analog.rs:374` | Tracking error uses abs(octave_distance), producing a non-physical V-shaped error |
| [Q052](#q052--) | medium | unverified | `src/combinator.rs:22` | Arrow laws are asserted but never tested, and the Arrow interface is only partially implem |
| [Q056](#q056--) | medium | unverified | `src/simd.rs:25` | AudioBlock documented 'SIMD-aligned' but Vec<f64> is only 8-byte aligned |
| [Q057](#q057--) | medium | unverified | `src/simd.rs:530` | No denormal flushing in ring-buffer/block feedback paths — real-time CPU spikes |
| [Q058](#q058--) | medium | unverified | `src/rng.rs:100` | next_bool() uses the lowest bit of xoroshiro128+, its weakest bit |
| [Q066](#q066--) | medium | unverified | `src/polyphony.rs:629` | Constant-power pan applied unconditionally: -3 dB on default path and wrong for stereo voi |
| [Q067](#q067--) | medium | unverified | `src/polyphony.rs:633` | No gain compensation across simultaneously sounding notes (polyphonic sum clips) |
| [Q068](#q068--) | medium | unverified | `src/polyphony.rs:309` | Voice stealing does not prefer releasing voices over held notes |
| [Q069](#q069--) | medium | unverified | `src/polyphony.rs:506` | set_sample_rate does not propagate to voice patches or inputs |
| [Q076](#q076--) | medium | unverified | `src/graph.rs:567` | Graph mutation after compile() silently freezes output until recompiled |
| [Q077](#q077--) | medium | unverified | `src/graph.rs:621` | Feedback patches are impossible: any cycle (even through a unit delay) is rejected |
| [Q079](#q079--) | medium | unverified | `src/graph.rs:716` | Normalled inputs read the output-buffer namespace, causing id collisions and a one-sample  |
| [Q080](#q080--) | medium | unverified | `src/graph.rs:635` | Nondeterministic evaluation order from HashMap-seeded topological sort |
| [Q081](#q081--) | medium | unverified | `src/port.rs:485` | ModulatedParam.value() adds volt-scale CV to a normalized base then clamps, pinning params |
| [Q087](#q087--) | medium | unverified | `src/serialize.rs:1354` | Output-node assignment is not serialized; from_def guesses it heuristically |
| [Q088](#q088--) | medium | unverified | `schemas/patch.schema.json:71` | Schema/implementation drift: module_type enum lists 36 of 63 registered types |
| [Q089](#q089--) | medium | unverified | `src/introspection_impls.rs:15` | Introspection coverage gap: ~25 stateful modules have no ModuleIntrospection impl and it i |
| [Q094](#q094--) | medium | unverified | `packages/@quiver/react/src/hooks.ts:372` | React hooks never free wasm-bindgen engines (leak + no audio teardown) |
| [Q095](#q095--) | medium | unverified | `packages/@quiver/wasm/src/worklet.ts:176` | Structural graph mutation and compile() run on the audio thread |
| [Q096](#q096--) | medium | unverified | `src/wasm/engine.rs:512` | MIDI API is wired to nothing and unreachable through the worklet |
| [Q101](#q101--) | medium | unverified | `src/observer.rs:793` | O(n^2) hand-rolled DFT executed on the audio thread |
| [Q102](#q102--) | medium | unverified | `src/io.rs:201` | MidiState multi-field updates use Relaxed with no release/acquire, allowing torn note snap |
| [Q109](#q109--) | medium | unverified | `src/modules.rs:4682` | ParametricEq recomputes three biquads (pow/cos/sin/sqrt) unconditionally every sample |
| [Q110](#q110--) | medium | unverified | `src/polyphony.rs:626` | PolyPatch::tick multiplies the graph's per-sample allocations by voices×unison |
| [Q117](#q117--) | medium | unverified | `Cargo.toml:60` | Benchmarks profile native x86 opt-3, but production is wasm32 opt-level="z" |
| [Q118](#q118--) | medium | unverified | `Makefile:146` | WASM build enables no SIMD128 and optimizes for size, hurting real-time headroom |
| [Q119](#q119--) | medium | unverified | `benches/audio_performance.rs:1182` | Worst-case expensive modules are never benchmarked |
| [Q123](#q123--) | medium | unverified | `src/port.rs:535` | Parameter API (params/get_param/set_param) is a trait-default no-op for nearly every modul |
| [Q124](#q124--) | medium | unverified | `src/port.rs:212` | Two divergent, publicly-exported signal-compatibility APIs that disagree |
| [Q130](#q130--) | medium | unverified | `src/modules.rs:69` | Core DSP idioms copy-pasted instead of shared helpers (V/Oct, env coef, delay read) |
| [Q131](#q131--) | medium | unverified | `src/mdk.rs:668` | mdk.rs is not dogfooded — internal modules never use the Module Development Kit |
| [Q132](#q132--) | medium | unverified | `src/modules.rs:68` | Fundamental domain values are unnamed magic numbers |
| [Q138](#q138--) | medium | unverified | `Cargo.toml:4` | Missing rust-version field despite CI enforcing MSRV 1.78 |
| [Q139](#q139--) | medium | unverified | `Cargo.toml:37` | No [package.metadata.docs.rs] section; wasm-gated public API invisible on docs.rs |
| [Q143](#q143--) | medium | unverified | `src/modules.rs:2114` | Nonlinear stages have no oversampling/anti-aliasing path |
| [Q144](#q144--) | medium | unverified | `src/modules.rs:1698` | Time-modulation effects are inconsistently mono vs. stereo |
| [Q145](#q145--) | medium | unverified | `src/extended_io.rs:1` | No WAV/audio export or offline rendering capability anywhere in the crate or demo |
| [Q146](#q146--) | medium | unverified | `src/modules.rs:3300` | No microtuning/Scala support - Scale enum is a fixed 12-TET preset list |
| [Q147](#q147--) | medium | unverified | `src/port.rs:450` | ModulatedParam smoothing abstraction is defined and exported but used by zero modules |
| [Q153](#q153--) | medium | unverified | `docs/src/how-to/module-catalog.md:14` | module-catalog.md type_id examples don't match real type_id() strings |
| [Q154](#q154--) | medium | unverified | `README.md:157` | README links to a non-existent DEVELOPMENT.md |
| [Q155](#q155--) | medium | unverified | `.github/CHANGELOG.md:10` | CHANGELOG is a stale auto-gen placeholder, not current history |
| [Q159](#q159--) | medium | unverified | `src/modules.rs:7419` | No test exercises set_sample_rate() mid-stream (after audio has flowed) on any module |
| [Q161](#q161--) | medium | unverified | `src/modules.rs:3354` | Quantizer/ScaleQuantizer have no test with negative V/Oct (notes below C4) |
| [Q163](#q163--) | medium | unverified | `src/polyphony.rs:857` | Polyphony has no stress test beyond 2-4 voices; no full-voice-count contention test |
| [Q164](#q164--) | medium | unverified | `src/wasm/engine.rs:15` | src/wasm/engine.rs (618 lines, QuiverEngine) has zero native Rust #[test] functions |
| [Q166](#q166--) | medium | unverified | `src/modules.rs:9073` | Long-run stability tests are rare and short; most DSP tests run only 100-1000 samples (~2- |
| [Q169](#q169--) | medium | unverified | `examples/simple_patch.rs:11` | 'Getting Started' tier has two near-duplicate examples |
| [Q170](#q170--) | medium | unverified | `examples/tutorial_fm.rs:15` | Tutorials explain WHAT the code does but rarely WHY (no DSP rationale) |
| [Q177](#q177--) | medium | unverified | `packages/@quiver/wasm/src/index.ts:10` | QuiverEngine is only re-exported as a type, forcing a duplicated interface in @quiver/reac |
| [Q178](#q178--) | medium | unverified | `packages/@quiver/react/src/hooks.ts:1` | @quiver/react hooks lack a 'use client' directive |
| [Q184](#q184--) | medium | unverified | `docs/src/how-to/connect-modules.md:128` | Primary 'connect modules' doc shows APIs that don't exist in the crate |
| [Q185](#q185--) | medium | unverified | `src/graph.rs:186` | CycleDetected error never surfaces which nodes/modules are in the cycle |
| [Q192](#q192--) | medium | unverified | `packages/@quiver/wasm/package.json:2` | npm packages under packages/@quiver are unpublished; crates.io status is unclear from a fr |
| [Q193](#q193--) | medium | unverified | `README.md:211` | Project has no external contributor community, undermining any 'open ecosystem' positionin |
| [Q194](#q194--) | medium | unverified | `docs/src/introduction.md:29` | No positioning against the closest real competitor (fundsp) or any named alternative anywh |
| [Q195](#q195--) | medium | unverified | `.gitignore:6` | packages/@quiver/wasm/dist/ is an untracked, stale build artifact not covered by .gitignor |
| [Q196](#q196--) | medium | unverified | `src/presets.rs:10` | Doc-tests are 100% ignored, contradicting CLAUDE.md's 'doc tests are part of the test suit |
| [Q007](#q007--) | low | unverified | `src/modules.rs:70` | Vco exponential FM scales a ±5V input as ±5 octaves with no linear/through-zero FM |
| [Q008](#q008--) | low | unverified | `src/modules.rs:69` | C4 reference constant 261.63 slightly off from documented 261.6256 Hz |
| [Q017](#q017--) | low | unverified | `src/modules.rs:1207` | Dynamics detectors leave denormal tails at silence (no flush) |
| [Q018](#q018--) | low | unverified | `src/modules.rs:588` | ADSR envelope segments are linear (not exponential) and retrigger does not restart from ze |
| [Q019](#q019--) | low | unverified | `src/modules.rs:675` | VCA is attenuation-only and linear; cannot amplify |
| [Q023](#q023--) | low | unverified | `src/modules.rs:1988` | Vibrato writes before reading, giving a one-sample-shorter delay than the other delays |
| [Q024](#q024--) | low | unverified | `src/modules.rs:5655` | Reverb stereo-spread offset is a fixed sample count, not scaled with sample rate |
| [Q032](#q032--) | low | unverified | `src/modules.rs:1582` | Bitcrusher quantizer truncates (floor) → DC bias and full-scale extra level |
| [Q033](#q033--) | low | unverified | `src/modules.rs:5291` | PitchShifter high pitch-up crosses write pointer; no oversampling |
| [Q041](#q041--) | low | unverified | `src/modules.rs:3964` | Comparator/quantizers lack true hysteresis, allowing boundary chatter |
| [Q042](#q042--) | low | unverified | `src/modules.rs:2641` | Euclidean accent uses pre-rotation step counter, can accent silent steps |
| [Q047](#q047--) | low | unverified | `src/analog.rs:19` | tanh_sat origin gain exceeds unity, boosting level through the Saturator |
| [Q048](#q048--) | low | unverified | `src/analog.rs:201` | Thermal model time constants are uncalibrated; drift never settles or becomes audible |
| [Q049](#q049--) | low | unverified | `src/analog.rs:578` | asym_sat on saw is described as 'slight' but compresses amplitude ~24% |
| [Q053](#q053--) | low | unverified | `src/combinator.rs:26` | `>>>`, `***`, `&&&` are described as operators but no operator overloads exist |
| [Q054](#q054--) | low | unverified | `src/combinator.rs:330` | Feedback first-tick value and combine-argument order are undocumented |
| [Q059](#q059--) | low | unverified | `src/rng.rs:4` | RNG documented as 'Xorshift128+' but is actually xoroshiro128+ |
| [Q060](#q060--) | low | unverified | `src/simd.rs:516` | RingBuffer uses modulo, not power-of-two masking, in the per-sample audio path |
| [Q061](#q061--) | low | unverified | `src/simd.rs:513` | RingBuffer::new(0).write() panics (OOB index and modulo-by-zero) |
| [Q062](#q062--) | low | unverified | `src/rng.rs:239` | Tests omit simd/non-simd equivalence and RNG known-answer vectors |
| [Q070](#q070--) | low | unverified | `src/polyphony.rs:250` | No per-voice DSP reset on allocation or steal (state leakage / steal clicks) |
| [Q071](#q071--) | low | unverified | `src/polyphony.rs:236` | Retrigger path skips LRU update |
| [Q072](#q072--) | low | unverified | `src/polyphony.rs:33` | AllocationMode doc comments for Highest/LowestPriority are mislabeled |
| [Q082](#q082--) | low | unverified | `src/port.rs:435` | ParamRange::Exponential produces NaN when min>0 and max<=0 |
| [Q090](#q090--) | low | unverified | `src/serialize.rs:1269` | to_def discards metadata and version handling is unused (no forward-compat) |
| [Q097](#q097--) | low | unverified | `packages/@quiver/wasm/src/audio.ts:165` | dispose()/processor never free the engine or stop the processor |
| [Q098](#q098--) | low | unverified | `packages/@quiver/react/src/hooks.ts:75` | tick() typed as tuple but returns a Float64Array at runtime; doc/API name drift |
| [Q103](#q103--) | low | unverified | `src/extended_io.rs:195` | OscPattern matching is a simplified stub that mis-implements OSC wildcard semantics |
| [Q104](#q104--) | low | unverified | `src/observer.rs:709` | LevelMeterState peak-hold never truly holds after first decay |
| [Q111](#q111--) | low | unverified | `src/port.rs:514` | Block-processing path is unused and still allocates |
| [Q126](#q126--) | low | unverified | `src/graph.rs:163` | Inconsistent error/return policy: Result vs panic vs silent no-op; PatchError not non_exha |
| [Q127](#q127--) | low | unverified | `src/lib.rs:196` | Prelude glob pollution: crate-root re-export of a ~150-name prelude |
| [Q128](#q128--) | low | unverified | `src/modules.rs:751` | Constructor conventions are inconsistent, and sample_rate is passed redundantly |
| [Q133](#q133--) | low | unverified | `src/modules.rs:1640` | read_interpolated duplicated verbatim across four modules |
| [Q134](#q134--) | low | unverified | `src/modules.rs:1` | Single 9915-line modules.rs should be a module directory |
| [Q135](#q135--) | low | unverified | `src/modules.rs:66` | Inconsistent naming for edge-detection state fields (last_ vs prev_) |
| [Q140](#q140--) | low | unverified | `Cargo.toml:13` | Unconditional cdylib crate-type conflates/amplifies the no_std breakage and burdens every  |
| [Q148](#q148--) | low | unverified | `src/modules.rs:1369` | Sidechain routing exists only on Compressor; no generic ducking or shared mechanism |
| [Q149](#q149--) | low | unverified | `src/analog.rs:679` | Wavefolder is a fully working module but orphaned outside modules.rs, hurting discoverabil |
| [Q150](#q150--) | low | unverified | `src/modules.rs:1` | No mid/side encode-decode utilities for stereo-bus processing |
| [Q156](#q156--) | low | unverified | `docs/src/getting-started/installation.md:7` | MSRV inconsistent across docs (1.70 vs 1.78) |
| [Q165](#q165--) | low | unverified | `Cargo.toml:1` | No property-based testing anywhere in the crate |
| [Q171](#q171--) | low | unverified | `README.md:98` | README Feature Flags table omits `wasm` feature documented in CLAUDE.md/Cargo.toml |
| [Q179](#q179--) | low | unverified | `packages/@quiver/wasm/dist:1` | Orphaned dist/ build output in @quiver/wasm doesn't match current source or any script |
| [Q187](#q187--) | low | unverified | `src/graph.rs:257` | Patch has no Debug impl for println-style inspection |
| [Q188](#q188--) | low | unverified | `src/visual.rs:787` | SpectrumAnalyzer::peak_frequency() panics on NaN input instead of degrading gracefully |
| [Q197](#q197--) | low | unverified | `Cargo.toml:9` | Cargo.toml missing readme/documentation fields for crates.io publish readiness |

## Confirmed findings (adversarially verified)

### Q083 — 10 of 12 built-in presets panic on build(); 2 more error — nearly the entire preset library is unusable

- **Severity:** critical  |  **Status:** confirmed  |  **Dimension:** `correct-serialize`  |  **Location:** `src/presets.rs:383`
- **Remediation:** **Fixed** — All 12 presets now build (preset cable port names/type_ids corrected) and a test builds every preset (wave-e/serialize, presets.rs).

**Finding.** Verified by building every preset via Patch::from_def: only "Basic Subtractive" succeeds. Every envelope preset panics because ADSR's output port is named "env" (modules.rs:~577, PortDef id 10 "env"), but presets cable `env.out`/`env_amp.out`/`env_filter.out` (e.g. line 383,428,481,533,808,852,901). Mixer inputs are "ch0".."ch3" (modules.rs:700 format!("ch{}",i)), but moog_bass/pwm_strings use `mixer.in1`/`mixer.in2` (370-372,585-586). Wavefolder input is "threshold" (analog.rs:132) but wavefold_growl uses `folder.amount` (718). Observed panics: 'Unknown output port: out', 'Unknown input port: in1', 'Unknown input port: amount'.

**Recommendation.** Fix preset cable port names (env→env output, ch0/ch1 for mixer, threshold for wavefolder) and type_ids; add a test that builds ALL presets (current test_preset_build only builds the one working preset).

**Verifier evidence.** ADSR output port is "env" (modules.rs:531), not "out"; presets cable env.out/env_amp.out/env_filter.out (presets.rs:383,428,719). Mixer inputs are ch0..ch3 (modules.rs:700 format!("ch{}",i)) but moog_bass uses mixer.in1/in2 (presets.rs:370-372). Wavefolder input is "threshold" (analog.rs:691) but wavefold_growl uses folder.amount (presets.rs:718). NodeHandle::out/in_ PANIC on unknown ports (graph.rs:231,243), and from_def (serialize.rs:1327-1349) calls them, so build() panics despite its Result signature. test_preset_build (presets.rs:1077) only builds "Basic Subtractive", hiding the breakage. Cited line 383 is a real mismatched cable.

### Q002 — Karplus-Strong re-excites every sample while the trigger gate is high (no edge detection)

- **Severity:** high  |  **Status:** confirmed  |  **Dimension:** `math-oscillators`  |  **Location:** `src/modules.rs:2357`
- **Remediation:** **Fixed** — KarplusStrong excites only on the trigger rising edge via EdgeDetector, no longer refilling noise every sample held high (wave-b/oscillators, modules/oscillators.rs).

**Finding.** tick() uses `if trigger > 0.5` (line 2357) with no stored previous-trigger state (struct has no last_trigger field). A Trigger/gate that stays >0.5 for N samples (a 1ms pulse ≈ 44 samples at 44.1k) refills the whole buffer with fresh noise and resets write_pos every one of those samples, so the string cannot ring during the pulse and only sounds on the falling edge — output is just filtered noise while held high. Vco (line 73) and Euclidean (line 2627) correctly edge-detect; KS does not.

**Recommendation.** Add a last_trigger field and excite only on the rising edge: `if trigger > 0.5 && last_trigger <= 0.5`.

**Verifier evidence.** modules.rs:2357 `if trigger > 0.5` re-excites every sample the input stays high: it truncates/resizes the buffer, calls excite() (refilling with fresh noise, 2321-2329), and resets write_pos=0 (2362). The struct (2285-2291) has no last_trigger field, so no edge detection. Peers do edge-detect: Euclidean 2627 (`clock>0.5 && last_clock<=0.5`), and 3187/4214/7062. Trigger sources here are wide: Clock main_out is high for `phase < pulse_width` (~50% duty = thousands of samples), and gate/ADSR sources stay high many samples. Routing any of these into KS re-fills noise each sample, so it can't ring until the falling edge — output is filtered noise while held. High severity is justified: normal gate/clock patching yields qualitatively wrong output. Cited line 2357 correct.

### Q009 — SVF self-oscillation drives internal state to infinity/NaN (clip never fed back into state)

- **Severity:** high  |  **Status:** confirmed  |  **Dimension:** `math-filters`  |  **Location:** `src/modules.rs:294`
- **Remediation:** **Fixed** — Chamberlin SVF replaced by a Zavalishin TPT/ZDF core that stays bounded at resonance=1, eliminating the inf/NaN self-oscillation blow-up (wave-b/filters, modules/filters.rs).

**Finding.** Chamberlin SVF state: band+=f*high; low+=f*band with high=in-low-q*band. State matrix det = 1 - f*q, trace = 2-fq-f^2. For res>0.95 the code sets q from 0.1 down to -0.05 (line 288). q=0 occurs at res≈0.983; above that q<0 so det=1-f*q>1 → poles outside unit circle → exponential growth of self.low/self.band. The safe_clip at lines 316-319 is applied only to the OUTPUT copies; self.low/self.band (updated at 295-296) are never bounded. At f≈0.99, q=-0.05, growth ≈2.4%/sample reaches f64 max (~1e308) in ~0.7s → inf-inf → NaN, which then persists forever. Worse, for 0<q<0.1 (res 0.95-0.983) det<1 so it merely decays: there is NO regime of stable sustained oscillation.

**Recommendation.** Clamp/soft-limit self.low and self.band in state before storing (e.g. fold safe_clip back into state), and keep q strictly positive (q>0); model self-oscillation via a bounded nonlinearity inside the feedback path rather than negative damping. Add a NaN/denormal guard on state.

**Verifier evidence.** Code matches claim: q goes 0.1→-0.05 for res>0.95 (modules.rs:285-291); q=0 at res≈0.983, so res>0.983 gives q<0. State updated at 294-296; safe_clip applied only to output copies at 316-319, never fed back into self.low/self.band. State matrix det=1-f·q>1 when q<0 → unbounded linear growth (no nonlinearity in feedback). Numeric sim confirms overflow→inf-inf→NaN: cutoff CV=1.0,res=1.0 → NaN at sample 29310 (0.665s, f=0.99, 2.4%/sample); mid cutoff → 7.2s. Existing tests (test_svf_self_oscillation_bounded 20000 samples, test_svf_high_resonance_bounded 10000) pass only because they measure tanh-clipped OUTPUTS over windows too short to reach overflow. NaN persists permanently, poisoning the whole patch. Reachable via max resonance, a normal user action.

### Q034 — ScaleQuantizer maps top-of-octave notes ~11 semitones down (octave-wrap bug)

- **Severity:** high  |  **Status:** confirmed  |  **Dimension:** `math-utilities`  |  **Location:** `src/modules.rs:2448`
- **Remediation:** **Fixed** — ScaleQuantizer carries +12 when the over-the-top note wraps, fixing the ~11-semitone octave-wrap error (wave-b/utilities, modules/utilities.rs).

**Finding.** quantize_to_scale computes wrap_dist = min(dist,12-dist) to pick the nearest scale note, but sets closest = s (the raw semitone) without adding 12 when the nearest note is the root of the NEXT octave. Output is octave*12 + closest. Example: MINOR=[0,2,3,5,7,8,10], input semitone 11. Nearest are 10 (down 1) and 0-of-next-octave (up 1); wrap_dist for s=0 is min(11,1)=1, chosen first, so closest=0 and output = octave*12+0 — an 11-semitone DROP instead of +1 semitone. PENT_MAJOR/BLUES etc. hit this non-tie whenever the top note is closest to root. A chromatic sweep produces an audible wrong note nearly an octave off.

**Recommendation.** When the winning candidate is the over-the-top wrap (i.e. (12-dist)<dist), set closest = s + 12 so the octave is carried, mirroring the working Quantizer::quantize which explicitly tests semi and semi+12.

**Verifier evidence.** src/modules.rs:2448-2470: quantize_to_scale computes wrap_dist=min(12-dist,dist) but sets closest=s (raw semitone), never carrying +12 for over-the-top wraps; returns octave*12+closest. Reproduced (python trace): MINOR=[0,2,3,5,7,8,10], note=11 -> 0 (should be 10 or 12), an 11-semitone drop; note 10->10, 12->12 correct. PENT_MAJOR note 11->0 too. Reachable via normal input: tick() (2493-2507) builds relative_note=round(input*12)-root, so a B-above-root input (semitone 11) yields an audibly wrong pitch nearly an octave low. No tests cover quantize_to_scale (only serialize registry ref). Recommendation to carry +12 when (12-dist)<dist is correct.

### Q043 — AnalogVco sin output diverges to infinity: HF rolloff one-pole coefficient exceeds stability bound

- **Severity:** high  |  **Status:** confirmed  |  **Dimension:** `math-analog`  |  **Location:** `src/analog.rs:444`
- **Remediation:** **Fixed** — HighFrequencyRolloff coefficient kept within the one-pole stability bound (effective-cutoff form), stopping the AnalogVco sin divergence to inf (wave-b/analog, analog.rs).

**Finding.** apply() computes effective_coef = self.coef / freq_factor, freq_factor=(frequency/cutoff).max(0.1). base coef at 12kHz/44.1k = 0.631. The one-pole y+=a*(x-y) has pole (1-a); stable only for a<2. For any note below ~3.8kHz, freq_factor floors near 0.1, giving effective_coef=6.31, pole=-5.31. tick() calls hf_rolloff.apply(sin,freq) every sample (line 587). Simulated at C4 (261.63Hz): state reaches 1e142 after 200 samples and goes to inf/NaN within ~450. All normal notes are affected. Tests miss it (single-tick, or inf<=inf passes).

**Recommendation.** The coefficient must stay in (0,1). Modulate the cutoff (lower it for high notes), not divide the coefficient; e.g. compute effective_cutoff then recompute coef=omega/(1+omega), and clamp effective_coef to <1. Reverse the freq_factor direction so higher notes get more rolloff sensibly.

**Verifier evidence.** analog.rs:443-447: freq_factor=(freq/12000).max(0.1); effective_coef=self.coef/freq_factor.min(4.0). The .min(4.0) only caps HIGH notes; low notes floor freq_factor at 0.1, so effective_coef=coef/0.1. coef@12kHz/44.1k=0.631, so effective_coef=6.31, one-pole multiplier (1-a)=-5.31 → divergence. tick() applies it to sin every sample (line 587). Simulated: C4 state→-1e71 by n=100, -inf by n=428 (~10ms); 1000Hz same; 4000Hz stable. Unstable for all notes <~3.79kHz. Tests miss it: line 856 single apply(1.0,261.0) (6.31>0 passes); line 863 loops at 16kHz (stable). Only sin output affected; AnalogVco is opt-in, not default Vco → severity high, not critical.

### Q063 — Per-voice control signals never reach the voice patches (PolyPatch produces uncontrolled audio)

- **Severity:** high  |  **Status:** confirmed  |  **Dimension:** `math-polyphony`  |  **Location:** `src/polyphony.rs:619`
- **Remediation:** **Fixed** — VoiceController nodes are wired into each voice graph so per-voice V/Oct/gate/trigger/velocity actually drive the DSP (wave-d/polyphony, polyphony.rs).

**Finding.** PolyPatch owns voice_inputs: Vec<VoiceInput> (l.472,487) separate from voice_patches: Vec<Patch> (l.470). In tick() the allocator state is written into these VoiceInput structs via input.set_from_voice (l.601) and input.set_voct(base_voct+detune) (l.621), but nothing ever adds those VoiceInput modules into the corresponding Patch graph (no `.add(` of voice_inputs anywhere). patch.tick() at l.626 therefore processes a graph that never sees voct/gate/trigger/velocity/detune. Result: which note is played, its pitch, gate and unison detune are all dropped; PolyPatch cannot function as a polyphonic synth. voice_input_mut() returns PolyPatch's private copy, not a graph node, so users cannot wire it either.

**Recommendation.** Insert one VoiceInput module into each voice Patch during construction (patch.add) and keep its NodeId; update that in-graph module each tick instead of a detached Vec, or expose an API that returns the in-graph node id so users connect voct/gate/trigger to their oscillators/envelopes.

**Verifier evidence.** PolyPatch holds voice_patches: Vec<Patch> (polyphony.rs:470) and voice_inputs: Vec<VoiceInput> (472), built independently in new() (486-487). tick() writes allocator state into the detached Vec via input.set_from_voice (601) and input.set_voct (621), then calls patch.tick() (626) on graphs that contain no VoiceInput node. grep confirms no `.add(` of voice_inputs anywhere; voice patches are created empty. Patch::add takes a module by value and boxes it (graph.rs:317-330), so the internal Vec cannot be an in-graph node, and voice_input_mut() (518) returns the private copy, not a NodeId. No test asserts controlled output (test_poly_patch_compile_tick_output:1162 only checks no-panic). Per-voice voct/gate/trigger/velocity/detune never reach the DSP. Downgrading: total feature non-functionality, but no crash/unsafety/data loss, so high not critical.

### Q064 — Release tails truncated: releasing voices freed one sample after note-off

- **Severity:** high  |  **Status:** confirmed  |  **Dimension:** `math-polyphony`  |  **Location:** `src/polyphony.rs:124`
- **Remediation:** **Fixed** — Releasing voices are freed by an amplitude-follower gate with a grace period, so release tails complete instead of truncating (wave-d/polyphony, polyphony.rs).

**Finding.** Voice::tick auto-frees a voice when state==Releasing && envelope_level < 0.0001 (l.124). envelope_level defaults to 0.0 (l.85) and is only ever set by set_envelope_level (l.292), which PolyPatch::tick never calls. After note_off sets Releasing (l.101-105), the very next PolyPatch::tick runs allocator.tick() first (l.593), so voice.tick() sees envelope_level 0.0 < 0.0001 and immediately calls free(). The processing loop then skips Free voices (l.607). The patch's release phase is therefore never processed - the voice is cut dead one sample after gate-off, producing an abrupt click and no release.

**Recommendation.** Feed the real envelope amplitude from each voice patch into set_envelope_level every tick before allocator.tick(), or gate auto-free on the patch's own envelope output rather than an unpopulated field; guard the threshold so a never-updated level does not trigger instant free.

**Verifier evidence.** Voice.tick() auto-frees when state==Releasing && envelope_level<0.0001 (polyphony.rs:124). envelope_level defaults 0.0 (l.85), only mutated by set_envelope_level (l.292), which grep shows is called only from a unit test (l.1086), never from PolyPatch::tick. note_off sets Releasing (l.101-105) without touching envelope_level. PolyPatch::tick calls allocator.tick() first (l.593) → VoiceAllocator::tick loops voice.tick() (l.286-288), so the first tick after note-off sees 0.0<0.0001 and free()s the voice. The processing loop then skips Free voices (l.607), so patch.tick() (l.626, the actual ADSR) never runs the release. Result: voice cut one sample after gate-off — no release tail. No test exercises the release path through PolyPatch. High severity is calibrated: affects the documented polyphony API for any patch with a release stage.

### Q075 — CableId is an unstable Vec index; disconnect/remove shift indices and silently invalidate held IDs

- **Severity:** high  |  **Status:** confirmed  |  **Dimension:** `correct-graph`  |  **Location:** `src/graph.rs:382`
- **Remediation:** **Fixed** — CableIds made stable (slotmap-style) so they survive disconnect/remove instead of being unstable Vec indices (wave-b graph/port overhaul, graph.rs).

**Finding.** connect() returns `self.cables.len()-1` as the CableId (line 382), i.e. the positional index into `cables: Vec<Cable>`. disconnect() does `self.cables.remove(cable_id)` (line 508) and remove()/disconnect_ports() also use Vec retain/remove — all of which shift every later element down one. After disconnecting an earlier cable, all previously-returned CableIds now point to the wrong cable (or past-the-end). A caller storing cable ids and later calling `disconnect(id)` silently removes an unrelated connection. No generation/version guards this.

**Recommendation.** Use a slotmap/generational key for cables (as done for nodes), or a stable monotonic id stored in Cable and looked up by scan; never expose Vec positions as durable handles.

**Verifier evidence.** graph.rs:134 `pub type CableId = usize`. connect/connect_attenuated/connect_modulated all return `self.cables.len()-1` (382/404/429), a raw Vec position. disconnect (504-508) does `self.cables.remove(cable_id)`; remove() uses `cables.retain` (357); disconnect_ports uses `cables.remove(idx)` (811). All shift trailing elements, so any previously-returned CableId becomes stale after an earlier cable is removed — a subsequent `disconnect(old_id)` silently drops the wrong cable. No generation/version guard on Cable, unlike nodes which use slotmap (`self.nodes.insert`, 341). The API documents CableId as a durable handle and disconnect(CableId) as the removal path, so normal use hits this. Behavior and math exactly as claimed; severity high is calibrated.

### Q084 — Two presets reference non-existent module type_ids (ring_modulator, noise_generator)

- **Severity:** high  |  **Status:** confirmed  |  **Dimension:** `correct-serialize`  |  **Location:** `src/presets.rs:627`
- **Remediation:** **Fixed** — Preset type_ids corrected to registered names (ring_mod, noise), fixing references to non-existent ring_modulator/noise_generator (wave-e/serialize, presets.rs).

**Finding.** metallic_ring uses ModuleDef type "ring_modulator" (line 627) and noise_sweep uses "noise_generator" (line 664). The registry registers these as "ring_mod" (serialize.rs:802) and "noise" (serialize.rs:726). registry.instantiate returns None → from_def returns Err("Unknown module type: ring_modulator"/"noise_generator"). Verified at runtime. The port names (carrier/modulator/out, white) are actually correct; only the type_id strings are wrong.

**Recommendation.** Change "ring_modulator"→"ring_mod" and "noise_generator"→"noise" in presets.rs; consider making from_def validate via validate_with_registry so such errors are reported structurally.

**Verifier evidence.** presets.rs:627 uses module_type "ring_modulator" and :664 uses "noise_generator". The registry keys are "ring_mod" (serialize.rs:802) and "noise" (serialize.rs:726) — grep found no other alias. ModuleRegistry::instantiate is a plain HashMap lookup: `self.factories.get(type_id)` (serialize.rs:1083), returning None for unknown keys. from_def then errors: `.ok_or_else(|| CompilationFailed("Unknown module type: {}"))?` (serialize.rs:1292-1299). The bug is latent: the only test touching these presets (presets.rs:969-976) calls PresetLibrary::load(), which returns the PatchDef without instantiating, so from_def is never exercised. Cable port names (carrier/modulator/out, white) are unrelated to this failure. Any user building these two shipped presets via from_def hits it — high severity is calibrated.

### Q086 — Round-trip loses ALL module parameter state: serialize_state always None and from_def ignores state + parameters

- **Severity:** high  |  **Status:** confirmed  |  **Dimension:** `correct-serialize`  |  **Location:** `src/serialize.rs:1230`
- **Remediation:** **Fixed** — Round-trip now serializes module parameters via introspection and reapplies them on load, so knob/waveform/step state survives (wave-e/serialize, serialize.rs).

**Finding.** to_def stores state: module.serialize_state() (line 1230), but NO module overrides serialize_state — grep shows only the trait defaults (port.rs:554 returns None; 560 deserialize_state is a no-op) and this single call site. So state is always None. from_def (1282-1368) never calls deserialize_state and never reads def.parameters at all (grep for def.parameters: no hits). Thus a Vco with changed waveform/PWM, Svf cutoff, Reverb size, StepSequencer steps — none survive save/load; they reset to factory defaults. The PatchDef.parameters map (populated by every preset) is dead data on both write (to_def writes empty map, 1277) and read.

**Recommendation.** Either implement serialize_state/deserialize_state per stateful module and have from_def call deserialize_state(module_def.state), or make to_def populate parameters from introspection and from_def apply them via set_param/set_param_by_id.

**Verifier evidence.** Confirmed. to_def sets state: module.serialize_state() (serialize.rs:1230) but serialize_state has only the trait default returning None (port.rs:553-556); grep across src/ finds zero overrides in any module. to_def also writes parameters: StdMap::new() (empty, serialize.rs:1277). from_def (1282-1368) instantiates via registry.instantiate (factory defaults), wires cables, sets output, compiles — it never calls deserialize_state (default no-op, port.rs:560-565) and never reads def.parameters. So round-trip preserves only topology; all per-module parameter state (Vco waveform/PWM, Svf cutoff, Reverb size, StepSequencer steps) resets to defaults. presets.rs:387+ populates parameters that are never applied on load. set_param_by_id exists (introspection_impls.rs) but is unused by from_def. Severity high is appropriate: normal save/load silently loses all tweaks.

### Q091 — SignalKind serializes PascalCase in Rust but all TS expects snake_case

- **Severity:** high  |  **Status:** confirmed  |  **Dimension:** `correct-wasm`  |  **Location:** `src/port.rs:22`
- **Remediation:** **Fixed** — SignalKind now serializes snake_case (e.g. cv_bipolar, volt_per_octave), matching the TypeScript expectation (wave-b graph/port overhaul, port.rs).

**Finding.** `enum SignalKind` (src/port.rs:22-24) has derives but NO `#[serde(rename_all = "snake_case")]`, so serde/tsify emit unit-variant strings as-is: quiver.d.ts:117 generates `type SignalKind = "Audio" | "CvBipolar" | ...`. But @quiver/types/src/index.ts:91 declares `'audio' | 'cv_bipolar' | ...`, DEFAULT_SIGNAL_COLORS keys are lowercase (index.ts:721), and engine.rs parse_signal_kind (line 607-617) only accepts lowercase. A PortDef.kind returned by get_catalog/get_port_spec is "Audio"; `getSignalColor(kind)` (react/index.ts:158) indexes colors["Audio"] → undefined, and passing it back to check_compatibility returns Err "Unknown signal kind: Audio".

**Recommendation.** Add `#[serde(rename_all = "snake_case")]` to `SignalKind` (matching ObservableValue/SubscriptionTarget), regenerate quiver.d.ts, and verify PortDef.kind round-trips as 'cv_bipolar'.

**Verifier evidence.** port.rs:22-24: SignalKind has Serialize/Deserialize/Tsify but no #[serde(rename_all)], unlike port.rs:196 sibling. Serde emits unit variants PascalCase → "Audio","CvBipolar". engine.rs:60,574 get_catalog/get_port_spec serialize PortDef.kind via serde_wasm_bindgen (PascalCase); generated types.d.ts:8 confirms 'Audio'|'CvBipolar'|... and its comment (213) acknowledges PascalCase. engine.rs:607-617 parse_signal_kind accepts ONLY snake_case; check_compatibility (94-98) uses it, so a catalog kind "Audio" round-tripped → Err "Unknown signal kind: Audio". SignalColors keys snake_case (types.d.ts:11-12; @quiver/types:722), so getSignalColor("Audio")→undefined. Claim's "all TS expects snake_case" slightly overstates (wasm/dist type is PascalCase) but defect stands.

### Q107 — gather_inputs rescans all cables for every input port every sample

- **Severity:** high  |  **Status:** confirmed  |  **Dimension:** `perf-tickpath`  |  **Location:** `src/graph.rs:692`
- **Remediation:** **Fixed** — Per-input adjacency (InEdge) precomputed at compile, replacing the per-sample O(modules x cables) cable rescan in gather_inputs (wave-c/perf, graph.rs).

**Finding.** For each input port of each module, the inner loop `for cable in &self.cables { if cable.to == port_ref ... }` (graph.rs:702-712) linearly scans the full cable list. Total cost per sample is O(modules × inputs_per_module × total_cables) — effectively quadratic in patch size. A 40-module patch with 100 cables does thousands of PortRef comparisons per sample purely for routing bookkeeping.

**Recommendation.** At compile(), build a per-input adjacency list (Vec of (from PortRef index, attenuation, offset) grouped by destination input) so tick does a single pass over only the cables feeding each input.

**Verifier evidence.** graph.rs:667-682 tick() calls gather_inputs per node each sample. gather_inputs (graph.rs:692-712) loops every input port and, inside, does `for cable in &self.cables { if cable.to == port_ref ... }`. cables is `Vec<Cable>` (graph.rs:259) with no compiled per-input index; buffers is a HashMap (graph.rs:263) so each match also incurs a hash lookup (line 705). Cost/sample = O(modules × inputs_per_module × total_cables) — genuinely quadratic in patch size, entirely in the real-time path. No compile()-built adjacency exists to short-circuit it. Recommendation (per-input adjacency list at compile) is sound. High severity is defensible for a real-time zero-alloc audio lib, though the constant (cheap PortRef int compares) makes medium arguable.

### Q112 — Polyphony benchmarks run EMPTY voice patches — the entire polyphony/max-voices story measures nothing

- **Severity:** high  |  **Status:** confirmed  |  **Dimension:** `perf-bench`  |  **Location:** `benches/audio_performance.rs:434`
- **Remediation:** **Fixed** — Polyphony benchmarks now run populated voice patches with non-silence assertions instead of empty voices (wave-e/benches, benches/audio_performance.rs).

**Finding.** PolyPatch::new (polyphony.rs:486) fills voice_patches with bare Patch::new(sr) — no VCO/VCF/ADSR added. Every polyphony bench (bench_polyphony_scaling:434, bench_high_polyphony:880, bench_polyphonic_realtime:757, bench_max_throughput:1156) calls PolyPatch::new+compile+note_on but never populates a voice with modules. So per-voice patch.tick() has an empty execution_order and returns (0,0) (graph.rs:679). The '128-voice @ 48kHz' and 'max sustainable polyphony' numbers measure only allocator.tick + empty-graph overhead, i.e. zero DSP. The headline real-time-polyphony validation is meaningless.

**Recommendation.** Build each voice patch with a realistic VCO→VCF→VCA→ADSR chain (via voice_patch_mut/a template) before compile; assert non-silent output in the bench setup so an empty graph fails.

**Verifier evidence.** PolyPatch::new (polyphony.rs:486) builds voice_patches from bare `Patch::new(sr)` — no modules. Every polyphony bench (audio_performance.rs:434,462,758,880,1156, plus 565,814,1120) only calls PolyPatch::new + compile + note_on + tick; grep shows voice_patch_mut/voice_patches_mut never appear in the bench file, so no voice is ever populated. compile() (polyphony.rs:584) just compiles empty patches; note_on (563) touches only the allocator. PolyPatch::tick (592) loops active voices calling patch.tick() (graph.rs:667), which iterates an empty execution_order and returns read_output() = silence. Thus the polyphony/max-voice numbers measure allocator.tick + unison/pan loop + empty-graph overhead, zero DSP. High severity defensible within perf-bench scope (bench-only, no user runtime impact; arguably medium).

### Q136 — no_std / alloc-only build documented in lib.rs does not compile

- **Severity:** high  |  **Status:** confirmed  |  **Dimension:** `elegance-features`  |  **Location:** `src/modules.rs:2368`
- **Remediation:** **Fixed** — 12 std-only f64 call sites routed through libm so the documented no_std and alloc-only tiers compile again (B-0 modules.rs split, verified by thumbv7em CI).

**Finding.** src/lib.rs:12-19 and CLAUDE.md claim 'Without any features, the library operates in no_std mode with alloc, providing core DSP modules for embedded systems.' Verified: `cargo check --no-default-features` fails with 10 errors; `cargo check --no-default-features --features alloc` fails with 14 errors (both reproduced with isolated --target-dir). Root cause: 11 call sites use std-only inherent f64 methods (.fract/.round/.floor/.rem_euclid/.sqrt) instead of the `libm::Libm` shim used everywhere else (7 files `use libm::Libm`): modules.rs:2368,2493,4913,5249,5325,6072,6371; introspection.rs:75,141,176; observer.rs:652. Confirmed via a minimal standalone `#![no_std]` probe crate that these exact methods are unavailable without std.

**Recommendation.** Route all 11 call sites through `Libm::<f64>::{sqrt,round,floor,fract,rem_euclid}(x)` as done elsewhere in the file, matching the existing libm-shim pattern.

**Verifier evidence.** Reproduced: `cargo check --no-default-features --features alloc` fails with 14 errors, incl. E0599 at modules.rs:2368 `period.fract()`, 2493 `.round()`, 4913 `.floor()`, 5249/5325 `.rem_euclid()`, 6072 `.round()`, 6371 `.sqrt()`; introspection.rs:75,141,176; observer.rs:652 — all use std-only inherent f64 methods while libm shim (`use libm::Libm`, modules.rs:12) is used elsewhere. lib.rs:18-19 advertises no_std+alloc for embedded. But `--features wasm` (Cargo.toml:46, enables alloc not std) COMPILES because its deps link std, so the default std build and the shipped WASM/browser product both work. Only the pure embedded no_std+alloc path breaks — a documented-but-untested niche, not mainstream. Severity overstated as critical.

### Q137 — CI and Makefile never exercise the documented no_std/alloc-only feature tiers

- **Severity:** high  |  **Status:** confirmed  |  **Dimension:** `elegance-features`  |  **Location:** `.github/workflows/ci.yml:114`
- **Remediation:** **Fixed** — Added a no_std/alloc-only CI job on thumbv7em-none-eabihf so the documented feature tiers are exercised (wave-f/hygiene, .github + Makefile).

**Finding.** grep of ci.yml shows every job uses `--all-features` (fmt/clippy line 30, test line 41, build line 53, doc line 70, check line 114, llvm-cov line 143) or default features; the MSRV job (lines 104-114, pinned to 1.78) also only runs `cargo check --all-features`. Makefile mirrors this (`check`, `test`, `build`, `lint`, `coverage` all pass `--all-features`); the only non-default-feature invocations are `--no-default-features --features wasm` (wasm/wasm-dev/wasm-check targets, Makefile lines 146,152,158). CLAUDE.md itself instructs 'Testing and building should use --all-features.' Consequently the finding above (broken no_std build) has no CI signal and can silently regress indefinitely.

**Recommendation.** Add a CI job (and Makefile target) running `cargo check --no-default-features` and `cargo check --no-default-features --features alloc` on a genuine no_std-capable target (e.g. thumbv7em-none-eabihf) to actually validate the advertised embedded/no_std story.

**Verifier evidence.** Verified. ci.yml uses `--all-features` in every job (clippy:30, test:41, build:53, doc:70, MSRV check:114, coverage:143); no `--no-default-features` except wasm's browser-tests. Makefile mirrors this (check/test/build/lint/coverage all `--all-features`; only wasm targets use `--no-default-features --features wasm`, lines 146,152,158). The crate genuinely advertises no_std: src/lib.rs:21 `#![cfg_attr(not(feature="std"), no_std)]`, doc line 18. Yet running `cargo check --no-default-features` and `--features alloc` both FAIL to compile (10 and 14 errors: `f64::sqrt` not found in core, e.g. modules.rs:6371, :652). So the advertised embedded/no_std tier is currently broken with zero CI signal. Cited line 114 is a fair anchor. Severity high is justified: a headline feature is fully broken and unguarded.

### Q151 — README Quick Start example uses a Patch API that no longer exists

- **Severity:** high  |  **Status:** confirmed  |  **Dimension:** `complete-docs`  |  **Location:** `README.md:132`
- **Independently found by** 3 auditors
- **Remediation:** **Fixed** — README Quick Start rewritten against the real Patch API and is now a tested crate-level doctest (wave-f/docs, README.md/lib.rs).

**Finding.** README.md:134 `Patch::new()` (no args) vs src/graph.rs:278 `pub fn new(sample_rate: f64)`. README.md:137 `patch.add_module(Vco::new())` — no `add_module` method exists and `Vco::new()` takes no sample_rate; real API is `patch.add("vco", Vco::new(sample_rate))` (src/graph.rs:317, src/modules.rs:26 `Vco::new(sample_rate: f64)`). README.md:141-143 `patch.connect(vco, "out", vcf, "input")` (4 positional args) vs actual `connect(&mut self, from: PortRef, to: PortRef)` (src/graph.rs:369) used as `patch.connect(vco.out("saw"), vcf.in_("in")).unwrap()`. The snippet also omits the required `patch.compile()` (src/graph.rs:600) before `tick()`. This code will not compile as written, while docs/src/getting-started/first-patch.md:26-67 shows the correct current API.

**Recommendation.** Replace the README Quick Start snippet with the working example from docs/src/getting-started/first-patch.md (Patch::new(sample_rate), patch.add(name, Module::new(sample_rate)), patch.connect(a.out("x"), b.in_("y")), patch.compile(), and destructure the (left, right) tick() result).

**Verifier evidence.** README.md:132 `Patch::new()` but src/graph.rs:278 requires `new(sample_rate: f64)`. README.md:135 `patch.add_module(Vco::new())` — no `Patch::add_module` exists (only src/wasm/engine.rs:141 on the WASM engine); real method is `add(name, module)` (src/graph.rs:317), and `Vco::new` requires sample_rate (src/modules.rs:26). README.md:141 `connect(vco, "out", vcf, "input")` (4 args) vs `connect(from: PortRef, to: PortRef)` (src/graph.rs:369). README.md:146 `patch.tick()` omits required `compile()` (src/graph.rs:600). Snippet cannot compile. Severity high is calibrated: it is the README's first copy-paste Quick Start example.

### Q172 — @quiver/wasm's documented helper API (createEngine/createAudioContext) is never built or wired into the package entry

- **Severity:** high  |  **Status:** confirmed  |  **Dimension:** `usable-ts`  |  **Location:** `packages/@quiver/wasm/package.json:5`
- **Remediation:** **Fixed** — @quiver/wasm entry rebuilt with tsup so it exports the real API (createQuiverAudioNode) instead of an unbuilt helper (wave-e/wasm-ts, packages/@quiver/wasm).

**Finding.** package.json main/module/types (lines 5-7) point at quiver.js/quiver.d.ts — the raw wasm-bindgen 'web' target glue (verified: quiver.js exports only `initSync`/default init, no createEngine/createAudioContext). The hand-written helpers (createEngine, createAudioContext, initWasm) live in src/index.ts and src/audio.ts, but the package's only 'build' script (`cd ../../.. && make wasm`) just copies 4 wasm-bindgen files (Makefile:145-148); nothing compiles src/*.ts. `@quiver/react/src/hooks.ts:356` does `const { createEngine } = await import('@quiver/wasm')` — this will be `undefined`, throwing a TypeError at runtime for the project's own consumer.

**Recommendation.** Add a real bundler step (tsup/rollup) that compiles src/index.ts + src/audio.ts (re-exporting the wasm-bindgen glue) into the package's actual main/module/exports entry, and verify with an integration test that `import {createEngine} from '@quiver/wasm'` resolves to a function.

**Verifier evidence.** package.json:5-7 sets main/module=quiver.js, types=quiver.d.ts; no exports field. quiver.js exports only QuiverEngine/QuiverError/initSync/default (lines 209,827,1210-1211) — no createEngine. Helpers live only in src/index.ts:38,57,85; make wasm (Makefile:145-148) just copies 4 glue files, never compiles src/*.ts. react/src/hooks.ts:356-357 imports {createEngine} from '@quiver/wasm' then calls it → undefined → TypeError. Confirmed. Severity→high not critical: the working browser demo (demos/browser/src/main.ts:3) imports quiver.js directly with `new QuiverEngine()`, unaffected; only @quiver/react breaks — and it doesn't even declare @quiver/wasm as a dependency, so that path is already non-functional. An untracked dist/ (tsup, exports createEngine) exists but package.json doesn't reference it.

### Q173 — createAudioContext() wires the caller to the wrong engine instance and will never produce sound

- **Severity:** high  |  **Status:** confirmed  |  **Dimension:** `usable-ts`  |  **Location:** `packages/@quiver/wasm/src/index.ts:85`
- **Independently found by** 2 auditors
- **Remediation:** **Fixed** — createAudioContext replaced by worklet-routed createQuiverAudioNode that wires the correct engine instance and produces sound (wave-e/wasm-ts, packages/@quiver/wasm).

**Finding.** createAudioContext (lines 85-132) creates a QuiverEngine on the main thread (line 95) and binds the returned loadPatch/setParam (lines 120-126) to that instance. The actual audio-producing engine is a separate instance created inside AudioWorkletProcessor.handleInit (worklet.ts:148-159). loadPatch/setParam never postMessage to the worklet, so the worklet's engine (the one process() reads from, worklet.ts:264) is never configured — process() runs forever on an empty engine, i.e. calling this documented API yields permanent silence.

**Recommendation.** Route loadPatch/setParam/compile through node.port.postMessage like createQuiverAudioNode in audio.ts does, or delete createAudioContext and point users at the working audio.ts helpers exclusively.

**Verifier evidence.** Confirmed from code. index.ts:95 creates a main-thread engine; :108 only posts {type:'init'} to the worklet, which then builds its OWN engine (worklet.ts:158). The returned loadPatch/setParam (index.ts:120-126) call engine.load_patch/compile/set_param directly on the main-thread instance and never postMessage to the worklet. The worklet's engine (read by process() at worklet.ts:264 via process_block) only ever gets 'init', never a patch, so it outputs silence. audio.ts:104-158 does it correctly via node.port.postMessage. Downgrade to high: createAudioContext is a secondary exported helper (unused internally) with in-code comments flagging it "simplified/for now," and a working alternative (createQuiverAudio/createQuiverAudioNode) exists.

### Q174 — Zero packages ever published; the publish CI is itself broken

- **Severity:** high  |  **Status:** confirmed  |  **Dimension:** `usable-ts`  |  **Location:** `.github/workflows/publish-npm.yml:54`
- **Remediation:** **Fixed** — Publish CI fixed: root package-lock.json committed and the npm workflow corrected so npm ci succeeds; cutting the v0.1.0 tag is the owner action (wave-e/wasm-ts, .github).

**Finding.** `git tag -l` returns 0 tags and the npm registry returns 404 for @quiver/wasm and @quiver/types — nothing has shipped. Even if a `v*` tag were pushed, publish-npm.yml:54 runs `npm ci` at the repo root, but no root package-lock.json is tracked in git (only demos/browser/package-lock.json exists) — `npm ci` hard-fails without an existing lockfile, so the workflow would break before reaching any `npm publish` step. Today an outside JS developer has no `npm install @quiver/*` path at all; they must clone the whole Rust monorepo, install wasm-pack, and run `make wasm` since the built artifacts are gitignored (confirmed via `git status --ignored`).

**Recommendation.** Commit a root package-lock.json (or switch the workflow to `npm install`), do a real dry-run of publish-npm.yml, and cut an initial `v0.1.0` tag to validate the whole pipeline end-to-end before advertising the packages as installable.

**Verifier evidence.** `git tag -l | wc -l` = 0 → nothing ever tagged/published. publish-npm.yml:53-54 runs `npm ci` at repo root. Root package.json is tracked (workspaces monorepo) but NO root package-lock.json exists — tracked lockfiles are only demos/browser/package-lock.json and demos/browser/tests/package-lock.json (`git ls-files | grep package-lock`), and none on disk (`ls package-lock.json` → not found). `npm ci` hard-requires an existing lockfile, so the step fails before any `npm publish` (lines 76-80). Also `.gitignore:6` ignores `/pkg` (the `make wasm` output), confirming built artifacts aren't shipped. All three factual pillars hold. The `npm ci` command is line 54 (line 53 is the step name).

### Q180 — from_def() can panic on malformed patch JSON instead of returning a Result

- **Severity:** high  |  **Status:** confirmed  |  **Dimension:** `usable-errors`  |  **Location:** `src/serialize.rs:1327`
- **Independently found by** 2 auditors
- **Remediation:** **Fixed** — from_def returns Err with port context instead of panicking on malformed patch JSON (wave-b graph/port overhaul + wave-e serialize, graph.rs/serialize.rs).

**Finding.** from_def() (declared `-> Result<Self, PatchError>`) calls `from_handle.out(from_port)` / `to_handle.in_(to_port)` at lines 1327-1349. NodeHandle::out()/in_() (src/graph.rs:227-248) do `.unwrap_or_else(|| panic!("Unknown output/input port: {}", name))`. A patch JSON with a valid module type but a mistyped port name in a cable (e.g. `"vco.sawtooth"` instead of `"vco.saw"`) is exactly the kind of untrusted/hand-edited input from_def exists to validate, yet it crashes the process instead of returning `Err(PatchError::CompilationFailed(...))` like the module/name-lookup errors just above it do.

**Recommendation.** Add fallible `try_out`/`try_in_` methods on NodeHandle returning Option<PortRef>, and have from_def() map None to a descriptive PatchError (module name, requested port, list of valid ports) instead of calling the panicking out()/in_().

**Verifier evidence.** graph.rs:227-248: out()/in_() panic via unwrap_or_else(||panic!("Unknown ... port")). serialize.rs from_def() returns Result<Self,PatchError> and validates module names with ok_or_else at 1316-1322, but calls panicking out()/in_() at 1327-1349. parse_port_ref (1371-1380) only checks for a '.' separator, not port validity, so JSON with a valid module name and mistyped port (e.g. "vco.sawtooth") reaches out() and panics instead of returning Err. Claim factually correct. Downgrading to high: it's an API-contract/robustness panic (unwind-recoverable, no memory unsafety or data loss), not a critical-class defect.

### Q000 — Vco produces fully naive, aliasing saw/square/triangle (no PolyBLEP)

- **Severity:** medium  |  **Status:** confirmed  |  **Dimension:** `math-oscillators`  |  **Location:** `src/modules.rs:81`
- **Remediation:** **Fixed** — Vco saw/square/pulse/sub now band-limited with PolyBLEP and triangle with PolyBLAMP, replacing naive aliasing waveforms (wave-b/oscillators, modules/oscillators.rs).

**Finding.** The library's primary oscillator generates saw=(2·phase−1)·5, sqr=(phase<pw?5:−5), tri=(1−4|phase−0.5|)·5 with zero anti-aliasing (lines 79-82). A hard-edged saw/pulse has harmonics to infinity; at C4 with fs=44.1k every harmonic above the 84th folds back, and at higher notes aliasing dominates. This is inconsistent: Supersaw (line 2239) uses PolyBLEP and Wavetable (line 4835) is band-limited, yet the flagship Vco — advertised with FM and hard sync — is not. Hard sync (line 74) resets phase with no BLEP correction either, worsening it.

**Recommendation.** Apply PolyBLEP to saw (subtract blep at the 0/1 wrap) and to the square (subtract at both pw and 1.0 edges, add PolyBLAMP-integrated correction for triangle), reusing Supersaw::polyblep. Correct hard-sync discontinuities similarly.

**Verifier evidence.** src/modules.rs:79-82 generates naive tri/saw/sqr with no band-limiting: saw=(2·phase−1)·5, sqr=(phase<pw?5:−5), tri=(1−4|phase−0.5|)·5. No PolyBLEP/BLAMP anywhere in Vco::tick. Hard sync (l.73-74) resets phase to 0 with no discontinuity correction. Contrast confirmed: Supersaw::polyblep exists (l.2196-2206) and is applied (l.2239-2240). Hard-edged saw/pulse harmonics fold above Nyquist, aliasing rising with pitch — genuine defect for the flagship oscillator. All claimed facts reproduced. Downgrade to medium: it degrades audio fidelity (audible aliasing) but the module still runs, is deterministic, allocation-free, and outputs correct-magnitude signals — a quality/fidelity limitation, not a functional/correctness break or crash.

### Q001 — Supersaw sub-oscillator is not an octave down; it is the fundamental with a DC-offset half-range ramp

- **Severity:** medium  |  **Status:** confirmed  |  **Dimension:** `math-oscillators`  |  **Location:** `src/modules.rs:2259`
- **Remediation:** **Fixed** — Supersaw sub output is now a true octave-down band-limited saw instead of a DC-offset fundamental ramp (wave-b/oscillators, modules/oscillators.rs).

**Finding.** sub_phase = (phases[3]·0.5) % 1.0 (line 2259). phases[3] already lives in [0,1) and wraps to 0 each fundamental period, so phases[3]·0.5 ∈ [0,0.5) and the %1.0 never triggers. The sub therefore sweeps sub=2·sub_phase−1 over [−1,0) only, resetting at the SAME frequency as the center oscillator — not an octave down as documented (line 2258). Its mean is −0.5 (DC offset), and it never completes a full ramp. A true sub needs an independent accumulator advancing by dt/2 and wrapping at 1.0.

**Recommendation.** Maintain a separate sub_phase state incremented by dt/2 per sample, wrapped at 1.0, and generate the saw from it (ideally with PolyBLEP).

**Verifier evidence.** modules.rs:2259 `sub_phase = (self.phases[3]*0.5) % 1.0`. phases[3] is the center osc (DETUNE_RATIOS[3]=0.0, line 2158), advanced by dt and wrapped at 1.0 (lines 2247-2250), so it lives in [0,1). Thus phases[3]*0.5 ∈ [0,0.5); the %1.0 is a no-op. sub = 2*sub_phase-1 ∈ [−1,0), resetting to −1 exactly when phases[3] wraps — same period as the fundamental, not an octave down (contradicting the line 2258 comment). Mean −0.5 (DC offset), never a full [−1,1] ramp. Confirmed. Downgrade to medium: the defect is on the auxiliary `sub` output port 11 (line 2189), off the main output path (port 10), reachable only if a user explicitly patches it.

### Q014 — Limiter soft mode (the default) is not brick-wall: output asymptotes to 2×threshold

- **Severity:** medium  |  **Status:** confirmed  |  **Dimension:** `math-dynamics`  |  **Location:** `src/modules.rs:1213`
- **Remediation:** **Fixed** — Limiter soft mode renormalized to asymptote to threshold (brick-wall), later given a C0-continuous knee in wave-g/fixups (wave-b/dynamics, modules/dynamics.rs).

**Finding.** Soft mode is default-on (port 3 default 5.0 > 2.5, lines 1171/1197). For envelope>threshold with over=env/threshold, gain = (threshold/env)·tanh(over−1) + 1/over = (1/over)(tanh(over−1)+1). Output peak = env·gain = threshold·(tanh(over−1)+1), which → 2·threshold as over→∞. So the ‘brick-wall limiter’ (CLAUDE.md) actually permits +6 dB above threshold. With default threshold 0.8·5=4V and a summed input (mixers reach ±20V), over≈5 gives output≈4·(tanh(4)+1)≈8.0V — exceeding both the 4V threshold and the ±5V audio range. test_limiter_prevents_spikes passes only because it uses threshold 1.5V (2×=3V<5V).

**Recommendation.** Make hard clamp the default, or bound soft output to threshold: e.g. gain = threshold/env·(1+softness·tanh(...)) normalized so peak never exceeds threshold, and add a final clamp to ±threshold on output port 10.

**Verifier evidence.** Code confirms: soft is default-on (port3 default 5.0>2.5, modules.rs:1171,1197); default threshold=0.8*5=4V (:1195). Soft gain (:1213)=(1/over)tanh(over-1)+1/over; at envelope peak input=env, so peak out=threshold(tanh(over-1)+1)->2*threshold. Numeric check: thr=4,env=20 gives 7.997V; env→∞ gives 8.0V, exceeding threshold and ±5V. Not brick-wall despite CLAUDE.md "Brick-wall limiter". test_limiter_prevents_spikes (:9876) passes only via 1.5V threshold→3V≤5V. Math fully reproduced. Downgrade to medium: with defaults the limiter engages only above 4V; nominal ±1 audio never triggers it, and breaching ±5V needs atypical high-voltage/summed input.

### Q020 — Phaser "allpass" stage is not an allpass (magnitude not flat)

- **Severity:** medium  |  **Status:** confirmed  |  **Dimension:** `math-timefx`  |  **Location:** `src/modules.rs:1745`
- **Remediation:** **Fixed** — Phaser stage replaced with a true first-order allpass (x1/y1 state, flat magnitude) producing real notches (wave-b/timefx, modules/timefx.rs).

**Finding.** The helper (1745-1749): output = state + coef*(input-state); state = input + coef*(output-input). Solving the recursion gives H(z) = [coef + (1-coef)^2 z^-1] / [1 - coef(1-coef) z^-1]. For an allpass we need num = [a1, 1] mirroring den [1, a1]; here b0=coef, b1=(1-coef)^2, a1=-coef(1-coef) — no mirror. Numeric check (coef=0.5): DC gain 1.0 but Nyquist gain 0.2. So each stage is a one-pole lowpass-ish filter, not a unit-magnitude allpass. Cascading them and mixing with dry (1794-1800) yields moving lowpass coloration, not the frequency-swept notches a phaser requires. The coefficient formula (1-tan)/(1+tan) is fine; the filter topology is wrong.

**Recommendation.** Implement a true first-order allpass, e.g. store previous input xm1 and output ym1 and compute y = coef*x + xm1 - coef*ym1 (H(z)=(coef+z^-1)/(1+coef z^-1)), or a TPT allpass. Verify unit magnitude at DC and Nyquist for all coef in [-1,1].

**Verifier evidence.** Code (modules.rs:1745-1749): y=state+coef*(x-state)=coef*x+(1-coef)*state; state=x+coef*(y-x)=(1-coef)*x+coef*y. Z-transform gives H(z)=(coef+(1-coef)^2 z^-1)/(1-coef(1-coef) z^-1) — exactly the auditor's b0=coef, b1=(1-coef)^2, a1=-coef(1-coef). Numerator/denominator are not mirror pairs, so it is not an allpass. Numeric check confirms: |H(DC)|=1.0 always, but |H(Nyq)|=0.2 at coef=0.5 (and 11.0 at coef=-0.5), matching time-domain impulse-response sums (DC=1.000, Nyq=0.200). Cascade+dry-mix (1794-1800) thus adds moving lowpass coloration, not flat-magnitude swept notches. Math confirmed. Severity: audio-quality defect; effect still runs and sweeps audibly (2-stage phase reaches 180°, some notching), no crash/safety/data issue — high is overcalibrated; medium.

### Q021 — Chorus modulation depth exceeds base delay → negative delay hard-clamped at default settings

- **Severity:** medium  |  **Status:** confirmed  |  **Dimension:** `math-timefx`  |  **Location:** `src/modules.rs:1074`
- **Remediation:** **Fixed** — Chorus modulated delay made unipolar over [base, base+depth] so it never goes negative/hard-clamped at default depth (wave-b/timefx, modules/timefx.rs).

**Finding.** BASE_DELAY_MS=7 (989), MAX_MOD_DELAY_MS=25 (987). delay_samples = base + lfo*mod_depth with lfo=sin in [-1,1] (1073-1074). mod_depth_ms = depth_cv*25. At the DEFAULT depth_cv=0.5, mod_depth=12.5 ms > 7 ms base, so min delay = 7-12.5 = -5.5 ms; at depth_cv=1 it is 7-25 = -18 ms. Line 1075 clamps to 1.0 sample, so whenever sin < -7/12.5 = -0.56 the delay pins to 1 sample for a large fraction of each LFO cycle. The sweep is asymmetrically flattened/clipped, producing a distorted, one-sided chorus rather than a smooth ± pitch modulation — audible at stock settings.

**Recommendation.** Ensure base > max modulation: e.g. set base delay ~15-20 ms and cap mod_depth so base - mod_depth stays >= ~1 ms, or offset delay = base + (lfo*0.5+0.5)*mod_depth to keep it strictly positive.

**Verifier evidence.** Code confirms: BASE_DELAY_MS=7 (modules.rs:989), MAX_MOD_DELAY_MS=25 (987). tick() defaults depth_cv=0.5 (1011,1055), so mod_depth_ms=12.5 (1062). delay_samples = base + sin*mod_depth (1074), then clamp(1.0, len-1) (1075). At default depth, min delay = 7-12.5 = -5.5ms; whenever sin < -0.56 (~31% of each LFO cycle) delay pins to 1 sample — verified numerically. At depth_cv=1, min = -18ms. Sweep is asymmetrically clipped → distorted one-sided chorus at stock defaults. Buffer sizing (993) is fine on the positive side. Real behavioral bug, but it degrades audio quality rather than crashing/corrupting; 'high' overstates it — this is a medium audio-correctness defect.

### Q025 — Distortion "tone" control is a static gain, not a filter

- **Severity:** medium  |  **Status:** confirmed  |  **Dimension:** `math-nonlinear`  |  **Location:** `src/modules.rs:2123`
- **Remediation:** **Fixed** — Distortion tone control is now a real one-pole low-pass filter instead of a static gain (wave-b/nonlinear, modules/nonlinear.rs).

**Finding.** Line 2123: `filtered = distorted*tone + distorted*(1-tone)*0.7`. Algebraically this equals `distorted*(0.7 + 0.3*tone)`, a frequency-independent scalar in [0.7,1.0]. There is no state, no cutoff, no filter of any kind, yet the comment claims 'blend between original and low-passed' and the port is documented as a tone control. The user-facing tone parameter does nothing tonal; it only trims level by up to 3 dB. A tone control is a documented feature that is effectively missing.

**Recommendation.** Add a real one-pole low-pass with retained state (e.g. y += a*(x-y), a mapped from tone) and blend distorted vs low-passed, or remove/rename the parameter.

**Verifier evidence.** Confirmed. modules.rs:2024-2026: `struct Distortion { spec: PortSpec }` holds NO filter state. set_sample_rate (2130) and reset (2128) are no-ops. Line 2123 `distorted*tone + distorted*(1-tone)*0.7` algebraically = `distorted*(0.7 + 0.3*tone)`, a frequency-independent scalar in [0.7,1.0] — no cutoff, no per-sample memory, no filter. Comment (2121-2122) claims "blend between original and low-passed" and port 2 is documented "tone" (2037). So the tone parameter only trims level ≤3dB; it does nothing tonal. Claim is factually accurate. Severity "high" overstated: no crash, no unsafe, audio path unaffected — a single parameter under-delivers vs its docs. Medium is better calibrated.

### Q026 — soft_clip is unbounded and input is never normalized (level staging)

- **Severity:** medium  |  **Status:** confirmed  |  **Dimension:** `math-nonlinear`  |  **Location:** `src/modules.rs:2053`
- **Remediation:** **Fixed** — Distortion shapers normalized by ±5V (Audio convention) and bounded, fixing level staging (wave-b/nonlinear, modules/nonlinear.rs).

**Finding.** soft_clip (2053-2058) uses the Padé tanh x(27+x²)/(27+9x²). Its derivative is 9(x²-9)²/D²≥0 (monotonic) but for |x|>3 it exceeds 1 and grows like x/9 unbounded (real tanh saturates at 1). tick() never normalizes input, while RingModulator (3652) and Bitcrusher (1581) treat Audio as ±5V. With ±5V input and drive→1, gained≈55, soft_clip≈6.1V — no saturation, exceeds Audio range. hard_clip (2063) instead clamps to ±1, a 5× level drop vs surrounding ±5V modules. Staging is internally inconsistent and output is not bounded.

**Recommendation.** Normalize input by 5.0 before shaping (and rescale output), and either clamp soft_clip output or use a genuinely bounded shaper (real tanh).

**Verifier evidence.** Quiver's audio convention is ±5V, confirmed by VCO outputs *5.0 (modules.rs:79-81), NoiseGen (2775-2778), RingModulator "both inputs are ±5V" (3651-3652), Bitcrusher normalizing/rescaling by 5.0 (1581,1583). Distortion::soft_clip (2053-2058) gains input by (1+drive*10) with no /5 normalization, and tick() (2106,2115) passes raw ±5V input. The Padé approx diverges ~x/9: I reproduced soft_clip(5,drive=1)=6.16V (exceeds ±5V, barely saturates) and soft_clip(5,drive=0.5)=3.42V. hard_clip (2061-2063) clamps to ±1, a 5x level drop vs surrounding ±5V modules. No Distortion tests or docs justify a ±1 convention. Real level-staging/correctness bug, but bounded (~6V), no NaN/panic/allocation, single module → severity medium, not high.

### Q035 — Clock div2/div4 outputs do not actually divide the clock

- **Severity:** medium  |  **Status:** confirmed  |  **Dimension:** `math-utilities`  |  **Location:** `src/modules.rs:3478`
- **Remediation:** **Fixed** — Clock div2/div4 derive from an integer cycle counter so they actually divide the main clock (wave-b/utilities, modules/sequencing.rs).

**Finding.** self.phase is wrapped to [0,1) every main cycle (line 3491). div2_raw = phase*0.5 therefore only ever ranges 0..0.5 within a cycle, so floor(div2_raw)=0 always and div2_phase = phase*0.5. div2_out fires when phase*0.5 < 0.1 i.e. phase < 0.2 — once per MAIN cycle (just a 20% pulse), not at half rate. div4 similarly fires every main cycle (phase<0.4). The /2 and /4 divided clock outputs run at the same tempo as the main clock, producing wrong rhythms.

**Recommendation.** Maintain an integer cycle counter incremented on each main-phase wrap; emit div2 pulse on even counts and div4 on counts divisible by 4, or accumulate a separate free-running phase for each divisor that is not reset each main cycle.

**Verifier evidence.** src/modules.rs:3491 wraps self.phase to [0,1) every tick. At 3478-3483: div2_raw=phase*0.5∈[0,0.5)→floor=0→div2_phase=phase*0.5, so div2_out (3482) fires when phase<0.2, once per MAIN cycle (20% pulse). div4_raw=phase*0.25∈[0,0.25)→div4_phase=phase*0.25, fires when phase<0.4, also every main cycle. No cross-cycle accumulator, so /2 and /4 outputs run at main tempo — reproduced exactly as claimed. Code comment (3476) admits it's a demonstration placeholder. grep shows no tests reference div2/div4. Downgraded to medium: silent, confined to two optional divided-clock outputs, no example/default patch uses them.

### Q036 — BernoulliGate latched gate outputs never latch

- **Severity:** medium  |  **Status:** confirmed  |  **Dimension:** `math-utilities`  |  **Location:** `src/modules.rs:4237`
- **Remediation:** **Fixed** — BernoulliGate latched gate outputs now persist in struct fields instead of a fresh per-tick buffer, so they truly latch (wave-b/utilities, modules/logic.rs).

**Finding.** gate_a/gate_b are meant to latch ("until the other side is triggered") by reading the previous state via outputs.get_or(12/13, 0.0). But graph.rs:670 allocates a fresh PortValues::new() every tick and never seeds it with prior outputs, so get_or always returns 0.0. Result: gate_a is 5V only on the single sample where trig_a fires, then 0V — identical to the momentary trigger outputs. The documented latched-gate behavior is completely non-functional.

**Recommendation.** Store the latched state in struct fields (e.g. last_gate_a/last_gate_b) updated inside tick, rather than reading back from the output buffer, which the engine does not persist across ticks.

**Verifier evidence.** graph.rs:670 creates a fresh `PortValues::new()` each tick; scatter_outputs stores into self.buffers but never re-seeds the `outputs` passed to tick. In modules.rs BernoulliGate::tick, gate latching (4242/4249) uses `outputs.get_or(12/13,0.0)` — but in the fresh buffer only ports 10/11 are set earlier this tick, so 12/13 read 0.0. So on non-trigger samples gate_a/gate_b = 0.0; the gate is 5V only on the single trigger sample, never latching. The only test (modules.rs:7353) reuses one `outputs` across ticks and never asserts ports 12/13, hiding it. Recommended struct-field fix is correct. Scope: one niche CV module's secondary outputs; audio path unaffected → medium not high.

### Q050 — Arrow/combinator "Layer 1" is disconnected from the real engine; documented composition is impossible

- **Severity:** medium  |  **Status:** confirmed  |  **Dimension:** `math-combinators`  |  **Location:** `src/combinator.rs:103`
- **Independently found by** 3 auditors
- **Remediation:** **Fixed** — Bridged combinator::Module and engine GraphModule via GraphModuleAdapter/ModuleGraphAdapter so Arrow Layer-1 composition works with real DSP modules (wave-b/combinator).

**Finding.** The combinator `Module` trait (single typed `In`/`Out`, src/combinator.rs:103) is a separate world from the `GraphModule` trait every DSP module actually implements (multi-port untyped `PortValues`, src/port.rs:506). Verified: 58 `impl GraphModule` in modules.rs, 0 `impl Module`. `Vco` implements only `GraphModule` (modules.rs:57). No adapter bridges the two. Grep of src/ and examples/ finds zero uses of `.then/.parallel/.fanout/Chain/Feedback` outside combinator.rs itself. So the documented example `vco.then(vcf).then(vca)` (combinator.rs:37,51) cannot compile with real modules, and CLAUDE.md's "three composable layers" that signal "flows through" is false — the layers are disjoint. The category-theory framing is decorative, not load-bearing.

**Recommendation.** Provide a `GraphModule`<->`Module` adapter (e.g. a single-in/single-out wrapper), or use combinator `Module` as the actual module trait so VCO/VCF/etc. really compose; otherwise stop presenting combinators as a composable layer of the engine and label them a standalone utility.

**Verifier evidence.** Cited line correct: combinator.rs:103 `pub trait Module: Send` with typed In/Out, separate from GraphModule (port.rs, PortValues). Grep confirms: 58 `impl GraphModule for` in modules.rs, 0 `impl Module for`. `impl Module for` only appears on combinator internals (Chain/Parallel/Fanout/Feedback/Map/Split etc., combinator.rs:229-489). No GraphModule<->Module adapter exists in src/. Zero `.then(/.parallel(/.fanout(/Chain::/Feedback::` uses anywhere outside combinator.rs (src+examples). combinator.rs tests (line 612+) use only toy Module impls. So built-in DSP modules genuinely cannot compose via combinators. Severity overstated: no runtime/audio bug; all real examples/tutorials use Patch/graph, so normal users never hit "impossible composition." This is a documentation/architecture-accuracy defect, not high-impact.

### Q093 — process_block allocates a Float32Array every render quantum (violates zero-alloc guarantee)

- **Severity:** medium  |  **Status:** confirmed  |  **Dimension:** `correct-wasm`  |  **Location:** `src/wasm/engine.rs:469`
- **Remediation:** **Fixed** — process_block writes into a preallocated block output with a zero-copy Float32Array view (tick_block + decimated observer), ending the per-quantum allocation (wave-e/wasm-ts, wasm/engine.rs).

**Finding.** process_block does `js_sys::Float32Array::new_with_length((num_samples*2) as u32)` (line 469) on every call. The worklet calls this once per 128-sample quantum (worklet.ts:264), i.e. ~344 times/sec at 44.1kHz, each allocating a fresh JS-heap typed array on the audio thread. Plus observer.collect_from_patch(&patch) runs every block (line 481). CLAUDE.md and the wasm/CLAUDE.md both promise 'zero allocation in the audio path'; this breaks it and creates GC pressure that can cause audible xruns.

**Recommendation.** Pre-allocate a persistent Float32Array (or Vec<f32>) sized to a max block on the engine, write into it each call and return a view/copy; gate observer collection behind a decimation counter instead of every block.

**Verifier evidence.** Verified: engine.rs:469 `Float32Array::new_with_length((num_samples*2))` allocates a fresh JS typed array on every process_block call; line 481 runs `observer.collect_from_patch(&patch)` unconditionally each block. worklet.ts:243-264 calls process_block once per AudioWorklet render quantum (fixed 128 samples/quantum), i.e. audio-thread hot path. CLAUDE.md ("Zero allocation in the audio path") and wasm/CLAUDE.md both assert the guarantee. So the deviation is real, not a design misunderstanding. Severity note: the allocation is a small (~1KB) short-lived JS-boundary typed array; "audible xruns" is plausible but unproven, and tick() (engine.rs:459) itself Box-allocates too, so the guarantee is already aspirational at the binding layer. Better calibrated as medium.

### Q099 — Observer decimates scope/spectrum to one sample per block, aliasing all audio-rate signals

- **Severity:** medium  |  **Status:** confirmed  |  **Dimension:** `correct-rtio`  |  **Location:** `src/observer.rs:426`
- **Remediation:** **Fixed** — Observer no longer decimates to one sample per block; it captures the full block so Scope/Spectrum stop aliasing audio-rate signals (wave-b/rtio, observer.rs).

**Finding.** collect_from_patch reads each subscribed port exactly ONCE per call via get_output_value (lines 478-480, 565-567, 601-603). In the worklet path it is invoked once per process_block (wasm/engine.rs:481), after the whole num_samples loop — never per sample. So Scope/Spectrum accumulate 1 sample every block (typically 128), i.e. an effective capture rate of sample_rate/block_size (~344 Hz at 44.1k/128) with NO anti-alias filtering. Any signal above ~172 Hz folds into garbage. Worse, collect_spectrum sets freq_range=(0, sample_rate/2)=(0,22050) (line 614) which is off by the block-size factor and mislabels every bin. Level RMS is likewise computed over decimated samples.

**Recommendation.** Feed every sample of the block into the port buffers (loop over the block writing each tick's output), or have the graph expose a per-sample ring buffer the observer drains; compute freq_range from the true capture rate.

**Verifier evidence.** Confirmed. `collect_from_patch` (observer.rs:426) reads each port ONCE via `get_output_value`, which returns a single buffered value (graph.rs:828 `self.buffers.get(...).copied()`). It is invoked exactly once per block at engine.rs:481, AFTER the full `for i in 0..num_samples` tick loop (471-478). Grep shows no other call site. So Scope/Spectrum/Level accumulate one sample per block → effective capture rate = sample_rate/block_size (~344 Hz at 44.1k/128), no anti-aliasing; audio-rate content folds. collect_spectrum (614) labels freq_range `(0, sample_rate/2)`=(0,22050) using full rate while samples were captured at rate/block_size, so every bin is mislabeled by the block-size factor. All cited lines and math hold.

### Q100 — collect_from_patch allocates on the audio worklet thread, violating zero-alloc guarantee

- **Severity:** medium  |  **Status:** confirmed  |  **Dimension:** `correct-rtio`  |  **Location:** `src/observer.rs:428`
- **Remediation:** **Fixed** — collect_from_patch capture path rewritten to iterate without heap allocation on the audio thread (wave-b/rtio, observer.rs).

**Finding.** process_block (wasm/engine.rs:466-484) is the AudioWorklet render call; it invokes collect_from_patch every block on the audio thread. That path allocates: line 428 `self.subscriptions.clone()` clones a Vec of SubscriptionTarget (each holding Strings) every block; collect_* build ObservableValue with `node_id.into()` String allocations (e.g. 460-464, 492-497); collect_scope clones the whole samples Vec (line 575); push_update runs `retain` O(n) plus `remove(0)` O(n) shifts (lines 389,396). This directly contradicts the documented "zero allocation in the audio path" / "Avoid allocations in the audio path (tick())" guarantee (wasm/CLAUDE.md).

**Recommendation.** Move collection/serialization off the audio thread (accumulate into pre-allocated lock-free buffers, format on the UI/poll side); avoid cloning subscriptions per block; replace Vec+remove(0) with a ring buffer.

**Verifier evidence.** process_block runs on the AudioWorklet render thread (worklet.ts:264 inside process()) and calls collect_from_patch every block (engine.rs:481). collect_from_patch:428 clones the subscriptions Vec (Strings); collect_* allocate via node_id.into() (obs.rs:461-464,493-497,542,578); collect_scope clones samples (575); push_update uses retain + remove(0) (389,396). All confirmed. But: severity over-stated. process_block already allocates a Float32Array per block (469) and tick() Box-allocates (459) — the zero-alloc guarantee (wasm/CLAUDE.md) is scoped to patch.tick(), not the JS-marshaling wrapper. The extra collect allocations are gated on active subscriptions (empty-Vec clone doesn't heap-allocate), so non-subscribed audio hits none of this. WASM alloc is bump-fast, no GC/syscall. Real RT concern only with GUI observer active → medium, not high.

### Q105 — Patch::tick clones the entire execution-order Vec every sample

- **Severity:** medium  |  **Status:** confirmed  |  **Dimension:** `perf-tickpath`  |  **Location:** `src/graph.rs:668`
- **Independently found by** 6 auditors
- **Remediation:** **Fixed** — Patch::tick no longer clones the execution-order Vec each sample; it uses a precompiled Routing struct (wave-c/perf, graph.rs).

**Finding.** `for &node_id in &self.execution_order.clone()` heap-allocates and frees a Vec<NodeId> on every tick to sidestep the borrow checker. At 44.1kHz that is 44,100 alloc/free pairs per second per patch, directly violating the zero-allocation guarantee (README.md:36). Under PolyPatch this multiplies by voices×unison.

**Recommendation.** Avoid the clone: iterate `for i in 0..self.execution_order.len() { let node_id = self.execution_order[i]; ... }`, or use `std::mem::take` to swap the Vec out into a reusable scratch field and swap it back after the loop.

**Verifier evidence.** graph.rs:668 literally `for &node_id in &self.execution_order.clone()` — a Vec<NodeId> heap clone every tick. execution_order is Vec<NodeId> (graph.rs:262); tick() is the real-time path exercised in benches (benches/audio_performance.rs:304+) and WASM engine, so a user hits it each sample. Fix (index loop / mem::take) is valid. However "critical / directly violates zero-alloc" is overstated: the same tick() already allocates ~2N times per sample — gather_inputs returns a fresh HashMap-backed PortValues per node and `PortValues::new()` for outputs per node (graph.rs:669-670, port.rs:329-336, StdMap=HashMap lib.rs:27). Removing only the clone leaves the guarantee broken; this graph API isn't the zero-alloc SIMD/combinator path. Confirmed but mis-scored.

### Q108 — No denormal protection in feedback DSP paths

- **Severity:** medium  |  **Status:** confirmed  |  **Dimension:** `perf-tickpath`  |  **Location:** `src/modules.rs:295`
- **Independently found by** 2 auditors
- **Remediation:** **Fixed** — Denormal flush applied at output scatter, adding denormal protection to feedback DSP paths (wave-c/perf, graph.rs).

**Finding.** grep finds no FTZ, flush-to-zero, or anti-denormal DC injection anywhere in modules.rs/graph.rs/simd.rs. Feedback state such as the SVF integrators (`self.band += f*high; self.low += f*self.band;`, modules.rs:299-301) and reverb/delay feedback lines decay toward subnormal magnitudes on silence. On x86 subnormal arithmetic stalls 10-100x, causing CPU spikes precisely when the signal goes quiet — a classic real-time hazard.

**Recommendation.** Enable FTZ/DAZ on the audio thread (set MXCSR) or inject a tiny anti-denormal offset (e.g. add 1e-20 or a ±1e-15 DC dither) into feedback accumulators, and/or flush states below ~1e-15 to zero.

**Verifier evidence.** Code confirms claim: SVF integrators `self.band += f*high; self.low += f*self.band;` (modules.rs:295-296) and reset only zeroes state (322-325). No FTZ/DAZ/MXCSR or anti-denormal injection anywhere (grep across src found only unrelated "denormalize" param scaling). safe_clip (302) applies only to outputs, not state, so state denormalizes. Numeric sim of the exact recurrence: after a kick then silence, state stays in subnormal range ~1.98M of 2M samples — a persistent, not transient, denormal load on x86 (10-100x stalls) during silence. Real hazard for an RT library. Severity trimmed to medium: penalty is material only on x86; the primary targets here (darwin/Apple-Silicon ARM, WASM) handle subnormals at near-full hardware speed, and it degrades perf under silence rather than corrupting output.

### Q114 — Real-time compliance is only printed, never asserted; CI never measures or stores timings

- **Severity:** medium  |  **Status:** confirmed  |  **Dimension:** `perf-bench`  |  **Location:** `.github/workflows/ci.yml:126`
- **Remediation:** **Fixed** — Added a release-gated real-time compliance test (tests/realtime_compliance.rs) that asserts <80% budget, plus CI criterion baseline artifacts (wave-e/benches).

**Finding.** Benches compute time_budget and eprintln it (e.g. lines 382, 710, 727) but no assertion fails when tick time exceeds budget — nothing gates on 'N× faster than real-time'. CI bench job runs only `cargo bench --no-run && cargo bench -- --test` (ci.yml:126): --test does a single non-measuring iteration, so no timing is ever produced, compared, or stored — contradicting benches/CLAUDE.md ('Full benchmark run stored for comparison'). There is no real-time regression gate anywhere; performance can silently regress past the deadline.

**Recommendation.** Add a criterion post-measurement assertion or a separate #[test] that ticks a worst-case patch/buffer and asserts elapsed < fraction*budget; in CI persist criterion baselines (e.g. bencher/critcmp artifact) on main and diff PRs.

**Verifier evidence.** benches/audio_performance.rs computes time_budget only to eprintln! it (lines 365/382, 710/727, 744/746, 841/857, 1133/1135); grep found zero timing asserts in the file (only line 502 allocator.panic, an allocation guard). ci.yml:126 runs `cargo bench --no-run && cargo bench -- --test`; criterion's --test flag runs each bench once without measuring/comparing/storing, so no real-time regression gate exists. benches/CLAUDE.md claims "Full benchmark run stored for comparison" and "Comparison against previous runs," contradicted by actual CI. Claim accurate. Severity down-scored: this is a missing CI/perf-regression gate, a latent maintainer risk no library user hits in normal usage, not a runtime defect — medium fits better than high.

### Q116 — SIMD benchmarks never enable the simd feature, and the impl is fake SIMD

- **Severity:** medium  |  **Status:** confirmed  |  **Dimension:** `perf-bench`  |  **Location:** `benches/audio_performance.rs:625`
- **Remediation:** **Fixed** — SIMD benchmarks now enable the simd feature with a scalar-vs-simd A/B flow against the real (wide-backed) SIMD impl (wave-e/benches).

**Finding.** make bench runs `cargo bench` (Makefile:72) and CI runs it with default features only; nothing passes `--features simd`. So simd_benches/bench_audio_block_operations (625) always compiles the #[cfg(not(feature="simd"))] scalar variants (simd.rs:108-139). No A/B comparison of simd vs scalar exists. Moreover the 'SIMD' impl (simd.rs:143-216) is just manual 4× unrolling of scalar f64 ops with no core::simd/std::arch intrinsics and no wasm simd128 — the plain loop autovectorizes identically, so the feature likely yields no speedup, and the benchmark could never show it.

**Recommendation.** Run the bench twice (with and without --features simd) and critcmp the two, or use real core::simd/wide; document the measured delta. Add `make bench` `--all-features` parity.

**Verifier evidence.** Makefile:71-72 `bench: cargo bench`; ci.yml:126 `cargo bench --no-run && cargo bench -- --test` — neither passes `--features simd`, and simd is non-default. So bench's add_scalar/mul_scalar (audio_performance.rs:633-655) bind to `#[cfg(not(feature="simd"))]` scalar variants (simd.rs:108-121); bench names differ by op (add/mul), not scalar-vs-SIMD, so no A/B exists. The `#[cfg(feature="simd")]` impls (simd.rs:143-216) are manual 4× unrolls; grep finds no core::simd/std::arch/wide/simd128/_mm intrinsics; SIMD_BLOCK_SIZE=4. All factual claims hold. Severity overstated: dev-facing benchmark gap, no runtime/user impact — medium.

### Q121 — set_output/read_output hardcode output port ids 0 and 1, breaking the codebase's own numbering convention

- **Severity:** medium  |  **Status:** confirmed  |  **Dimension:** `elegance-api`  |  **Location:** `src/graph.rs:746`
- **Independently found by** 2 auditors
- **Remediation:** **Fixed** — read_output/set_output use PortSpec order with a validating try_set_output, dropping the hardcoded ports 0/1 (wave-b graph/port overhaul, graph.rs).

**Finding.** `read_output` reads `PortRef{node, port:0}` as left and `port:1` as right. But the library's convention is inputs=0,1,2… and outputs=10,11… (Passthrough out=10, BitCrusher out=10, Offset out=10). StereoOutput is the sole module that renumbers outputs to 0/1 specifically to satisfy this. `set_output(node)` accepts any NodeId with no check. Point it at a Vco or any normal module and `tick()` reads nonexistent ports 0/1 → silent `(0.0, 0.0)` with no error. This magic-number contract is undocumented and unenforced.

**Recommendation.** Make `set_output` take a `PortRef` (or two, for L/R), or validate that the node exposes ports 0/1, or read the node's first two outputs by spec order instead of hardcoded ids.

**Verifier evidence.** Verified. read_output (graph.rs:746-768) hardcodes PortRef{port:0} as left, {port:1} as right, .unwrap_or(0.0)/unwrap_or(left). set_output (graph.rs:514-516) just stores the NodeId with zero validation. The numbering convention holds: StereoOutput uniquely uses output ids 0/1 (modules.rs:3109-3112), while normal modules use 10+ (Vco outputs 10,11,12,13 at modules.rs:41-44; SampleAndHold out=10; port.rs test out1=10,out2=11). So set_output(vco_id) makes tick() read absent ports 0/1 -> silent (0.0,0.0), no error. Undocumented, unenforced magic contract confirmed. Severity: all docs/examples route through StereoOutput, so users rarely hit it, and the failure is silence (easily diagnosed at dev time), not a crash/corruption -> medium, not high.

### Q129 — Gate/trigger threshold inconsistent across modules; canonical port.rs helper ignored

- **Severity:** medium  |  **Status:** confirmed  |  **Dimension:** `elegance-internals`  |  **Location:** `src/modules.rs:2627`
- **Independently found by** 2 auditors
- **Remediation:** **Fixed** — Gate/trigger consumers standardized on the shared GATE_THRESHOLD_V (2.5V) rising-edge helper across modules (wave-b/utilities + B-0 helpers).

**Finding.** port.rs:76-78 defines SignalKind::gate_threshold() → Some(2.5) for Gate/Trigger/Clock, but modules.rs never calls it. Most modules hardcode `> 2.5` (73 matches), yet KarplusStrong (2357: `trigger > 0.5`, port at 2305 is Trigger) and Euclidean (2622 `reset > 0.5`, 2627 `clock > 0.5 && last_clock <= 0.5`, ports at 2552/2562 are Trigger) use 0.5. A trigger/clock source producing a slow ramp, an attenuated <5V pulse, or CV bleed in the 0.5–2.5V band fires KarplusStrong/Euclidean early or repeatedly while Adsr(564)/StepSequencer(3052)/Clock use 2.5 and stay silent — cross-module behavioral divergence on the same signal.

**Recommendation.** Introduce one `const GATE_THRESHOLD: f64 = 2.5;` (or call `kind.gate_threshold()`) and a shared `rising_edge(cur, prev)` helper; replace all `> 0.5` gate/clock/trigger comparisons in Euclidean and KarplusStrong with it.

**Verifier evidence.** Verified: port.rs:76-78 gate_threshold()→Some(2.5) for Gate/Trigger/Clock; grep shows it's called only in port.rs tests, never in modules.rs. Euclidean uses 0.5: reset (modules.rs:2622), clock rising-edge (2627), ports at 2552/2562 are Trigger. KarplusStrong trigger>0.5 (2357), port 2305 Trigger. Contrast Adsr (564-565 use 2.5) and StepSequencer (3052-3053 use 2.5). 37 `>2.5` vs 7 `>0.5` (4 gate-related). Divergence is genuine for signals in the 0.5-2.5V band. Downgrade to medium: canonical triggers/clocks in-library emit 5.0, so normal Clock→Euclidean/StepSequencer chains behave identically; divergence only bites attenuated/ramping/CV-bleed sources — a real but niche consistency defect, not audio corruption under normal use.

### Q167 — No example produces actual audio (file or speakers) - critical for an audio library

- **Severity:** medium  |  **Status:** confirmed  |  **Dimension:** `usable-examples`  |  **Location:** `examples/quick_taste.rs:32`
- **Independently found by** 2 auditors
- **Remediation:** **Fixed** — Examples now produce audio: quick_taste/tutorial_subtractive write WAV and a new render_wav.rs flagship renders a sequenced phrase (wave-f/examples).

**Finding.** quick_taste.rs:32-35 computes peak amplitude and prints 'Generated 44100 samples / Peak amplitude: 5.00V' - verified by running it. tutorial_subtractive.rs and every other tutorial (grep across examples/*.rs) only ever print peak/RMS text stats; none call a wav writer or an audio-out crate. Cargo.toml has no hound/cpal/rodio dependency at all. So the entire cargo-run learning path (all 13 examples) never emits an audible or file-based signal - a newcomer following the docs cannot hear Quiver produce sound without leaving Rust for the separate demos/browser Node project.

**Recommendation.** Add a lightweight `hound` dev-dependency and have quick_taste (and ideally first_patch/tutorial_subtractive) write a .wav file (e.g. quick_taste.wav) in addition to printing stats, with a println! telling the user to open/play the file. This single change would fix the top onboarding gap.

**Verifier evidence.** quick_taste.rs:33-35 only computes peak via fold(max) and prints "Generated N samples"/"Peak amplitude". No example references hound/cpal/rodio/WavWriter (grep -inE across examples/*.rs returned nothing; earlier "wav" hits were substrings of "sawtooth"/"wavetable"). Cargo.toml has no audio/wav dep. No File::create for audio in any of the 13 examples. Claim is factually accurate. But "critical" is overstated: all examples compile/run correctly and demonstrate the API; the repo also ships demos/browser which produces audible sound. This is a genuine onboarding/usability gap, not a broken-functionality critical defect — medium.

### Q051 — Combinators claim compile-time signal-type safety but carry no SignalKind

- **Severity:** low  |  **Status:** confirmed  |  **Dimension:** `math-combinators`  |  **Location:** `src/combinator.rs:4`
- **Remediation:** **Fixed** — Combinator 'compile-time signal-type safety' claim softened to honest structural (arity/shape) safety in docs (wave-b/combinator, combinator.rs).

**Finding.** Docs assert "compile-time type checking" and "type-safe signal flow" (combinator.rs:4-5, 64-68). But `Module::In`/`Out` are raw Rust types — in every real case `f64`. `Chain<A,B>` only requires `B: Module<In = A::Out>` (line 232), i.e. `f64 == f64`. Since `SignalKind::Audio`, `CvBipolar`, and `VoltPerOctave` are all represented as bare `f64`, `>>>` will silently feed an Audio output into a V/Oct pitch input with no error. `SignalKind` (port.rs:24) exists only in the port/graph layer, which the combinators never touch. The "typed combinators" only prevent tuple-shape mismatches, not the semantic mixing the prose promises.

**Recommendation.** Encode `SignalKind` at the type level (e.g. `Module<In: Signal, Out: Signal>` with marker types like `Audio`, `VOct`) so chaining incompatible kinds is a compile error, or soften the docs to "structural (arity) type checking, not signal-semantic checking."

**Verifier evidence.** Verified: `Module::In`/`Out` are raw associated types (combinator.rs:104-107), documented as `f64` in every real case (line 87, 621, 709). `Chain<A,B>` only requires `B: Module<In = A::Out>` (line 232) i.e. structural equality of Rust types. `SignalKind` (port.rs:24) never appears in combinator.rs (grep: 0 hits). So `>>>` cannot distinguish Audio-f64 from V/Oct-f64. Docs do overstate: line 4 "compile-time type checking", line 68 "compile-time verification of signal flow". Claim's facts hold. But this is a docs/API-design nuance, not a runtime correctness bug — combinators do provide genuine arity/shape safety, and semantic mis-patching is a deliberate coding act, not something normal usage silently triggers. Severity high is overstated; low is appropriate.

### Q055 — `simd` feature provides no actual SIMD — just bounds-checked scalar loops

- **Severity:** low  |  **Status:** confirmed  |  **Dimension:** `math-simd-rng`  |  **Location:** `src/simd.rs:142`
- **Independently found by** 2 auditors
- **Remediation:** **Fixed** — AudioBlock add/mul and block ops now use real SIMD via the wide crate (f64x4), replacing the fake 4x-unrolled scalar loops (wave-b/simd-rng, simd.rs).

**Finding.** Cargo.toml line 44 declares `simd = []` (empty). Under `#[cfg(feature="simd")]`, add_scalar/mul_scalar/add_block/mul_block (lines 142-216) are 4x manually-unrolled scalar loops over `self.samples[base+k]` — every access is a bounds-checked `Vec` index, with a scalar remainder loop. grep for `core::arch`, `std::simd`, `portable_simd`, `_mm_*`, `f64x*`, `target_feature` returns nothing. There is no vectorization, no dependency (e.g. `wide`/`packed_simd`), no `#[target_feature]`. The feature table documents 'SIMD vectorization for block processing'; the code delivers none. Numerically identical to the non-simd path, so audio is correct, but the documented performance guarantee is unmet.

**Recommendation.** Either implement real SIMD via `core::arch` intrinsics or `wide`/`std::simd` operating on `&mut [f64]` chunks (chunks_exact(4)) with a scalar remainder, or rename/remove the feature and drop the 'SIMD vectorization' claim to avoid misrepresentation.

**Verifier evidence.** src/simd.rs:142-216: under #[cfg(feature="simd")], add_scalar/mul_scalar/add_block/mul_block are 4x manually-unrolled scalar loops over self.samples[base+k] (bounds-checked Vec index) + scalar remainder — identical result to the non-simd path (lines 108-139). Cargo.toml:44 `simd = []` is empty with comment "SIMD vectorization"; CLAUDE.md feature table claims "SIMD vectorization for block processing". grep of src/ for core::arch|std::simd|portable_simd|_mm_|f64x|target_feature|packed_simd|wide:: returns nothing. No intrinsics, no SIMD dep, no #[target_feature]. Claim is accurate. But output is numerically correct, so no functional/audio bug — purely an unmet documented performance claim. Severity high is overcalibrated; this is a doc/misrepresentation issue.

### Q065 — Unison detune spread is double the documented cents value

- **Severity:** low  |  **Status:** confirmed  |  **Dimension:** `math-polyphony`  |  **Location:** `src/polyphony.rs:399`
- **Remediation:** **Fixed** — Unison detune now spans the documented total cents (half each side of center), fixing the 2x spread (wave-d/polyphony, polyphony.rs).

**Finding.** detune_cents is documented as 'total spread across all voices' (l.357). detune_offset computes centered = normalized*2-1 in [-1,+1] (l.396) then centered * detune_cents/1200 (l.399). The extreme voices land at +/- detune_cents/1200 octaves = +/- detune_cents cents, so the total spread edge-to-edge is 2*detune_cents cents, twice the documented total. E.g. UnisonConfig::new(3,10.0) yields voices at -10c and +10c = 20c total, not 10c. The test only checks sign and symmetry (l.899-916), not magnitude, so it passes. Conversion 100c=1/12 octave is otherwise correct.

**Recommendation.** Either divide by 2 (centered * detune_cents/2400) to make detune_cents the true total spread, or change the doc to state detune_cents is the +/- deviation from center. Add a magnitude assertion to the test.

**Verifier evidence.** Reproduced from code. src/polyphony.rs:357 documents detune_cents as "total spread across all voices". detune_offset (l.395-399): normalized*2-1 gives centered in [-1,+1], then centered*detune_cents/1200 octaves. Extreme voices reach ±detune_cents/1200 oct = ±detune_cents cents, so edge-to-edge spread = 2*detune_cents. For new(3,10.0): voice0=-10c, voice2=+10c = 20c total, not 10c. Test (l.898-916) only asserts sign/symmetry (d0<0, d2>0, d0+d2≈0), never magnitude, so it passes. Real mismatch, but it's a factor-2 semantic ambiguity on a subjective musical detune parameter (±deviation vs total is a common convention); no crash or functional break. Severity high is overcalibrated → low.

### Q122 — Port access is stringly-typed and panics on typo; no ergonomic discovery path

- **Severity:** low  |  **Status:** confirmed  |  **Dimension:** `elegance-api`  |  **Location:** `src/graph.rs:239`
- **Independently found by** 2 auditors
- **Remediation:** **Fixed** — Added fallible NodeHandle::output()/input() plus port-name discovery and informative panic messages for typo'd ports (wave-b graph/port overhaul, graph.rs).

**Finding.** `NodeHandle::in_`/`out` do `spec.input_by_name(name).unwrap_or_else(|| panic!("Unknown input port"))`. A misspelled port name is a runtime panic, not a `Result` — inconsistent with `connect()` returning `Result`. Every example chains `.unwrap()` on top (first_patch.rs lines 31-41). The `in_` trailing underscore is a keyword-collision smell. There is no compile-time typed handle and no runtime listing helper on the public happy path — a user must call `handle.spec()` and iterate `PortDef`s, or read source, to learn valid names.

**Recommendation.** Add fallible `try_in`/`try_out -> Result<PortRef,PatchError>`, expose `handle.input_names()/output_names()`, and consider typed const port handles per module. Rename `in_` to `input`.

**Verifier evidence.** graph.rs:227-248: both out()/in_() call unwrap_or_else(|| panic!("Unknown output/input port: {}", name)) — panic, while connect() returns Result<_,PatchError>. Examples chain .unwrap() (first_patch.rs:31-41). in_ underscore works around the `in` keyword. grep finds no try_in/try_out/input_names/output_names in src/; PortSpec exposes only public inputs/outputs Vec<PortDef> (port.rs:301-315), so discovery needs spec()+manual iteration. All facts accurate. Severity overstated: a typo in a static port-name literal is a programmer error surfaced immediately at dev time with a clear message, not a runtime/data hazard shipped to users. This is an API-ergonomics nit on the elegance dimension, not a correctness bug — high is inflated.

## Unverified findings (medium/low + high beyond verification cap)

### Q142 — No sample playback / sampler module - synthesis-only despite 'software synth library' claim

- **Severity:** high  |  **Status:** unverified  |  **Dimension:** `complete-domain`  |  **Location:** `src/modules.rs:1`
- **Remediation:** **Implemented** — Added a SamplePlayer module (cubic interpolation, looping, eos, V/Oct pitch, start position) type_id sample_player (wave-e/new-modules, modules).

**Finding.** Across the full module inventory (Vco, Wavetable, FormantOsc, Supersaw, KarplusStrong, NoiseGenerator, Granular, ...) there is no module that plays back an in-memory or loaded audio buffer (no `Sampler`, `SamplePlayer`, `Looper`, `AudioClip`). `Granular` (line 6291) processes/re-synthesizes an incoming live signal, not stored samples. Eurorack (Rample, Morphagene) and general software synths treat sample playback as a first-class primitive alongside synthesis; Quiver has none, so any workflow needing drum hits, one-shots, or user-recorded material is unsupported.

**Recommendation.** Add a `SamplePlayer`/`Sampler` GraphModule taking a pre-loaded `Vec<f32>`/`Arc<[f32]>` buffer with trigger/gate, pitch (V/Oct), start/loop-point CV inputs, keeping playback state advance allocation-free in tick().

### Q152 — ~20 implemented modules have zero coverage in the module reference docs

- **Severity:** high  |  **Status:** unverified  |  **Dimension:** `complete-docs`  |  **Location:** `docs/src/reference:1`
- **Remediation:** **Fixed** — The ~20 undocumented modules were given module-reference coverage (wave-f/docs, docs/src/reference).

**Finding.** Cross-checking the 58 `pub struct` DSP modules in src/modules.rs against every reference/*.md heading shows these modules are not mentioned anywhere in docs/src/ at all: Arpeggiator, Bitcrusher, ChordMemory, Compressor, DelayLine, EnvelopeFollower, Euclidean, Flanger, FormantOsc, Granular, KarplusStrong, NoiseGate, ParametricEq, Phaser, PitchShifter, Reverb, ScaleQuantizer, Supersaw, Vocoder, Wavetable (verified via `grep -rl <Name> docs/src/` returning 0 hits for each). These modules are fully implemented with rustdoc port/range comments in modules.rs (e.g. Granular ports documented at src/modules.rs:6186-6195) but a user browsing the mdbook reference/ section has no way to discover their port names or parameter ranges.

**Recommendation.** Add reference entries (or a new reference/effects2.md / envelopes2.md page) for each undocumented module listing its inputs/outputs/param ranges, mirroring the format already used for Chorus/Limiter/etc.; cross-check against the CLAUDE.md 'Common Module Types' list, which already enumerates all of them correctly.

### Q157 — Six DSP modules (out of ~58) have zero unit tests in modules.rs

- **Severity:** high  |  **Status:** unverified  |  **Dimension:** `complete-tests`  |  **Location:** `src/modules.rs:1825`
- **Remediation:** **Fixed** — Added unit tests for the six previously-untested DSP modules (wave-f/tests, module test modules).

**Finding.** Grepping the test module (lines 6395-9915) for these struct names returns no hits at all: Tremolo (struct at 1825), Vibrato (1910), Distortion (2024), Supersaw (2145), KarplusStrong (2285), Euclidean (2537). None of these have a constructor call, tick(), reset(), set_sample_rate(), or type_id() exercised anywhere. ScaleQuantizer (2414) is also never instantiated in tests -- only the unrelated Scale enum (used by Quantizer) is tested via test_scale_enum_semitones/test_scale_dorian_and_mixolydian.

**Recommendation.** Add at minimum: a basic tick/output test, a reset+set_sample_rate test, and one signal-property assertion (e.g. Distortion clipping threshold, Supersaw detuned-voice spectral spread, KarplusStrong pluck decay/pitch, Euclidean pattern correctness, Vibrato pitch-modulation depth, Tremolo AM rate/depth) for each of these 6 modules.

### Q158 — Frequency-domain / filter-response claims are asserted with vacuous DC or finite-only checks

- **Severity:** high  |  **Status:** unverified  |  **Dimension:** `complete-tests`  |  **Location:** `src/modules.rs:8221`
- **Remediation:** **Fixed** — Frequency-domain/filter-response tests strengthened from vacuous DC/finite-only checks to real response assertions (wave-f/tests, dsp_stability).

**Finding.** test_svf_filter (line ~6447) sets a low cutoff and only asserts `outputs.get(10).is_some()`, never comparing to a high-cutoff run to show attenuation. test_parametric_eq_mid_cut (8201) and test_parametric_eq_high_boost (8220) feed a constant (DC, 0 Hz) input via `inputs.set(0,1.0)` held for 1000 ticks and only assert `out.is_finite()` -- a DC signal has no mid/high spectral content, so these tests cannot detect whether the EQ boost/cut math is even wired to the right band; they would pass even if the gain parameter were ignored entirely.

**Recommendation.** Drive these filters/EQ bands with actual sine tones at frequencies inside vs. outside the target band, run to steady state, and assert RMS ratio (dB) matches the configured gain/cutoff within tolerance, mirroring the good zero-crossing approach already used in test_vco_frequency.

### Q160 — No test feeds NaN or Infinity into any module despite feedback state that can be permanently poisoned

- **Severity:** high  |  **Status:** unverified  |  **Dimension:** `complete-tests`  |  **Location:** `src/modules.rs:9712`
- **Remediation:** **Fixed** — Added NaN/Inf input tests and fixed a real NaN-latching bug via sanitize_audio() at feedback inputs of Svf/DiodeLadder/DelayLine/Reverb/Chorus/Flanger/Phaser (wave-f/tests, common.rs).

**Finding.** `grep -n 'NAN\|is_nan\|INFINITY'` over src/modules.rs returns only one unrelated hit (f64::MAX use in Quantizer). The '_bounded'/'_extreme_input' tests (e.g. test_svf_extreme_input_bounded, 9712) only use large-but-finite input (20.0), never actual NaN/Inf. Modules with persistent feedback state (Svf integrators, DiodeLadderFilter stages, DelayLine/Reverb/Chorus buffers) will propagate a single NaN sample into their state forever once written, since nothing resets on NaN.

**Recommendation.** Add NaN/Infinity-injection tests for stateful/feedback modules: feed one NaN sample, then feed clean signal for N samples afterward, and assert the module recovers (or intentionally document/clamp NaN inputs at the input stage) rather than latching NaN into all future output.

### Q162 — Patch::to_def / Patch::from_def (documented save/load round-trip) has zero test coverage

- **Severity:** high  |  **Status:** unverified  |  **Dimension:** `complete-tests`  |  **Location:** `src/serialize.rs:1282`
- **Remediation:** **Fixed** — Added to_def/from_def round-trip test coverage (wave-f/tests).

**Finding.** `to_def` (serialize.rs:1223) and `from_def` (serialize.rs:1282) are the exact API shown in CLAUDE.md's 'Patch Serialization' example. Grepping the whole repo for `to_def(`/`from_def(` shows they are only called in examples/howto_serialization.rs and examples/simple_patch.rs (127 lines, zero `assert` statements) and in src/presets.rs (which tests `into_def()`, a different, simpler conversion, at presets.rs:1094). No #[test] anywhere constructs a Patch with modules+cables, serializes via to_def, reloads via from_def with a ModuleRegistry, and asserts the reloaded patch produces the same tick() output or preserves cable/param data.

**Recommendation.** Add a round-trip test: build a multi-module Patch with several cable types (including modulated cables and normalled/mult connections), call to_def, serialize to JSON and back, reload with from_def, and assert both structural equality (module count, cable count, params) and behavioral equality (matching tick() output sequence).

### Q175 — @quiver/react's workspace:* dependency is incompatible with the plain-npm monorepo and will break external installs

- **Severity:** high  |  **Status:** unverified  |  **Dimension:** `usable-ts`  |  **Location:** `packages/@quiver/react/package.json:48`
- **Remediation:** **Fixed** — @quiver/react workspace:* dependency replaced with a semver range compatible with plain-npm installs (wave-e/wasm-ts, packages/@quiver/react).

**Finding.** react/package.json:48 declares `"@quiver/types": "workspace:*"`, a pnpm/Yarn-Berry-only protocol. The repo's root package.json uses plain npm `workspaces` (no pnpm-workspace.yaml, no yarn.lock, no `packageManager` field), and publish-npm.yml publishes with a bare `npm publish` (no changesets or workspace-range rewrite step). npm does not understand the `workspace:` protocol; installing the published @quiver/react outside this monorepo would fail to resolve @quiver/types.

**Recommendation.** Either adopt pnpm/Yarn workspaces consistently with a rewrite-on-publish tool (changesets, pnpm publish), or replace `workspace:*` with an explicit semver range (e.g. `^0.1.0`) that plain npm can resolve both locally and once published.

### Q176 — Flagship browser demo bypasses @quiver/wasm's AudioWorklet path entirely, contradicting its own docs

- **Severity:** high  |  **Status:** unverified  |  **Dimension:** `usable-ts`  |  **Location:** `demos/browser/src/main.ts:3`
- **Remediation:** **Fixed** — The browser demo now runs on the package's AudioWorklet path (ScriptProcessor removed), matching @quiver/wasm's own docs (wave-e/wasm-ts, demos/browser).

**Finding.** main.ts:3 imports via a raw relative path (`'../../../packages/@quiver/wasm/quiver.js'`) reaching outside its own package boundary — demos/browser/package.json has no `@quiver/wasm` dependency at all. Audio is produced with the deprecated main-thread `ctx.createScriptProcessor(512,0,2)` (main.ts:213-214, wired at 284-285), not the AudioWorklet infrastructure the package ships (worklet.ts, audio.ts's createQuiverAudioNode). This directly contradicts demos/browser/CLAUDE.md's claim that 'the WASM module runs in the worklet thread, ensuring glitch-free audio' — the one demo that could prove the worklet path works never exercises it, and ScriptProcessorNode is UI-thread-blocking, exactly the real-time footgun this library says it avoids.

**Recommendation.** Rewrite the demo to depend on `@quiver/wasm` as a real dependency and drive audio via createQuiverAudioNode/createQuiverAudio, or relabel the demo and worklet.spec.ts's 'AudioWorklet' tests honestly as testing a ScriptProcessor fallback only.

### Q181 — tick() before/after compile() silently returns (0.0, 0.0) forever with no error

- **Severity:** high  |  **Status:** unverified  |  **Dimension:** `usable-errors`  |  **Location:** `src/graph.rs:667`
- **Remediation:** **Fixed** — tick() before/after compile no longer silently returns (0,0); lazy recompile and last_compile_error() surface the missing-compile state (wave-b graph/port overhaul, graph.rs).

**Finding.** tick() (line 667) iterates `self.execution_order.clone()`, which is empty until compile() runs, and read_output() (line 746) returns `(0.0, 0.0)` when output_node is unset or execution_order is empty — no panic, no Result, no log. Worse, every add()/connect()/disconnect() calls invalidate() (line 567-569) which clears execution_order without recompiling, so calling tick() after any post-compile edit silently degrades to permanent silence again. This is the exact 'silence with no error' failure mode the audit calls out, baked into the library's own core loop.

**Recommendation.** Track a `compiled: bool` (or make execution_order Option) and have tick() panic/return a documented Result/log a one-time diagnostic when called while uncompiled or stale, rather than quietly producing zeroes.

### Q182 — connect() validation errors omit module/port names and valid alternatives

- **Severity:** high  |  **Status:** unverified  |  **Dimension:** `usable-errors`  |  **Location:** `src/graph.rs:581`
- **Remediation:** **Fixed** — connect() validation errors now carry the offending node/port names in Display instead of bare InvalidNode/InvalidPort (wave-b graph/port overhaul, graph.rs).

**Finding.** validate_output_port/validate_input_port (lines 571-597) return bare `PatchError::InvalidNode`/`InvalidPort` with no data, and Display (lines 183-185) just prints "Invalid node"/"Invalid port". The caller gets no indication of which node, which port name was requested, or what ports actually exist on that module's PortSpec (which is available right there via `node.module.port_spec()`), forcing the developer to re-derive the mismatch by hand.

**Recommendation.** Change InvalidPort/InvalidNode to carry the offending PortRef plus the module's available port names (from PortSpec), e.g. `InvalidPort { node: NodeId, requested: PortId, available: Vec<String> }`, and print them in Display.

### Q183 — Docs claim default validation mode is Warn; code defaults to None (fully silent)

- **Severity:** high  |  **Status:** unverified  |  **Dimension:** `usable-errors`  |  **Location:** `src/graph.rs:286`
- **Remediation:** **Fixed** — ValidationMode now defaults to Warn, matching the how-to doc, instead of the silent None (wave-b graph/port overhaul, graph.rs).

**Finding.** Patch::new() sets `validation_mode: ValidationMode::None` (line 286), matching `ValidationMode::default()` (lines 19-23, `#[default] None`). But docs/src/how-to/connect-modules.md states "Default is `Warn`, which helps catch mistakes without blocking experimentation." A user who reads only the docs believes connecting an Audio output to a Gate input, or CvBipolar to CvUnipolar, will at least log a warning; in reality validate_signal_compatibility() (graph.rs:438-440) short-circuits to Ok(()) immediately whenever mode is None, so the mismatch is 100% silent.

**Recommendation.** Either change the default to ValidationMode::Warn to match the documented behavior, or fix the doc to state the true default (None) and tell users they must opt in with set_validation_mode(Warn).

### Q003 — Karplus-Strong systematic tuning error from buffer length and fractional-delay tap placement

- **Severity:** medium  |  **Status:** unverified  |  **Dimension:** `math-oscillators`  |  **Location:** `src/modules.rs:2366`
- **Remediation:** **Fixed** — KarplusStrong places fractional-delay taps so total loop delay equals the target period, correcting the systematic tuning error (Q003 comment, wave-b/oscillators).

**Finding.** Buffer is sized period_int+2 (lines 2359-2360) and read at taps read_pos (delay L−1) and read_pos2 (delay L−2) interpolated by frac=period.fract() (lines 2366-2369). Effective delay = (L−1)(1−frac)+(L−2)frac = L−1−frac = period_int+1−frac, whereas the desired loop delay is period = period_int+frac. Error = 1−2·frac samples (0 only at frac=0.5, up to ±1 sample otherwise), ignoring the ~0.5-sample loop-filter delay entirely. At period=20 (~2.2kHz) a 1-sample error is ~5% ≈ 0.8 semitone — audibly out of tune.

**Recommendation.** Compute a target delay D = period − filter_group_delay, size the buffer to floor(D), and place the fractional interpolation taps so the total loop delay equals D.

### Q004 — Karplus-Strong DC excitation never decays (loop DC gain = 1)

- **Severity:** medium  |  **Status:** unverified  |  **Dimension:** `math-oscillators`  |  **Location:** `src/modules.rs:2373`
- **Remediation:** **Fixed** — KarplusStrong removes DC from the excitation and leaks the loop (LOOP_LEAK 0.9995) so DC gain is below unity and decays (wave-b/oscillators, modules/oscillators.rs).

**Finding.** excite() adds a purely positive impulse component `impulse = if i<period/4 {1.0}` (line 2327), giving the excitation a positive DC bias at low brightness. The loop filter filtered = sample·c + last·(1−c) with c=0.5+damping·0.49 (line 2373) has DC gain c+(1−c)=1 exactly, so the DC component circulates undamped forever — a plucked note retains a constant offset/thump that never decays regardless of the damping control. Standard KS uses zero-mean excitation or a filter with DC gain <1.

**Recommendation.** Make the excitation zero-mean (bipolar impulse or DC-block the excitation), or insert a leaky/one-zero loop filter with DC gain slightly below 1.

### Q005 — Wavetable has no mipmapping: fixed harmonic count aliases at high pitch and is dull at low pitch

- **Severity:** medium  |  **Status:** unverified  |  **Dimension:** `math-oscillators`  |  **Location:** `src/modules.rs:4839`
- **Remediation:** **Fixed** — Wavetable now uses an 8-level mip pyramid (octave-per-level band-limiting, level chosen from phase increment) to stop high-pitch aliasing (wave-b/oscillators).

**Finding.** Tables are precomputed once with a fixed harmonic count (saw 16, tri/square 8; lines 4839,4826,4852) independent of playback pitch. read_table (line 4908) just linearly interpolates one table for all notes. Above a fundamental of fs/(2·16) ≈ 1378 Hz (≈ F6) the 16th saw harmonic exceeds Nyquist and folds back — high notes alias. Conversely 16 harmonics at low notes (e.g. A1) roll off at ~880 Hz, sounding dull. A proper wavetable oscillator selects a per-octave mip level with harmonics ≤ Nyquist.

**Recommendation.** Generate a mip pyramid (halving max harmonic per octave) and select the level from phase_inc, or synthesize tables on the fly limited to floor(Nyquist/frequency) harmonics.

### Q006 — Supersaw center saw bypasses PolyBLEP, reintroducing aliasing when mix<1

- **Severity:** medium  |  **Status:** unverified  |  **Dimension:** `math-oscillators`  |  **Location:** `src/modules.rs:2255`
- **Remediation:** **Fixed** — Supersaw center saw reuses the already band-limited center voice for the mix, so mix<1 no longer reintroduces aliasing (wave-b/oscillators, modules/oscillators.rs).

**Finding.** The 7 detuned saws are band-limited with PolyBLEP (line 2239-2240), but the crossfade blends toward a raw center_saw = 2·phases[3]−1 (line 2255) with no blep. output = center_saw·(1−mix)+normalized·mix, so at the default/low mix the dominant term is an aliased naive saw. The center oscillator (index 3) is already computed with blep inside the loop as `saw`; that band-limited value should be reused instead of recomputing a naive ramp.

**Recommendation.** Store the band-limited center saw from the loop (i==3) and use it in the mix blend instead of the raw 2·phases[3]−1.

### Q010 — SVF cutoff frozen above ~fs/6 (~7.3 kHz at 44.1 kHz); documented 20 kHz unreachable

- **Severity:** medium  |  **Status:** unverified  |  **Dimension:** `math-filters`  |  **Location:** `src/modules.rs:280`
- **Remediation:** **Fixed** — TPT SVF reaches the full 20 kHz cutoff range, removing the ~fs/6 frequency freeze (wave-b/filters, modules/filters.rs).

**Finding.** f = 2*sin(pi*fc/fs) then f = min(f, 0.99). f reaches 0.99 when sin(pi*fc/fs)=0.495 → fc ≈ 0.165*fs (≈7.27 kHz at 44.1 kHz). base_cutoff maps CV to 20-20000 Hz (line 274) and clamps to 20000 (278), but every requested cutoff above ~7.3 kHz collapses to the same coefficient f=0.99, so the top ~1.5 octaves of the advertised range are unreachable and the cap is sample-rate dependent (≈16 kHz at 96 kHz). This is the classic Chamberlin fc<fs/6 stability limit, not a true 12 dB/oct 20-20k filter.

**Recommendation.** Switch to a Zavalishin TPT SVF (g=tan(pi*fc/fs), zero-delay resolution) which is stable and correctly tuned up to Nyquist, or oversample the Chamberlin core; document the true usable cutoff range.

### Q011 — DiodeLadder one-pole is naive forward-Euler (state=y), not TPT — cutoff mistuned high

- **Severity:** medium  |  **Status:** unverified  |  **Dimension:** `math-filters`  |  **Location:** `src/modules.rs:449`
- **Remediation:** **Fixed** — DiodeLadder one-pole stages now use the true TPT update (s = 2y - s_old) instead of naive forward-Euler, correcting cutoff tuning (wave-b/filters, modules/filters.rs).

**Finding.** g1 = g/(1+g) with g=tan(pi*fc/fs) is the correct ZDF/TPT integrator gain, and y = s + g1*(x-s) matches the TPT output. But TPT then updates state s = 2y - s_old; the code instead stores stages[i] = y (line 449-458). That makes each stage a forward-difference one-pole with pole 1-g1 = 1/(1+g), whereas the intended bilinear pole is (1-g)/(1+g). At fc=fs/4 (g=1) intended pole=0 but actual pole=0.5, so the real -3 dB point sits far below the knob value; the error grows with frequency and compounds across 4 stages, making the filter progressively too dark/mistuned toward the top.

**Recommendation.** Use the true TPT update: v=(x-s)*g/(1+g); y=v+s; s=y+v (i.e. s=2y-s_old) for each stage, so the pole is (1-g)/(1+g).

### Q012 — DiodeLadder resonance feedback uses previous-sample output (unit-delay, non-ZDF)

- **Severity:** medium  |  **Status:** unverified  |  **Dimension:** `math-filters`  |  **Location:** `src/modules.rs:443`
- **Remediation:** **Fixed** — DiodeLadder resonance feedback resolved within the sample via a 2-iteration ZDF solve, removing the unit-delay non-ZDF path (wave-b/filters, modules/filters.rs).

**Finding.** fb is computed from self.feedback (line 443), which was written as s4/5 at the END of the prior tick (line 459); u = input_driven - fb*5 (446). Thus the global resonance path has a full one-sample delay rather than zero-delay feedback resolution. With k=res*4 this detunes resonance, shifts the self-oscillation frequency (increasingly at high fc / low fs where one sample is a larger phase), and reduces effective resonance versus a true ladder. Stability is preserved only because diode_sat (tanh) bounds fb and each stage, so it won't NaN — but the resonance tuning and self-osc pitch are wrong.

**Recommendation.** Resolve the feedback within the sample (ZDF): solve the 4-stage system for the current-sample output with the k*output term, or at minimum apply a half-sample delay correction; document it as a unit-delay approximation if kept.

### Q015 — ADSR decay/release parameter times do not equal actual segment durations

- **Severity:** medium  |  **Status:** unverified  |  **Dimension:** `math-dynamics`  |  **Location:** `src/modules.rs:578`
- **Remediation:** **Fixed** — ADSR decay/release rates scaled by the traversed span so labeled times equal actual segment durations (wave-b/dynamics, modules/dynamics.rs).

**Finding.** Segments are linear: level−=decay_rate with decay_rate=1/(decay_time·fs) (line 578,595), running from 1.0 down to sustain. Actual decay samples = (1−sustain)/decay_rate = (1−sustain)·decay_time·fs, so real decay duration = (1−sustain)·decay_time. With sustain=0.7 a ‘0.3s’ decay lasts 0.09s (3.3× off). Release runs from current level to 0: duration = level·release_time, so a ‘0.4s’ release from sustain 0.7 lasts 0.28s. The parameters denote full-scale traversal time, not the conventional peak→sustain / sustain→0 time. Attack (0→1) is correct at attack_time.

**Recommendation.** Scale rates by the traversed span: decay_rate = (1.0−sustain)/(decay_time·fs); release_rate = level_at_release_start/(release_time·fs) (capture start level on gate-fall) so labeled times match durations.

### Q016 — NoiseGate: gate open/close ramp reuses detector coefficients and lacks a hold time

- **Severity:** medium  |  **Status:** unverified  |  **Dimension:** `math-dynamics`  |  **Location:** `src/modules.rs:1313`
- **Remediation:** **Fixed** — NoiseGate gained an independent 5ms anti-click fade and a hold time, decoupled from the detector coefficients (wave-b/dynamics, modules/dynamics.rs).

**Finding.** gate_state ramps with attack_coef/release_coef (lines 1313,1315) which are the level-detector coefficients derived from attack_ms(0.1–50ms)/release_ms(10–490ms) at lines 1299-1300. Thus the click-avoidance ramp speed is welded to detector ballistics rather than an independent fade time, so a fast detector forces a fast (clicky) gate fade. There is also no hold time (CLAUDE lists NoiseGate ballistics), so signals hovering near threshold chatter despite the 0.7 hysteresis, and gate_state*=release_coef (line 1315) decays asymptotically toward denormals, never reaching 0.

**Recommendation.** Introduce a separate fade coefficient for gate_state and a hold-counter that keeps the gate open for N samples after the last supra-threshold sample; flush gate_state to 0 below ~1e-15.

### Q022 — Delay time changes are not smoothed → zipper/click on modulation (DelayLine)

- **Severity:** medium  |  **Status:** unverified  |  **Dimension:** `math-timefx`  |  **Location:** `src/modules.rs:935`
- **Remediation:** **Fixed** — DelayLine delay-time changes smoothed by a one-pole 5ms slew, removing zipper/click on modulation (wave-b/timefx, modules/timefx.rs).

**Finding.** DelayLine maps time CV to delay per-sample (935-937) and immediately reads at the new delay (940) with no slew/crossfade on the delay length. When the 'time' input is automated or a control jumps, the read pointer moves discontinuously; linear interpolation smooths sub-sample but not multi-sample jumps, producing clicks and pitch glitches. (LFO-driven Chorus/Flanger/Vibrato are fine because their delay changes continuously each sample.) This only bites when the time parameter itself is modulated, hence medium.

**Recommendation.** Slew-limit the target delay_samples toward its setpoint (e.g. one-pole smoothing at a few ms) or crossfade between old and new read taps when the delay changes abruptly.

### Q027 — Vocoder top bands collapse to f=0.99 clamp (mistuned/degenerate)

- **Severity:** medium  |  **Status:** unverified  |  **Dimension:** `math-nonlinear`  |  **Location:** `src/modules.rs:6035`
- **Remediation:** **Fixed** — Vocoder band centers capped relative to sample rate so the top bands no longer collapse to the f=0.99 clamp (wave-b/nonlinear, modules/nonlinear.rs).

**Finding.** The Chamberlin SVF coefficient f=2·sin(π·freq/sr) is clamped to 0.99 (line 6035). Solving 2·sin(π·f/sr)=0.99 at sr=44100 gives f≈7266 Hz. With VOCODER_FREQ_MAX=8000, every band center above ~7.3 kHz clamps to the same f=0.99, so the highest one or two bands are detuned downward and become identical in response — their bandpass tuning is lost. At lower sample rates the problem worsens. This distorts the analysis/synthesis filterbank at the top of the spectrum.

**Recommendation.** Lower VOCODER_FREQ_MAX relative to sample rate, or use a bilinear/TPT SVF whose coefficient stays valid up toward Nyquist instead of a hard 0.99 clamp.

### Q028 — Granular normalizes by sqrt(active_count) → amplitude zipper

- **Severity:** medium  |  **Status:** unverified  |  **Dimension:** `math-nonlinear`  |  **Location:** `src/modules.rs:6371`
- **Remediation:** **Fixed** — Granular normalized by the smoothed expected overlap instead of sqrt(active_count), removing the amplitude zipper (wave-b/nonlinear, modules/nonlinear.rs).

**Finding.** Line 6371 divides the summed grain output by sqrt(active_count). active_count changes discretely whenever a grain spawns or ends (6357,6364), so the normalization factor jumps sample-to-sample, producing audible amplitude steps/zipper. Worse, grains near phase 0 or 1 contribute ~0 via the Hann window (6346) yet still increment active_count, so the denominator over-counts silent grains and over-attenuates. Proper constant-power overlap-add should normalize by the expected steady-state overlap (density·grainsize), not the fluctuating instantaneous count.

**Recommendation.** Divide by a smoothed/constant expected-overlap factor (grains_per_sec·grain_seconds) or apply gain smoothing, rather than instantaneous sqrt(active_count).

### Q029 — Bitcrusher fractional downsample truncates to integer periods

- **Severity:** medium  |  **Status:** unverified  |  **Dimension:** `math-nonlinear`  |  **Location:** `src/modules.rs:1574`
- **Remediation:** **Fixed** — Bitcrusher fractional downsample now accumulates phase rather than truncating to integer periods (wave-b/nonlinear, modules/nonlinear.rs).

**Finding.** downsample_factor is fractional (1..64, line 1572) but the hold logic (1574-1578) increments hold_counter by 1 and resets it to 0 when counter≥factor. Because it zeroes rather than subtracting the factor, no fractional phase accumulates: the effective hold period is ceil(factor) for all values. Factors 1.1 and 1.9 both yield a period of 2; there is no way to realize, e.g., an average 1.5× reduction. The advertised continuous sample-rate reduction is quantized to integers.

**Recommendation.** Accumulate phase and subtract on wrap: `hold_counter -= downsample_factor` when it exceeds the factor, so fractional ratios average correctly over time.

### Q030 — Foldback distortion uses a variable-time while-loop in the audio path

- **Severity:** medium  |  **Status:** unverified  |  **Dimension:** `math-nonlinear`  |  **Location:** `src/modules.rs:2071`
- **Remediation:** **Fixed** — Foldback distortion replaced its data-dependent while-loop with a constant-time closed-form fold, restoring the RT guarantee (wave-b/nonlinear, modules/nonlinear.rs).

**Finding.** foldback (2067-2079) reflects with `while folded > threshold || folded < -threshold`. Iteration count scales with input magnitude: with the ±5V convention and drive→1, gained≈5·6=30 folds ~15 times; a stray large input folds proportionally more. This is a data-dependent, unbounded-iteration loop inside tick(), violating the documented 'predictable performance / avoid variable-time algorithms' real-time guarantee.

**Recommendation.** Replace with the closed-form triangle fold: `folded = threshold - abs(((gained - threshold).rem_euclid(4*threshold)) - 2*threshold)` (constant time).

### Q031 — Granular pitch range doc/impl mismatch and extreme-speed buffer overrun

- **Severity:** medium  |  **Status:** unverified  |  **Dimension:** `math-nonlinear`  |  **Location:** `src/modules.rs:6312`
- **Remediation:** **Fixed** — Granular pitch clamped to ±24 st and grain reads bounded, fixing the doc/impl mismatch and extreme-speed buffer overrun (wave-b/nonlinear, modules/nonlinear.rs).

**Finding.** Port doc (line 6192) states pitch shift '-24 to +24' semitones, but line 6313 computes semitones = pitch_cv·12 with pitch_cv clamped ±5 (6301), i.e. ±60 semitones → speed = 2^±5 = 0.031..32 (comment 6312 even says -60..+60). At speed=32 with grain size up to 0.5s·44100=22050 (6306), read_offset = phase·size·speed reaches ~705600 samples, wrapping the 96000 buffer ~7 times and reading effectively random/aliased buffer content. The documented and actual ranges disagree, and the extreme end is unusable.

**Recommendation.** Reconcile the range: clamp semitones to ±24 (map pitch_cv·4.8) to match the doc, and bound read speed so grains cannot lap the buffer.

### Q037 — Euclidean pulses control is inert unless step count changes

- **Severity:** medium  |  **Status:** unverified  |  **Dimension:** `math-utilities`  |  **Location:** `src/modules.rs:2616`
- **Remediation:** **Fixed** — Euclidean regenerates its pattern when steps OR pulses change, so the pulses control is no longer inert (wave-b/utilities, modules/sequencing.rs).

**Finding.** The pattern is regenerated only when self.pattern.len() != steps (line 2617). generate_pattern takes both steps and pulses, but if the user changes only pulses_cv while steps stays constant, len() still equals steps so no regeneration occurs — the pulses knob has no audible effect. Turning the primary density control does nothing except at the moment the step count also changes.

**Recommendation.** Cache last steps AND last pulses; regenerate when either differs. Recompute pulses = (pulses_cv*steps) each tick and compare against a stored value.

### Q038 — Clock default CV yields ~27 BPM, not the documented 120 BPM

- **Severity:** medium  |  **Status:** unverified  |  **Dimension:** `math-utilities`  |  **Location:** `src/modules.rs:3443`
- **Remediation:** **Fixed** — Clock default bpm CV set so the documented 120 BPM is produced, fixing the ~27 BPM default (wave-b/utilities, modules/sequencing.rs).

**Finding.** cv_to_bpm(cv) = 20 * 15^(cv/10). The bpm input defaults to 1.2 with the comment "120 BPM when scaled" (line 3430) and tick defaults to 1.2 (line 3461). But 20*15^(0.12) = 20*1.377 ≈ 27.5 BPM. To get 120 BPM you need cv ≈ 6.6 (since 6 = 15^(cv/10) → cv = 10*ln6/ln15 ≈ 6.62). Any patch relying on the default tempo runs 4x too slow.

**Recommendation.** Set the default bpm CV to ~6.6, or change cv_to_bpm mapping/comment so cv=1.2 (or the chosen default) actually produces 120 BPM.

### Q040 — Arpeggiator never releases held notes; reset input does not clear them

- **Severity:** medium  |  **Status:** unverified  |  **Dimension:** `math-utilities`  |  **Location:** `src/modules.rs:5484`
- **Remediation:** **Fixed** — Arpeggiator releases the note captured on the gate rising edge and clears on reset, fixing stuck/never-released notes (wave-b/utilities, modules/sequencing.rs).

**Finding.** Notes are added on gate rising edge (line 5563) but remove_note (line 5484) is never called from tick, and the reset input (line 5570) only resets current_step/direction, not held_notes. So the doc claim that notes "persist until reset" is wrong — nothing except GraphModule::reset() clears them. In normal play the chord grows monotonically on every gate until num_notes hits the 8-note cap (add_note then silently drops further notes), and old notes are never removed on gate release.

**Recommendation.** Track note-off (gate falling edge) to call remove_note, and/or clear held_notes on the reset trigger so the arpeggiator reflects the currently-held chord.

### Q044 — cubic_sat has a discontinuity at the knee (wrong threshold 2/3 instead of 1)

- **Severity:** medium  |  **Status:** unverified  |  **Dimension:** `math-analog`  |  **Location:** `src/analog.rs:88`
- **Remediation:** **Fixed** — cubic_sat knee threshold corrected from 2/3 to 1, removing the ~0.099 discontinuity (wave-b/analog, analog.rs).

**Finding.** cubic_sat returns x - x^3/3 for |x|<2/3, else sign*2/3. The classic cubic soft-clip x - x^3/3 reaches its zero-slope maximum of 2/3 at x=1, not x=2/3. At x=2/3 the polynomial equals 2/3-(8/27)/3=0.5679, but the clamp jumps to 0.6667: a discontinuous step of ~0.099 and a slope kink. This injects a click/high harmonics whenever the signal crosses +/-2/3, defeating the point of a soft clipper.

**Recommendation.** Use the branch condition |x|<1.0 (not 2/3) so the curve joins the clamp value 2/3 continuously with zero slope: if fabs(x)<1.0 { x - x*x*x/3.0 } else { sign*2.0/3.0 }.

### Q045 — V/Oct drift random walk is sample-rate dependent and effectively frozen

- **Severity:** medium  |  **Status:** unverified  |  **Dimension:** `math-analog`  |  **Location:** `src/analog.rs:369`
- **Remediation:** **Fixed** — V/Oct drift replaced with an Ornstein-Uhlenbeck process (tau=30s, ~3c std), fixing the SR-dependent, effectively-frozen random walk (wave-b/analog, analog.rs).

**Finding.** drift_state += random_bipolar()*drift_rate*dt*1000.0 is a pure random walk (no mean reversion, clamped to +/-10 cents). Increment scales with dt (not sqrt(dt)), so per-second diffusion variance = sr*Var(step) is proportional to dt = 1/sr: doubling the sample rate halves the drift speed. It is not sample-rate independent, and it is not Ornstein-Uhlenbeck. Also magnitude: step = +/-0.0001*0.0000227*1000 ≈ +/-2.3e-6 cents; reaching the +/-10 cent clamp needs ~1e13 samples (decades). The drift is inaudible/non-functional.

**Recommendation.** Scale the increment by sqrt(dt) for sample-rate-invariant diffusion, or make it an OU process (drift += (-drift/tau + noise)*dt) with a physically calibrated tau. Raise drift_rate so the effect is audible on a musical timescale.

### Q046 — Tracking error uses abs(octave_distance), producing a non-physical V-shaped error

- **Severity:** medium  |  **Status:** unverified  |  **Dimension:** `math-analog`  |  **Location:** `src/analog.rs:374`
- **Remediation:** **Fixed** — Tracking error uses signed octave distance instead of abs(), removing the non-physical V-shaped error (wave-b/analog, analog.rs).

**Finding.** octave_distance = (current_octave-center).abs(), then error_cents += octave_distance*octave_error_coef (coef always positive, 1-3 cents/oct). This makes both high and low octaves sharp by the same sign (a V with a kink at C4). Real VCO V/Oct tracking error is a scale-factor error (e.g. 1.005 V/oct) giving a monotone signed deviation: sharp at the top and flat at the bottom, not symmetric. The abs() model is not how analog tracking degrades.

**Recommendation.** Drop the abs(): use signed (current_octave-center)*octave_error_coef so error grows sharp above center and flat below, matching a V/oct scale error. Optionally add a small quadratic bow term for realism.

### Q052 — Arrow laws are asserted but never tested, and the Arrow interface is only partially implemented

- **Severity:** medium  |  **Status:** unverified  |  **Dimension:** `math-combinators`  |  **Location:** `src/combinator.rs:22`
- **Remediation:** **Fixed** — Added the missing Arrow arr primitive (arr/Arr) and Arrow-law tests, completing the interface (wave-b/combinator, combinator.rs).

**Finding.** Docs list the full Arrow interface incl. `arr: (a->b)->Arrow a b` (line 16) and claim identity/associativity/first-distribution laws hold (lines 22-28). But there is no `arr` primitive (only `Map`/`Contramap` on an existing module) and no projection/`fst`/`assoc` primitives, so the exchange laws (Hughes laws 5-7) cannot even be expressed. The test module (lines 611-922) only checks pointwise `tick` values; no property/law test exists. The laws that are structurally trivial do hold (each sub-module is ticked once per tick in fixed order, so state-associativity is fine) — but the sweeping "satisfy the Arrow laws" claim is unverified and the interface is a subset.

**Recommendation.** Add an `arr` constructor and property tests (proptest) checking `id>>>f==f`, `(f>>>g)>>>h==f>>>(g>>>h)`, and `first(f>>>g)==first f>>>first g` over random stateful modules; or scope the doc claim to the laws actually implemented and tested.

### Q056 — AudioBlock documented 'SIMD-aligned' but Vec<f64> is only 8-byte aligned

- **Severity:** medium  |  **Status:** unverified  |  **Dimension:** `math-simd-rng`  |  **Location:** `src/simd.rs:25`
- **Remediation:** **Documented as intended** — The false 'SIMD-aligned' claim was dropped; the Vec<f64> 8-byte alignment with unaligned wide loads is now honestly documented (wave-b/simd-rng, simd.rs).

**Finding.** Line 25 comments the struct as a 'SIMD-aligned audio buffer' and the module claims a 'SIMD-friendly audio buffer for vectorized operations'. The backing store is a plain `Vec<f64>` (line 31), which the global allocator aligns to `align_of::<f64>() = 8` bytes. SSE requires 16-byte and AVX 32-byte alignment for aligned loads/stores; an 8-byte-aligned base cannot guarantee those. So even if real SIMD were added, aligned loads (`_mm256_load_pd`) could fault; only unaligned loads would be safe. The alignment claim is false.

**Recommendation.** Drop the 'SIMD-aligned' wording, or back the buffer with an over-aligned allocation (e.g. `#[repr(align(32))]` wrapper or an aligned-alloc), and document the actual guarantee (currently 8 bytes).

### Q057 — No denormal flushing in ring-buffer/block feedback paths — real-time CPU spikes

- **Severity:** medium  |  **Status:** unverified  |  **Dimension:** `math-simd-rng`  |  **Location:** `src/simd.rs:530`
- **Remediation:** **Fixed** — flush_denorm applied at RingBuffer::read_interp, removing denormal CPU spikes in ring-buffer/block feedback paths (wave-b/simd-rng, simd.rs).

**Finding.** RingBuffer (delay/feedback primitive) and AudioBlock block ops perform no denormal flush-to-zero. In feedback loops (`read_interp`, lines 530-539, feeding a decaying delay) signals asymptote toward 1e-300-scale denormals, which on x86 incur ~100x slower arithmetic — a well-known audio-DSP real-time hazard directly at odds with the library's 'predictable performance / no blocking in tick()' guarantee. Nothing here adds a tiny DC/anti-denormal offset or clamps `|x|<1e-20` to 0.

**Recommendation.** Add a denormal guard in the hot paths, e.g. `if x.abs() < 1e-30 { x = 0.0 }` after interpolation/accumulation, or add a small anti-denormal offset in feedback consumers, or set FTZ/DAZ where the platform allows.

### Q058 — next_bool() uses the lowest bit of xoroshiro128+, its weakest bit

- **Severity:** medium  |  **Status:** unverified  |  **Dimension:** `math-simd-rng`  |  **Location:** `src/rng.rs:100`
- **Remediation:** **Fixed** — next_bool now draws the top bit ((next_u64()>>63)==1) instead of xoroshiro128+'s weak low bit (wave-b/simd-rng, rng.rs).

**Finding.** next_u64() (lines 72-82) is xoroshiro128+, whose low-order bits have low linear complexity — the LSB in particular is essentially an LFSR bit and fails BigCrush linear-complexity/matrix-rank tests (documented by Blackman & Vigna). `next_bool` (line 100) returns `next_u64() & 1`, extracting exactly that weakest bit, and `next_bool_with_probability` correctly uses next_f64 instead. Any consumer of next_bool (e.g. probabilistic/Bernoulli gating) gets a statistically poor bitstream.

**Recommendation.** Derive the bool from a high bit, e.g. `(self.next_u64() >> 63) & 1 == 1`, or from `self.next_f64() < 0.5`, matching the higher-quality path already used for the probability variant.

### Q066 — Constant-power pan applied unconditionally: -3 dB on default path and wrong for stereo voices

- **Severity:** medium  |  **Status:** unverified  |  **Dimension:** `math-polyphony`  |  **Location:** `src/polyphony.rs:629`
- **Remediation:** **Fixed** — Constant-power pan applied only when unison>1 with spread; the default mono path is unity-gain and stereo voices are balance-preserved (wave-d/polyphony, polyphony.rs).

**Finding.** For every unison voice, pan_angle=(pan+1)*PI/4 with left_gain=cos, right_gain=sin (l.629-631). With the default UnisonConfig (voices=1) pan_position returns 0 (l.405), giving pan_angle=PI/4 and left_gain=right_gain=0.7071, so left+=l*0.7071, right+=r*0.7071 - every voice is attenuated ~3 dB even with no unison. Worse, the law is applied to an already-stereo (l,r) pair: at pan=-1 right_gain=0 zeroes the patch's right channel entirely, collapsing/discarding stereo content rather than positioning a mono source.

**Recommendation.** Skip panning when unison.voices<=1 (or stereo_spread==0). For unison, either sum the voice to mono before applying the pan law, or use a balance law that preserves an existing stereo signal, so the neutral center case is unity gain.

### Q067 — No gain compensation across simultaneously sounding notes (polyphonic sum clips)

- **Severity:** medium  |  **Status:** unverified  |  **Dimension:** `math-polyphony`  |  **Location:** `src/polyphony.rs:633`
- **Remediation:** **Fixed** — Polyphonic sum gets smoothed 1/sqrt(N) gain compensation so simultaneous notes no longer clip (wave-d/polyphony, polyphony.rs).

**Finding.** tick() accumulates every active voice's output into left/right (l.633-634) with only unison_gain=1/sqrt(unison.voices) applied. There is no scaling by the number of active polyphony voices. N held full-scale notes sum toward N (correlated attack transients approach linear addition), so 4-8 note chords routinely exceed +/-1.0 and clip downstream. The unison 1/sqrt(N) is a defensible perceptual compromise, but nothing bounds the polyphonic sum. output() (l.645) exposes the unbounded value.

**Recommendation.** Apply a master scale (e.g. 1/sqrt(active_count) or a soft limiter) to the summed poly output, or document that callers must attenuate; at minimum clamp/limit before returning to avoid hard clipping.

### Q068 — Voice stealing does not prefer releasing voices over held notes

- **Severity:** medium  |  **Status:** unverified  |  **Dimension:** `math-polyphony`  |  **Location:** `src/polyphony.rs:309`
- **Remediation:** **Fixed** — Voice stealing prefers releasing voices and QuietestSteal works, instead of stealing held notes (wave-d/polyphony, polyphony.rs).

**Finding.** For RoundRobin/OldestSteal, find_steal_voice picks max_by_key(|v| v.age) (l.311) across all non-free voices without regard to state. A voice that is still held (Active) but old will be stolen even when a Releasing voice (already fading, inaudible-bound) exists - the standard, less-audible steal target. This cuts sustained notes the player is holding and increases click likelihood. QuietestSteal (l.313) relies on envelope_level which PolyPatch never populates, so it always returns voice 0.

**Recommendation.** Two-pass steal: first choose among Releasing voices (oldest/quietest), fall back to Active only if none are releasing. Wire real envelope levels for QuietestSteal.

### Q069 — set_sample_rate does not propagate to voice patches or inputs

- **Severity:** medium  |  **Status:** unverified  |  **Dimension:** `math-polyphony`  |  **Location:** `src/polyphony.rs:506`
- **Remediation:** **Fixed** — set_sample_rate propagates to voices (voices rebuilt from the stored builder) (wave-d/polyphony, polyphony.rs).

**Finding.** PolyPatch::set_sample_rate only writes self.sample_rate (l.507) and comments that patches 'would need to be recompiled' (l.508-509). It never calls set_sample_rate on any entry of voice_patches or voice_inputs. After a sample-rate change every oscillator/filter/envelope in the voices keeps its old rate, producing wrong pitch, cutoff and envelope timing. The test (l.1104-1108) only checks the scalar field, masking the gap.

**Recommendation.** Iterate voice_patches (and voice_inputs) calling set_sample_rate on each module (Patch should expose a set_sample_rate that forwards to nodes), then require/trigger recompile, so the change actually takes effect.

### Q076 — Graph mutation after compile() silently freezes output until recompiled

- **Severity:** medium  |  **Status:** unverified  |  **Dimension:** `correct-graph`  |  **Location:** `src/graph.rs:567`
- **Remediation:** **Fixed** — Graph mutation triggers lazy recompile with last_compile_error(); no more silent frozen output after post-compile edits (wave-b graph/port overhaul, graph.rs).

**Finding.** invalidate() only does `self.execution_order.clear()` (lines 567-569); it does not clear buffers or set any 'dirty' flag that tick() checks. After a compiled patch is mutated (add/connect/remove), execution_order is empty, so tick()'s loop body never runs and scatter_outputs is never called. read_output() then returns the stale `buffers` values from the last pre-mutation tick — output freezes at the last sample rather than erroring or reflecting the new graph. tick() before the first compile() likewise silently returns (0,0). Nothing signals that a recompile is required.

**Recommendation.** Track a `dirty` flag; have tick() debug-assert/return an explicit error or auto-recompile when dirty, and clear buffers on invalidate.

### Q077 — Feedback patches are impossible: any cycle (even through a unit delay) is rejected

- **Severity:** medium  |  **Status:** unverified  |  **Dimension:** `correct-graph`  |  **Location:** `src/graph.rs:621`
- **Remediation:** **Fixed** — Feedback patches now compile through delay cycle-breakers with one-sample semantics, instead of rejecting every cycle (wave-b graph/port overhaul, graph.rs).

**Finding.** topological_sort() builds edges purely from cable from.node->to.node (lines 626-632) and rejects any graph whose Kahn ordering is incomplete with CycleDetected (lines 654-660). There is no implicit unit-delay/feedback-break: a delay-with-feedback or FM-feedback patch — core to a 'hardware modular' system — cannot compile. Even inserting a UnitDelay module in the loop still forms a graph cycle (its input->output edge is present), so compile() still fails. The engine therefore cannot express a large class of legitimate patches the library advertises.

**Recommendation.** Support feedback by designating delay-type modules as cycle-breakers: exclude their input edges from the sort and read their output from the previous tick's buffer, or provide an explicit feedback cable type.

### Q079 — Normalled inputs read the output-buffer namespace, causing id collisions and a one-sample lag

- **Severity:** medium  |  **Status:** unverified  |  **Dimension:** `correct-graph`  |  **Location:** `src/graph.rs:716`
- **Remediation:** **Fixed** — Normalled inputs read current-tick sibling input values, fixing the output-namespace id collision and one-sample lag (wave-b graph/port overhaul, graph.rs).

**Finding.** For an unpatched input with normalled_to, gather_inputs reads `self.buffers.get(&{node, normalled_port})` (lines 716-726). But `buffers` is keyed by PortRef{node,port} and only ever stores OUTPUTS (compile:606-616, scatter_outputs:736). Since input and output port ids share the {node,port} key space, a normalled_to targeting an input id finds no entry and silently falls back to `input.default`; if an output happens to share that id it reads the previous tick's output (one-sample stale). E.g. StereoOutput normals right(in id1)->0, reading last tick's left OUTPUT and overriding the module's own current-sample mono fallback with a delayed value.

**Recommendation.** Normalled sources should read from the resolved current-tick input value of the target port, not the output buffer map; disambiguate input vs output keys.

### Q080 — Nondeterministic evaluation order from HashMap-seeded topological sort

- **Severity:** medium  |  **Status:** unverified  |  **Dimension:** `correct-graph`  |  **Location:** `src/graph.rs:635`
- **Remediation:** **Fixed** — Topological sort made deterministic, removing HashMap-seeded nondeterministic evaluation order (wave-b graph/port overhaul, graph.rs).

**Finding.** Under the default `std` feature StdMap = std::collections::HashMap (lib.rs:27). topological_sort seeds Kahn's queue via `in_degree.iter().filter(deg==0)` (lines 635-639) and iterates `successors` (a HashMap) — both in nondeterministic hash order. Independent source nodes (e.g. parallel oscillators feeding one mixer) therefore get an arbitrary, run-varying relative order. Because multi-cable inputs are summed in cable-list order this is mostly stable, but execution_order itself (exposed and used by observers) and any order-sensitive shared state are not reproducible across runs/builds.

**Recommendation.** Make the sort deterministic: seed the queue and successor lists in a stable order (sort by NodeId, or use an insertion-ordered map) so compile() is reproducible.

### Q081 — ModulatedParam.value() adds volt-scale CV to a normalized base then clamps, pinning params

- **Severity:** medium  |  **Status:** unverified  |  **Dimension:** `correct-graph`  |  **Location:** `src/port.rs:485`
- **Remediation:** **Fixed** — ModulatedParam normalizes CV by CV_FULL_SCALE_VOLTS (5V) before combining with the 0-1 base, so params are no longer pinned (wave-b graph/port overhaul, port.rs).

**Finding.** value() computes `modulated = base + cv*attenuverter` then `range.apply(modulated)` (lines 485-488). For Linear/Exponential, apply() clamps `modulated` to [0,1] (lines 434,436). `base` is documented normalized 0-1, but `cv` is 'incoming CV voltage' — CvBipolar is ±5V. So cv=5, base=0.5 -> 5.5 -> clamped to 1.0: any modest positive CV slams the parameter to its maximum, and any negative CV to minimum, giving on/off behavior instead of proportional modulation. The unit tests only exercise cv=0.2 (already normalized), hiding the mismatch.

**Recommendation.** Define and document cv units consistently (normalize CV by voltage_range, e.g. cv/5.0) before combining with base, or scale attenuverter to convert volts->normalized.

### Q087 — Output-node assignment is not serialized; from_def guesses it heuristically

- **Severity:** medium  |  **Status:** unverified  |  **Dimension:** `correct-serialize`  |  **Location:** `src/serialize.rs:1354`
- **Remediation:** **Fixed** — PatchDef gained an explicit output field so the output node is serialized instead of heuristically guessed (wave-e/serialize, serialize.rs).

**Finding.** PatchDef has no field for the output node, and to_def never records patch.output_node. from_def picks the output by looking for a module literally named "output", else the first module with a port named "left"/"right" (lines 1354-1364). A patch whose output node has a different name, or a patch containing multiple modules exposing left/right (e.g. Chorus, Reverb, RingModulator all have left/right outputs — modules.rs), restores the wrong output or none. Round-trip through to_def/from_def is therefore not faithful for output routing.

**Recommendation.** Add an explicit output field (module name) to PatchDef; set it in to_def from output_node and honor it in from_def, falling back to the heuristic only when absent.

### Q088 — Schema/implementation drift: module_type enum lists 36 of 63 registered types

- **Severity:** medium  |  **Status:** unverified  |  **Dimension:** `correct-serialize`  |  **Location:** `schemas/patch.schema.json:71`
- **Remediation:** **Fixed** — The schema module_type enum was regenerated from the registry (36->66 types) with a drift-guard test (wave-e/serialize, schemas/patch.schema.json).

**Finding.** The registry registers 63 types (serialize.rs, e.g. reverb:944, delay_line:452, compressor:519, distortion:570, chorus:462, wavetable:922, vocoder:963, granular:983, arpeggiator:1006, mixer8:422, etc.), but the schema's module_type enum (lines 72-107) contains only 36 and omits all of those. A fully valid, loadable patch using e.g. "reverb" fails JSON-schema validation against the shipped schema. The schema also documents a generic `state` object (line 120) that the implementation never emits or consumes.

**Recommendation.** Generate the enum from the registry (or drop the enum) so schema and registry cannot diverge; remove/implement the state field consistently.

### Q089 — Introspection coverage gap: ~25 stateful modules have no ModuleIntrospection impl and it is never dispatched from a live patch

- **Severity:** medium  |  **Status:** unverified  |  **Dimension:** `correct-serialize`  |  **Location:** `src/introspection_impls.rs:15`
- **Remediation:** **Fixed** — ModuleIntrospection coverage extended and dispatched from a live Patch via param_infos/get_param_by_id/set_param_by_id (wave-e/serialize, introspection_impls.rs).

**Finding.** Only the modules imported at lines 15-22 get an impl; stateful modules like DelayLine, Chorus, Flanger, Phaser, Tremolo, Vibrato, Distortion, Bitcrusher, Limiter, NoiseGate, Compressor, EnvelopeFollower, Supersaw, KarplusStrong, Euclidean, ScaleQuantizer have no ModuleIntrospection impl at all. Moreover the trait is implemented only on concrete types; the graph stores Box<dyn GraphModule> and nothing exposes ModuleIntrospection from it (grep finds no param_infos/set_param_by_id usage in graph.rs/observer/wasm). So a GUI cannot enumerate or set parameters of a loaded patch, which — combined with the dead parameters map — means there is no working end-to-end parameter round-trip.

**Recommendation.** Provide ModuleIntrospection impls for stateful modules and expose param_infos/set_param_by_id through Patch (e.g. a dyn-accessible method or a supertrait object) so GUIs and serialization can use them.

### Q094 — React hooks never free wasm-bindgen engines (leak + no audio teardown)

- **Severity:** medium  |  **Status:** unverified  |  **Dimension:** `correct-wasm`  |  **Location:** `packages/@quiver/react/src/hooks.ts:372`
- **Remediation:** **Fixed** — Engines get an explicit free/destroy lifecycle so React hooks release wasm-bindgen engines and tear down audio (wave-e/wasm-ts, packages/@quiver).

**Finding.** useQuiverEngine (hooks.ts:345-378) creates an engine via createEngine and, on unmount or sampleRate change, its cleanup only sets `mounted=false` (line 372-374) — it never calls the wasm-bindgen `engine.free()`. wasm-bindgen objects hold WASM linear memory and are not reclaimed by JS GC without explicit free or a FinalizationRegistry, so every sampleRate change and every unmount leaks an engine. No hook wires a worklet, so 'does unmount stop audio?' — there is no audio to stop, and dispose() in audio.ts (line 165) likewise never frees the worklet's engine.

**Recommendation.** Return an engine.free() call in the useEffect cleanup (guarding against use-after-free), and add a worklet 'destroy' message that calls engine.free() before the processor is torn down.

### Q095 — Structural graph mutation and compile() run on the audio thread

- **Severity:** medium  |  **Status:** unverified  |  **Dimension:** `correct-wasm`  |  **Location:** `packages/@quiver/wasm/src/worklet.ts:176`
- **Remediation:** **Fixed** — Structural graph mutation/compile made boundary-safe on the worklet thread rather than running unguarded on the audio thread (wave-e/wasm-ts, wasm/engine.rs).

**Finding.** handleMessage (worklet.ts:176-237) runs inside port.onmessage, which executes on the AudioWorklet render thread. add_module (line 192), connect (199), and especially compile() (line 223) rebuild the patch schedule and allocate, executing between render quanta. A load_patch also does load_patch+compile inline (line 182-184). Long/allocating operations on the audio thread risk deadline misses and audible glitches, contradicting the 'no blocking / predictable performance in tick()' guarantee.

**Recommendation.** Build/compile the new graph off-thread (main thread engine or a staging structure) and hand the finished, immutable graph to the audio thread via an atomic pointer swap / double-buffer, so process() never observes a half-mutated or reallocating graph.

### Q096 — MIDI API is wired to nothing and unreachable through the worklet

- **Severity:** medium  |  **Status:** unverified  |  **Dimension:** `correct-wasm`  |  **Location:** `src/wasm/engine.rs:512`
- **Remediation:** **Fixed** — MIDI is wired to five engine-owned in-patch ExternalInputs, reachable through the worklet and verified audible in a browser (wave-e/wasm-ts, wasm/engine.rs).

**Finding.** midi_note_on merely stores v_oct/velocity/gate into engine fields (engine.rs:512-516, comment: 'For now, just store them for retrieval'); nothing feeds these into any patch input, so notes never affect audio. Worse, worklet.ts's WorkletMessage union and handleMessage switch (lines 83-92, 180-230) have no midi cases at all, so the documented MIDI integration cannot even be invoked on the audio-producing engine. The feature is effectively dead.

**Recommendation.** Expose an ExternalInput/note module the MIDI setters drive, and add midi_note_on/off/cc/pitch_bend message types to the worklet protocol so they reach the worklet engine.

### Q101 — O(n^2) hand-rolled DFT executed on the audio thread

- **Severity:** medium  |  **Status:** unverified  |  **Dimension:** `correct-rtio`  |  **Location:** `src/observer.rs:793`
- **Remediation:** **Fixed** — The O(n^2) hand-rolled DFT on the audio thread was replaced, removing the audio-thread spectral cost (wave-b/rtio, observer.rs/visual.rs).

**Finding.** compute_magnitude_spectrum is a naive DFT: outer k in 0..n/2, inner i in 0..n, each iteration calling libm::cos and libm::sin (lines 813-821). For fft_size=256 that is 256*128≈32768 sin/cos pairs per full buffer, run inside collect_spectrum on the worklet thread. visual.rs SpectrumAnalyzer::compute_spectrum (lines 729-756) has the identical O(n^2) structure with std trig. This is non-predictable, variable-time work on the real-time path (the library claims "predictable performance, avoid variable-time algorithms").

**Recommendation.** Use a real radix-2 FFT (O(n log n)) with a precomputed twiddle table, or perform spectrum computation off the audio thread from a captured sample buffer.

### Q102 — MidiState multi-field updates use Relaxed with no release/acquire, allowing torn note snapshots

- **Severity:** medium  |  **Status:** unverified  |  **Dimension:** `correct-rtio`  |  **Location:** `src/io.rs:201`
- **Remediation:** **Fixed** — MidiState note-on writes pitch/gate with release/acquire ordering instead of separate Relaxed atomics, preventing torn note snapshots (wave-b/rtio, io.rs).

**Finding.** MidiState is documented for cross-thread use ("Update from a MIDI callback thread, read from the audio thread"). Note-on writes pitch then gate as separate atomics (lines 201-203), each via AtomicF64::set which uses Ordering::Relaxed (line 32). Relaxed provides no inter-variable ordering, so the audio thread reading gate=5V (new) may still observe the old pitch (or a new pitch with an old gate) for a sample, producing a wrong-pitch transient on note changes. The same applies to velocity/gate pairing. Individual scalar reads are fine, but the multi-field handoff is unsynchronized.

**Recommendation.** Publish note events atomically: pack pitch+gate into a single atomic word or a seqlock/generation counter, or store gate with Release and load with Acquire after loading pitch to establish a happens-before edge.

### Q109 — ParametricEq recomputes three biquads (pow/cos/sin/sqrt) unconditionally every sample

- **Severity:** medium  |  **Status:** unverified  |  **Dimension:** `perf-tickpath`  |  **Location:** `src/modules.rs:4682`
- **Remediation:** **Fixed** — ParametricEq caches each band's biquad coefficients and recomputes only on parameter change, ending the unconditional per-sample pow/cos/sin/sqrt (wave-b/filters).

**Finding.** tick() calls calc_low_shelf, calc_peaking, calc_high_shelf every sample (modules.rs:4682-4684). Each does pow(10,..)/cos/sin plus sqrt (calc_low/high_shelf use two sqrt, lines 4581-4582). That is ~3 pow + 3 cos + 3 sin + 6 sqrt per sample even when the CV inputs are static. The struct (modules.rs:4528-4534) caches only biquad state, no coefficients, so there is no change-detection.

**Recommendation.** Store last-seen CV/coefficients in the struct; recompute the three coefficient sets only when the relevant input changes (or at a control-rate decimation, e.g. every 16-32 samples), then run just the three process_biquad calls per sample.

### Q110 — PolyPatch::tick multiplies the graph's per-sample allocations by voices×unison

- **Severity:** medium  |  **Status:** unverified  |  **Dimension:** `perf-tickpath`  |  **Location:** `src/polyphony.rs:626`
- **Remediation:** **Fixed** — PolyPatch::tick is allocation-free: inner sub.patch.tick() uses the zero-alloc graph tick, unison gain is cached outside the loop, and UnisonConfig is an all-scalar heap-free clone (wave-c+wave-d).

**Finding.** The inner loops call `patch.tick()` once per active voice per unison voice (polyphony.rs:606-636). Each call incurs the execution_order.clone() (graph.rs:668) and 2×(modules) PortValues HashMap allocations. With 8 voices × 4 unison that is 32 full graph ticks — and 32× the per-sample heap churn — for a single output sample, compounding findings above into the worst-case real-time load.

**Recommendation.** Fix the underlying graph allocations first; additionally, cache `unison.voice_gain()` and pan trig outside the hot path and consider that re-ticking one shared patch per unison voice both allocates heavily and shares oscillator state across unison voices (also a correctness smell).

### Q117 — Benchmarks profile native x86 opt-3, but production is wasm32 opt-level="z"

- **Severity:** medium  |  **Status:** unverified  |  **Dimension:** `perf-bench`  |  **Location:** `Cargo.toml:60`
- **Remediation:** **Fixed** — Added a wasm-oriented size profile and .cargo/config.toml flags so benches reflect production wasm settings, not just native x86 opt-3 (wave-e/benches).

**Finding.** [profile.release] sets opt-level="z" + lto=true (Cargo.toml:60-62), and the shipped target is wasm32 (Makefile:146 `wasm-pack build --target web`). Criterion benches build under the bench profile on the host (x86-64, opt-level 3). Size-optimized wasm in a browser can be several× slower than native opt-3, so the whole real-time-budget analysis (tables in benches/CLAUDE.md, realtime_compliance group) is measured on a platform/opt-level the user never runs. There is no wasm-side performance measurement at all.

**Recommendation.** Add a wasm/browser micro-benchmark (e.g. performance.now() around process_block in a headless Playwright run) and/or a `[profile.bench]` matching production; report budgets against wasm numbers, not native.

### Q118 — WASM build enables no SIMD128 and optimizes for size, hurting real-time headroom

- **Severity:** medium  |  **Status:** unverified  |  **Dimension:** `perf-bench`  |  **Location:** `Makefile:146`
- **Remediation:** **Fixed** — WASM build enables simd128 and adds a [profile.wasm-size] rather than only optimizing for size, restoring real-time headroom (wave-e/benches, Cargo.toml/.cargo).

**Finding.** The wasm target builds with `--no-default-features --features wasm` and no `RUSTFLAGS=-C target-feature=+simd128` and no `simd` feature (Makefile:146); there is no .cargo/config setting it. Combined with opt-level="z" (Cargo.toml:61), the shipped audio engine has neither WebAssembly SIMD128 nor speed-favoring codegen — the worst configuration for a real-time DSP engine. The browser demo also drives audio via the deprecated main-thread ScriptProcessorNode(512) (demos/browser/src/main.ts:214) rather than the AudioWorklet, adding main-thread jank the benches never model.

**Recommendation.** For wasm: build with `-C target-feature=+simd128` and opt-level=3 (or a dedicated wasm profile), verify autovectorization; switch the demo's default path to the AudioWorklet (worklet.ts) at the 128-frame render quantum.

### Q119 — Worst-case expensive modules are never benchmarked

- **Severity:** medium  |  **Status:** unverified  |  **Dimension:** `perf-bench`  |  **Location:** `benches/audio_performance.rs:1182`
- **Remediation:** **Fixed** — Added heavy-FX worst-case and per-expensive-module benchmarks (wave-e/benches, benches/audio_performance.rs).

**Finding.** Only vco/svf/diode/adsr/lfo/noise/quantizer/slew/clock are benched (groups at 1182-1197), and create_complex_patch (92) is just 2 VCOs + mixer + diode ladder. The modules most likely to blow the real-time budget — Reverb, Granular, PitchShifter, Vocoder, DelayLine (ring-buffer/interp), Wavetable, Supersaw, KarplusStrong, FFT-based effects — have zero coverage. A 'large patch / all-modules' worst case (dense FX chain at 96kHz/32-sample buffer) is never measured, so the suite validates cheap modules and omits the ones that matter for real-time compliance.

**Recommendation.** Add a heavy-FX worst-case patch bench (Supersaw→DiodeLadder→Chorus→DelayLine→Reverb, plus a Granular/PitchShifter bench) at 96kHz and 32/64-sample buffers, and per-module benches for each O(n)/FFT effect.

### Q123 — Parameter API (params/get_param/set_param) is a trait-default no-op for nearly every module

- **Severity:** medium  |  **Status:** unverified  |  **Dimension:** `elegance-api`  |  **Location:** `src/port.rs:535`
- **Remediation:** **Documented as intended** — The trait-default param no-op is kept but the real parameter API is ModuleIntrospection (param_infos/set/get), and the trait defaults are now honestly documented (wave-b/E serialize).

**Finding.** `GraphModule::params()` defaults to `&[]`, `get_param`→`None`, `set_param`→ignore. `grep` finds exactly one `fn params` override in modules.rs (Offset, line 789). So `patch.set_param(vco.id(), 0, 0.5)` (as in graph.rs test line 1273) silently does nothing for VCO/VCF/ADSR/etc., and `get_param` returns `None`. A GUI enumerating parameters via this API sees empty lists everywhere. The surface advertises live parameter control that is essentially unimplemented across the module library.

**Recommendation.** Either implement `params()/get_param/set_param` consistently across modules, or remove the defaulted methods from the core trait and gate parameter discovery behind the `introspection` feature so the absence is explicit.

### Q124 — Two divergent, publicly-exported signal-compatibility APIs that disagree

- **Severity:** medium  |  **Status:** unverified  |  **Dimension:** `elegance-api`  |  **Location:** `src/port.rs:212`
- **Remediation:** **Fixed** — The two divergent signal-compatibility APIs were unified into a single is_compatible source of truth (wave-b graph/port overhaul, graph.rs/port.rs).

**Finding.** `port.rs::ports_compatible` returns a `Compatibility` enum; `graph.rs::SignalKind::is_compatible_with` returns a `CompatibilityResult` struct. They give different verdicts: for Audio→CvBipolar, `ports_compatible` yields `Allowed` (no warning) while `is_compatible_with` yields a warning. The graph's validation uses `is_compatible_with`; `ports_compatible` is exported in the prelude (lib.rs:76) but never called internally — it is dead but public. Two overlapping truth sources for the same question is confusing and a maintenance hazard.

**Recommendation.** Collapse to one function returning one type; have the graph call it. If both must exist, define one in terms of the other and document which is authoritative.

### Q130 — Core DSP idioms copy-pasted instead of shared helpers (V/Oct, env coef, delay read)

- **Severity:** medium  |  **Status:** unverified  |  **Dimension:** `elegance-internals`  |  **Location:** `src/modules.rs:69`
- **Remediation:** **Fixed** — Copy-pasted DSP idioms replaced by shared helpers (voct_to_hz, env_coef, read_interpolated, EdgeDetector, flush_denorm, polyblep) in the B-0 modules.rs split.

**Finding.** No shared DSP helper layer. `261.63 * pow(2.0, voct)` (V/Oct→Hz) is re-derived with the literal 261.63 at 69, 2226, 2352, 4945, 5147 (13 total). Envelope smoothing coefficient `exp(-1.0/(time*sr))` is duplicated at 1200,1299-1300,1406-1407,1497-1498,6078-6079 (9×), split across ms-vs-seconds variants inviting unit bugs. `read_interpolated` (linear wrapping delay read) is byte-identical at 1640,1946 and near-identical at 899 (4 defs total). 314 `PortDef::new` and 199 `inputs.get_or` calls show the same construction/param pattern repeated ~58×.

**Recommendation.** Add `src/dsp.rs` with `voct_to_hz(v)`, `env_coef(secs, sr)`, `linear_delay_read(buf, write_pos, delay)`, and a `#[macro_export] port_spec!` helper; delete the duplicated bodies.

### Q131 — mdk.rs is not dogfooded — internal modules never use the Module Development Kit

- **Severity:** medium  |  **Status:** unverified  |  **Dimension:** `elegance-internals`  |  **Location:** `src/mdk.rs:668`
- **Remediation:** **Fixed** — Added an mdk dogfooding test so the Module Development Kit is exercised by internal modules (wave-f/tests, tests/mdk).

**Finding.** `grep 'mdk\|ModuleTestHarness\|AudioAnalysis' src/modules.rs` → 0 hits. mdk.rs ships ModuleTestHarness::run_all (test_reset, test_stability, test_nan_inf, test_output_range, test_zero_input) and AudioAnalysis (rms, peak, dc_offset, estimate_frequency). modules.rs has 172 `#[test]`s that instead reimplement zero-crossing counting inline (6419, 8490) and never run the standard harness against its 58 modules. This means the kit's own contract is unverified by the library it ships with, and each module misses free NaN/inf/stability/range coverage.

**Recommendation.** Add a parameterized test that runs `ModuleTestHarness::new(m, sr).run_all()` over every module (via the registry/enum), and use AudioAnalysis in oscillator/filter tests instead of hand-rolled analysis.

### Q132 — Fundamental domain values are unnamed magic numbers

- **Severity:** medium  |  **Status:** unverified  |  **Dimension:** `elegance-internals`  |  **Location:** `src/modules.rs:68`
- **Remediation:** **Fixed** — Magic numbers named as constants (C4_HZ, GATE_THRESHOLD_V/GATE_HIGH_V, etc.) during the B-0 modules.rs split refactor (modules/common.rs).

**Finding.** While per-module tables have constants, the cross-cutting domain values have none: gate high 5.0 and threshold 2.5 (scattered ~73×), C4 = 261.63 Hz (13×), keytrack/FM `pow(2.0, x)` octave math. The compressor/limiter dB math at 1417-1420 (`20*log10`, `pow(10, -x/20)`) is also unnamed and duplicated in feel across dynamics modules. A reader cannot grep a single source of truth for 'what voltage is a gate high', and changing the tuning reference requires editing 13 sites.

**Recommendation.** Define module-level `const C4_HZ`, `const GATE_HIGH_V`, `const GATE_THRESHOLD_V` and `db_to_gain`/`gain_to_db` helpers; reference them everywhere.

### Q138 — Missing rust-version field despite CI enforcing MSRV 1.78

- **Severity:** medium  |  **Status:** unverified  |  **Dimension:** `elegance-features`  |  **Location:** `Cargo.toml:4`
- **Remediation:** **Fixed** — Added rust-version = 1.78 to Cargo.toml, matching the CI-enforced MSRV (wave-f/hygiene, Cargo.toml).

**Finding.** Cargo.toml has no `rust-version` key (only `edition = "2021"` at line 4; grep for rust-version/msrv returns nothing). CI's msrv job (.github/workflows/ci.yml:104-114) pins `dtolnay/rust-toolchain@1.78.0` and only runs on pushes to main, so a PR that raises the effective MSRV (e.g. via a dependency bump or a newer language feature) merges without warning and only breaks after merge. Also, without this field, crates.io/docs.rs cannot display or enforce the MSRV for downstream consumers.

**Recommendation.** Add `rust-version = "1.78"` to [package] in Cargo.toml so `cargo` itself enforces/reports the MSRV and it shows on crates.io/docs.rs.

### Q139 — No [package.metadata.docs.rs] section; wasm-gated public API invisible on docs.rs

- **Severity:** medium  |  **Status:** unverified  |  **Dimension:** `elegance-features`  |  **Location:** `Cargo.toml:37`
- **Remediation:** **Fixed** — Added [package.metadata.docs.rs] (all-features + docsrs cfg badges) so the wasm-gated API is visible on docs.rs (wave-f/hygiene, Cargo.toml).

**Finding.** No `[package.metadata.docs.rs]` section exists (grep confirms), and no `docsrs` cfg string appears anywhere in src/ (grep confirms zero matches). docs.rs builds with default features only (`std`, which implies `alloc` but not `wasm`), so the wasm bindings re-exported in the prelude (`QuiverEngine`, `QuiverError`, gated `#[cfg(feature = "wasm")]` at src/lib.rs:190-192, plus the whole `pub mod wasm` at src/lib.rs:63-64) never render on the published docs, and no gated item shows a 'this is supported on feature=...' badge since `--cfg docsrs` is never passed.

**Recommendation.** Add `[package.metadata.docs.rs]\nall-features = true\nrustdoc-args = ["--cfg", "docsrs"]` to Cargo.toml, and annotate feature-gated public items with `#[cfg_attr(docsrs, doc(cfg(feature = "...")))]`.

### Q143 — Nonlinear stages have no oversampling/anti-aliasing path

- **Severity:** medium  |  **Status:** unverified  |  **Dimension:** `complete-domain`  |  **Location:** `src/modules.rs:2114`
- **Remediation:** **Implemented** — Added an Oversampler (polyphase sinc 31/63-tap ~-74dB) with opt-in 2x/4x set_oversample on Distortion/Wavefolder for anti-aliasing (wave-e/new-modules, modules).

**Finding.** `Distortion::tick` (modules.rs:2100-2130) dispatches to `hard_clip`, `foldback`, and `asymmetric` (line ~2114-2117) directly at the host sample rate with no oversample/downsample step; `grep -n 'oversample\|upsample'` across src/*.rs returns nothing. Hard clipping and foldback are strongly nonlinear and generate harmonics well above Nyquist at high drive/high pitch, which will fold back as audible aliasing - a well-known problem these algorithms specifically need 2x-8x oversampling to avoid. `Wavefolder::fold` (src/analog.rs:68) and `Bitcrusher` (modules.rs:1561) have the same lack of any anti-aliasing measure.

**Recommendation.** Add a shared oversampling helper (polyphase or simple linear 2x/4x upsample + lowpass + decimate) and apply it inside Distortion/Wavefolder/Bitcrusher's nonlinear stages, at minimum as an opt-in mode.

### Q144 — Time-modulation effects are inconsistently mono vs. stereo

- **Severity:** medium  |  **Status:** unverified  |  **Dimension:** `complete-domain`  |  **Location:** `src/modules.rs:1698`
- **Remediation:** **Fixed** — Flanger and Phaser made stereo with a decorrelated L/R spread port appended (mono out port kept bit-compatible), fixing the mono/stereo inconsistency (wave-e/stereo-timefx).

**Finding.** Chorus (modules.rs:975-1047) and Reverb (5812+) implement true dual-channel processing with decorrelated left/right delay lines (`STEREO_SPREAD` offset, comb_buffers_l/_r) and expose `left`/`right` output ports (Chorus port 11/12). Flanger's port_spec (verified via its GraphModule impl at line 1656) exposes only a single output port 10 `out` and processes one buffer/LFO; Phaser (1758) is likewise single-buffer/single-output. Users must instantiate two mono Flanger/Phaser copies per channel, but since each keeps its own LFO phase independently, there's no shared-phase / stereo-spread control, unlike hardware/plugin equivalents.

**Recommendation.** Give Flanger and Phaser the same stereo treatment as Chorus/Reverb (dual delay/allpass chains with a stereo-spread parameter and left/right outputs), or clearly document them as mono-only utility blocks.

### Q145 — No WAV/audio export or offline rendering capability anywhere in the crate or demo

- **Severity:** medium  |  **Status:** unverified  |  **Dimension:** `complete-domain`  |  **Location:** `src/extended_io.rs:1`
- **Remediation:** **Implemented** — Added offline render/render_to_wav/write_wav (hand-rolled RIFF, std-gated) so patches can be rendered to WAV (wave-e/new-modules, render.rs).

**Finding.** Grepping the whole workspace (`src/*.rs`, `Cargo.toml`, `demos/browser/src`, `packages/@quiver`) for `wav`, `hound`, `MediaRecorder`, `write_wav` returns no hits related to file/audio export; extended_io.rs provides OSC and WebAudio glue but no bounce-to-disk. For a library marketed as a 'software synth library' with example patches producing audio, there is no supported way to render a patch to a `.wav` file for offline use, only real-time `tick()` consumption.

**Recommendation.** Add a lightweight WAV writer (or an optional `hound` dependency behind the `std`/`alloc` feature) with a `render_to_wav(patch, duration)` helper, and/or wire MediaRecorder into the browser demo for user-facing export.

### Q146 — No microtuning/Scala support - Scale enum is a fixed 12-TET preset list

- **Severity:** medium  |  **Status:** unverified  |  **Dimension:** `complete-domain`  |  **Location:** `src/modules.rs:3300`
- **Remediation:** **Implemented** — Added microtuning: ScaleQuantizer set_custom_scale/load_scala plus a src/scala.rs .scl parser, beyond the fixed 12-TET presets (wave-e/new-modules).

**Finding.** `Scale` (modules.rs:3300-3322) is a closed enum: Chromatic/Major/Minor/PentatonicMajor/PentatonicMinor/Dorian/Mixolydian/Blues, each mapped to a fixed 12-semitone-degree slice, and `ScaleQuantizer`/`Quantizer` only ever snap V/Oct to these built-in tables. There is no cents-based tuning table, no `.scl`/`.kbn` (Scala) import, and no way to define a custom microtonal scale - the V/Oct spec itself (port.rs) hardcodes `2^n` equal-tempered octave scaling with no alternate temperament hook.

**Recommendation.** Add a `Scale::Custom(Vec<f64>)` variant (cents or ratios) and a small Scala-file (.scl) parser under the alloc feature so quantizers/oscillators can use arbitrary tunings.

### Q147 — ModulatedParam smoothing abstraction is defined and exported but used by zero modules

- **Severity:** medium  |  **Status:** unverified  |  **Dimension:** `complete-domain`  |  **Location:** `src/port.rs:450`
- **Remediation:** **Fixed** — ModulatedParam adopted as the parameter-read path in new modules (SamplePlayer pitch/start, Ducker amount/threshold), so the abstraction is no longer dead (wave-e/new-modules).

**Finding.** `ModulatedParam` (port.rs:450-486) combines a base knob value, CV, attenuverter and range mapping, and is re-exported from the prelude (lib.rs:76), suggesting it's meant to be the standard way modules read parameters. Grepping `src/modules.rs` for `ModulatedParam` returns zero hits - every module reads raw `inputs.get_or(...)` each tick with no smoothing. Only `SlewLimiter` (modules.rs:3251) offers smoothing, and only if a user manually patches it onto a CV connection; there is no crate-level mechanism to avoid zipper noise on knob/CV steps for any of the ~55 modules.

**Recommendation.** Either wire ModulatedParam (with an internal slew stage) into representative modules (cutoff, gain, mix knobs) as the canonical parameter-read path, or remove the dead abstraction and document that smoothing is the user's responsibility via SlewLimiter.

### Q153 — module-catalog.md type_id examples don't match real type_id() strings

- **Severity:** medium  |  **Status:** unverified  |  **Dimension:** `complete-docs`  |  **Location:** `docs/src/how-to/module-catalog.md:14`
- **Remediation:** **Fixed** — module-catalog.md type_id examples corrected to the real type_id() strings (wave-f/docs, docs/src).

**Finding.** docs/src/how-to/module-catalog.md:14 shows `type_id: "Vco"`, line 37-39 lists `Vco, Lfo, NoiseGenerator`, `AdsrEnvelope, SlewLimiter`, `SvfFilter, DiodeLadderFilter`, and line 77 does `catalog.find(m => m.type_id === 'SvfFilter')`. The actual `GraphModule::type_id()` implementations return lowercase snake_case strings: `"vco"` (src/modules.rs:107), `"svf"` (src/modules.rs:332), `"adsr"` (src/modules.rs:635), `"lfo"` (src/modules.rs:201) — there is no `SvfFilter` or `AdsrEnvelope` type_id anywhere in the codebase. A developer copying this doc's filter/search examples verbatim will get no matches.

**Recommendation.** Update the examples to use the real type_id strings ("vco", "svf", "adsr", "lfo", etc.) by grepping `fn type_id` return values in src/modules.rs, and regenerate the TypeScript catalog example against the actual @quiver/wasm catalog() output.

### Q154 — README links to a non-existent DEVELOPMENT.md

- **Severity:** medium  |  **Status:** unverified  |  **Dimension:** `complete-docs`  |  **Location:** `README.md:157`
- **Remediation:** **Fixed** — The missing DEVELOPMENT.md that README linked to was created (wave-f/docs, DEVELOPMENT.md).

**Finding.** README.md:157 `| 🗺️ [DEVELOPMENT.md](./DEVELOPMENT.md) | Architecture decisions and roadmap |` and README.md:209 `See [DEVELOPMENT.md](./DEVELOPMENT.md) for the development roadmap...` both link to `./DEVELOPMENT.md`, but `ls DEVELOPMENT.md` at repo root returns 'No such file or directory' — the file does not exist anywhere in the repo (verified via find).

**Recommendation.** Either create DEVELOPMENT.md with the promised architecture/roadmap content or remove both README references and point instead to docs/src/concepts/architecture.md, which does exist and covers the same ground.

### Q155 — CHANGELOG is a stale auto-gen placeholder, not current history

- **Severity:** medium  |  **Status:** unverified  |  **Dimension:** `complete-docs`  |  **Location:** `.github/CHANGELOG.md:10`
- **Remediation:** **Fixed** — The stale placeholder CHANGELOG was replaced with current history (wave-f/docs, CHANGELOG).

**Finding.** .github/CHANGELOG.md:10-19 'Unreleased' section only lists 'Developer experience setup' and 'Changelog generation script', with 'Fixed: None'/'Changed: None', and line 24 literally reads '<!-- Generated on: Run `make changelog` to update -->' — the placeholder was never re-run. `git log -1 --format=%cd -- .github/CHANGELOG.md` shows Dec 21 2025, while the repo's recent commit log (e.g. 96f6f3b, 9d86900, f2f6a20, 5a79576, ce3f9aa) shows substantial unrelated feature/docs work landed after that with no changelog entries. There is also no root-level CHANGELOG.md despite `make changelog` being documented as a top-level command in CLAUDE.md.

**Recommendation.** Run `make changelog` (git-cliff) before releases/PRs to regenerate .github/CHANGELOG.md, or add a CI check that fails if CHANGELOG.md is stale relative to HEAD.

### Q159 — No test exercises set_sample_rate() mid-stream (after audio has flowed) on any module

- **Severity:** medium  |  **Status:** unverified  |  **Dimension:** `complete-tests`  |  **Location:** `src/modules.rs:7419`
- **Remediation:** **Fixed** — Added tests exercising set_sample_rate() mid-stream after audio has flowed (wave-f/tests, tests/sample_rate_change.rs).

**Finding.** All ~28 `*_default_reset_sample_rate` tests (e.g. test_vco_default_reset_sample_rate, line 7419) call `set_sample_rate()` immediately after construction, before any tick(), then tick, then reset(). None call set_sample_rate() after already accumulating state (e.g. a DelayLine with echoes buffered, an SVF mid-resonant-decay). DelayLine::set_sample_rate (src/modules.rs:958-963) reallocates `self.buffer` and resets write_pos to 0, silently discarding any in-flight delayed audio with no click/discontinuity or allocation-in-tick test in place.

**Recommendation.** Add tests that tick with real signal for N samples, call set_sample_rate with a different rate, then continue ticking and assert output stays finite/bounded and buffer-dependent modules (DelayLine, Chorus, Reverb, PitchShifter) do not panic on index math using the old vs. new buffer size.

### Q161 — Quantizer/ScaleQuantizer have no test with negative V/Oct (notes below C4)

- **Severity:** medium  |  **Status:** unverified  |  **Dimension:** `complete-tests`  |  **Location:** `src/modules.rs:3354`
- **Remediation:** **Fixed** — Added Quantizer/ScaleQuantizer tests with negative V/Oct (notes below C4) (wave-f/tests).

**Finding.** test_quantizer_chromatic (7009) and test_quantizer_major_scale (7034) only use inputs 0.0-0.07V (C4 to slightly above). Quantizer::quantize (src/modules.rs:3354-3384) relies on `floor(total_semitones/12.0)` for octave and a scale-wrap search that also checks `semi+12`; the floor-based negative-octave path and the wrap-to-next-octave branch for negative `within_octave` values are entirely unexercised, despite modular synths routinely sending negative V/Oct for bass notes.

**Recommendation.** Add quantizer tests with negative CV (e.g. -0.5V, -1.0V, -13/12V) verifying correct octave and scale-degree snapping, including boundary values right at an octave crossing (e.g. -1/24V).

### Q163 — Polyphony has no stress test beyond 2-4 voices; no full-voice-count contention test

- **Severity:** medium  |  **Status:** unverified  |  **Dimension:** `complete-tests`  |  **Location:** `src/polyphony.rs:857`
- **Remediation:** **Fixed** — Added a full-voice-count polyphony contention/stress test beyond 2-4 voices (wave-f/tests, poly stress).

**Finding.** All VoiceAllocator tests (test_voice_stealing 857, test_no_steal_mode 873, test_poly_patch_basic 953, test_poly_patch_panic 967) use VoiceAllocator::new(2) or PolyPatch::new(4, ...) with 2-4 voices and only a handful of note_on/note_off calls. There is no test with a realistic voice count (8/16/32), rapid overlapping note_on/note_off churn causing repeated voice stealing, or a sustained multi-thousand-sample tick() loop verifying mixed polyphonic audio stays bounded/correct under full load.

**Recommendation.** Add a stress test with e.g. 16 voices, interleaved note_on/note_off/retrigger across a 10k+ sample tick loop under OldestSteal and NoSteal modes, asserting no panics, correct active_count bookkeeping, and bounded mixed output.

### Q164 — src/wasm/engine.rs (618 lines, QuiverEngine) has zero native Rust #[test] functions

- **Severity:** medium  |  **Status:** unverified  |  **Dimension:** `complete-tests`  |  **Location:** `src/wasm/engine.rs:15`
- **Remediation:** **Fixed** — Added native Rust #[test] coverage for src/wasm/engine.rs (QuiverEngine) (wave-f/tests).

**Finding.** `grep -rn '#\[test\]' src/wasm/*.rs` returns 0 matches across error.rs, mod.rs, and engine.rs (618 lines). Commit ce3f9aa ('Add comprehensive tests for edge cases, error handling, and subscriptions in QuiverEngine') added only TypeScript Playwright specs under demos/browser/tests/ that exercise QuiverEngine indirectly through the compiled WASM binary in a browser -- there is no `cargo test --features wasm` coverage of the Rust-side bindings (parameter marshaling, error mapping, tsify type generation) that can run in normal CI without a browser.

**Recommendation.** Add native #[test] functions in src/wasm/engine.rs (gated appropriately) that construct QuiverEngine, call its methods directly, and assert on Result/error variants and state, so bugs in the Rust glue are caught before ever reaching Playwright.

### Q166 — Long-run stability tests are rare and short; most DSP tests run only 100-1000 samples (~2-23ms)

- **Severity:** medium  |  **Status:** unverified  |  **Dimension:** `complete-tests`  |  **Location:** `src/modules.rs:9073`
- **Remediation:** **Fixed** — Added long-run DSP stability tests, replacing the short 100-1000-sample runs (wave-f/tests, tests/dsp_stability.rs).

**Finding.** Counting `for _ in 0..N` loop bounds in modules.rs: 28 tests use only 100 iterations, 25 use 1000, and just 3 (lines 6651, 8314, 8860) use 10000. test_reverb_stereo_output (9073) -- the module most at risk for feedback-driven denormal/DC drift -- only runs 3000 samples of silence after an impulse (~68ms at 44.1kHz), far short of the 'stability over 10k+ samples' bar for a feedback comb/allpass reverb network with 8 comb + 4 allpass filters per channel.

**Recommendation.** Extend feedback-heavy modules' (Reverb, Chorus, DiodeLadderFilter, Flanger, Phaser) decay/stability tests to run 50k-100k samples (roughly 1-2 seconds) and assert monotonic decay to near-zero with no re-growth, catching denormal-driven CPU spikes or slow instability that short runs miss.

### Q169 — 'Getting Started' tier has two near-duplicate examples

- **Severity:** medium  |  **Status:** unverified  |  **Dimension:** `usable-examples`  |  **Location:** `examples/simple_patch.rs:11`
- **Remediation:** **Fixed** — The two near-duplicate starters were differentiated: simple_patch is the minimal patch, first_patch the full voice (wave-f/examples).

**Finding.** first_patch.rs:12-26 and simple_patch.rs:11-28 both build the identical VCO->VCF->VCA chain with Adsr and ExternalInput gate/pitch, same module names, same structure, verified by reading both files side by side. Of the 3 'Getting Started' examples (quick_taste, first_patch, simple_patch), 2 teach the exact same concept with almost no differentiation, burning a newcomer's limited exploration time without adding a new idea (e.g. no MIDI, no polyphony, no different envelope shape).

**Recommendation.** Merge into one canonical first_patch example, or clearly differentiate simple_patch (e.g. make it the bare-minimum 4-module version without ADSR/gate, contrasted with first_patch's fuller voice).

### Q170 — Tutorials explain WHAT the code does but rarely WHY (no DSP rationale)

- **Severity:** medium  |  **Status:** unverified  |  **Dimension:** `usable-examples`  |  **Location:** `examples/tutorial_fm.rs:15`
- **Remediation:** **Fixed** — Every tutorial gained DSP-rationale comments explaining why, not just what the code does (wave-f/examples).

**Finding.** tutorial_fm.rs:15-22 comments say 'Carrier oscillator - this is what we hear' / 'modulates the carrier's frequency' / 'Modulation index control (depth of FM effect)' but never explain the FM math (e.g. that sidebands appear at fc ± n*fm, or how mod_depth in volts maps to modulation index). Grep across all tutorial_*.rs for explanatory markers ('// Why', '// Note:', '// This is because') returned zero hits, confirming comments are structural, not conceptual.

**Recommendation.** Add a short doc-comment block per tutorial explaining the underlying synthesis concept (e.g. FM sideband formula, ADSR stage time constants, filter cutoff/resonance relationship) so the example teaches theory, not just API calls.

### Q177 — QuiverEngine is only re-exported as a type, forcing a duplicated interface in @quiver/react

- **Severity:** medium  |  **Status:** unverified  |  **Dimension:** `usable-ts`  |  **Location:** `packages/@quiver/wasm/src/index.ts:10`
- **Remediation:** **Fixed** — QuiverEngine's real interface is exported from the package so @quiver/react no longer duplicates it (wave-e/wasm-ts, packages/@quiver).

**Finding.** index.ts:10 does `export type { QuiverEngine, QuiverError } from '../quiver';` — a type-only export, so `@quiver/wasm` has no runtime value consumers can `new QuiverEngine(...)` from directly (only via the broken createEngine/createAudioContext helpers, finding #1). Because the real type isn't safely importable/aligned, @quiver/react/src/hooks.ts:20-85 hand-declares its own parallel `QuiverEngine` interface duck-typing the Rust API, which will silently drift from the actual wasm-bindgen surface as the Rust API evolves.

**Recommendation.** Export both the value and type from @quiver/wasm's built entry, and have @quiver/react import that type instead of re-declaring it.

### Q178 — @quiver/react hooks lack a 'use client' directive

- **Severity:** medium  |  **Status:** unverified  |  **Dimension:** `usable-ts`  |  **Location:** `packages/@quiver/react/src/hooks.ts:1`
- **Remediation:** **Fixed** — @quiver/react hooks gained the 'use client' directive (wave-e/wasm-ts, packages/@quiver/react).

**Finding.** hooks.ts and index.ts (both checked, lines 1-3) contain no `'use client'` pragma despite exporting useState/useEffect/useCallback-based hooks (useQuiverEngine, useQuiverUpdates, etc.). Any Next.js App Router consumer importing these hooks into a file that isn't already a client boundary gets a build/runtime error requiring them to add the pragma themselves — a hooks package this shape conventionally ships the directive so it 'just works' in RSC frameworks.

**Recommendation.** Add `'use client';` as the first line of src/index.ts (or each hook file) before the tsup build.

### Q184 — Primary 'connect modules' doc shows APIs that don't exist in the crate

- **Severity:** medium  |  **Status:** unverified  |  **Dimension:** `usable-errors`  |  **Location:** `docs/src/how-to/connect-modules.md:128`
- **Remediation:** **Fixed** — The 'connect modules' doc was rewritten to the actual crate API (connect/connect_attenuated/connect_modulated, disconnect_ports) (wave-f/docs, docs/src).

**Finding.** The doc's 'Error Handling' section shows `Err(PatchError::PortNotFound(port))` and `Err(PatchError::CycleDetected)` (line ~128-136), but the real enum (src/graph.rs:164-178) has `InvalidPort` (unit variant, no port data) and `CycleDetected { nodes: Vec<NodeId> }` (struct variant). It also documents `patch.disconnect_port(...)` and `patch.cables_to(...)` (lines 122,148), neither of which exists — the real method is `disconnect_ports(from, to)` (graph.rs:804) taking two PortRefs, and there is no cables_to. Earlier examples use `connect_with`/`Cable::new().with_attenuation()` (lines 40-46, 58-64) which also don't exist (real API: connect_attenuated/connect_modulated). All blocks are fenced ```rust,ignore```, so mdbook never compiles them and this drift is invisible in CI.

**Recommendation.** Rewrite the doc examples against the actual API (connect/connect_attenuated/connect_modulated, disconnect_ports, PatchError::InvalidPort/CycleDetected{nodes}), and switch fenced blocks from `ignore` to compiled/tested doctests where feasible so drift is caught by `make test-doc`.

### Q185 — CycleDetected error never surfaces which nodes/modules are in the cycle

- **Severity:** medium  |  **Status:** unverified  |  **Dimension:** `usable-errors`  |  **Location:** `src/graph.rs:186`
- **Remediation:** **Fixed** — CycleDetected error now reports the nodes in the cycle via Display (wave-b graph/port overhaul, graph.rs).

**Finding.** topological_sort() correctly collects the stuck nodes into `PatchError::CycleDetected { nodes: Vec<NodeId> }` (lines 654-660), but Display (lines 186-188) only prints "Cycle detected involving {} nodes" — the count, not the NodeIds or module names, even though the field is public and available at format time. A developer debugging an accidental feedback loop across a dozen-module patch has to manually iterate `nodes` and call `patch.get_name()` themselves; nothing in the message itself is actionable.

**Recommendation.** Store the list of module names (or a Display bound to the Patch) at error-construction time so `{}` prints the actual node names involved, e.g. "Cycle detected: vco -> vcf -> lfo -> vco".

### Q192 — npm packages under packages/@quiver are unpublished; crates.io status is unclear from a fresh audit

- **Severity:** medium  |  **Status:** unverified  |  **Dimension:** `position`  |  **Location:** `packages/@quiver/wasm/package.json:2`
- **Remediation:** **Documented as intended** — Packages made publish-ready with a fixed workflow/lockfile, DEVELOPMENT.md added and README status clarified; actual npm/crates.io publish is deliberately gated on owner credentials (wave-e/wave-f).

**Finding.** packages/@quiver/wasm/package.json and packages/@quiver/react/package.json both declare version 0.1.0 with `prepublishOnly` build scripts, but `curl https://registry.npmjs.org/@quiver/wasm` returns HTTP 404 ({"error":"Not found"}), confirming the package has never been published to npm. A direct GET to crates.io/crates/quiver from this environment returned 403 (WAF-blocked, inconclusive) rather than a clean 200/404, so crates.io status could not be independently confirmed here — but combined with the missing DEVELOPMENT.md referenced by README.md line 157 (file does not exist in the repo), the overall packaging/release hygiene looks incomplete for a library asking users to add it as a dependency.

**Recommendation.** Publish @quiver/wasm and @quiver/react to npm (or remove/mark them WIP in docs), verify and document crates.io publication status explicitly in README, and add the missing DEVELOPMENT.md or remove the dead link.

### Q193 — Project has no external contributor community, undermining any 'open ecosystem' positioning

- **Severity:** medium  |  **Status:** unverified  |  **Dimension:** `position`  |  **Location:** `README.md:211`
- **Remediation:** **Documented as intended** — README now transparently states this is a solo (human+AI-assisted) project rather than implying an active contributor community (wave-f/docs, README.md).

**Finding.** README.md lines 211-233 invite contributions and list 'good first issue' areas, but `git shortlog -sn` on this checkout shows only 2 authors across 123 commits (48 by Alex Nodeland, 75 by 'Claude'), and the GitHub contributors API for alexnodeland/quiver lists only the owner and an automated 'claude' account. There is no evidence of any external human contributor, reviewer, or issue discussion driving the project.

**Recommendation.** Be transparent in README/positioning that this is presently a solo (human+AI-assisted) project rather than implying an active contributor base; this materially affects a prospective adopter's risk assessment (bus factor, long-term maintenance).

### Q194 — No positioning against the closest real competitor (fundsp) or any named alternative anywhere in the docs

- **Severity:** medium  |  **Status:** unverified  |  **Dimension:** `position`  |  **Location:** `docs/src/introduction.md:29`
- **Remediation:** **Fixed** — Docs added an honest comparison/positioning against fundsp (and named alternatives), filling the competitor-positioning gap (wave-f/docs, README/docs).

**Finding.** docs/src/introduction.md's 'Why Quiver?' section (lines 29-50) and README.md's 'Why Quiver?' (lines 23-44) both argue against a generic strawman ('low-level vs high-level convenience') but never mention fundsp, dasp, glicol, kira/oddio, Faust, SuperCollider, or VCV Rack anywhere in the repo (grep across all .md files returns zero hits). fundsp already offers typed Arrow-style Rust audio combinators and is published to crates.io with an established user base, making the comparison directly relevant and its absence conspicuous to anyone evaluating alternatives.

**Recommendation.** Add an honest comparison table/section (README or docs/concepts) contrasting Quiver's typed-voltage/graph model against fundsp's combinator-only approach and VCV Rack's plugin SDK, naming the concrete differentiator (hardware-semantic ports + runtime-patchable graph) rather than only an abstract category-theory framing.

### Q195 — packages/@quiver/wasm/dist/ is an untracked, stale build artifact not covered by .gitignore or package.json

- **Severity:** medium  |  **Status:** unverified  |  **Dimension:** `hygiene`  |  **Location:** `.gitignore:6`
- **Remediation:** **Fixed** — packages/@quiver/wasm/dist/ removed and covered by .gitignore (packages/@quiver/*/dist/), so the stale build artifact is no longer at risk of commit (wave-e/wasm-ts).

**Finding.** git status shows packages/@quiver/wasm/dist/ as untracked. .gitignore (lines 6-9) only ignores root-level wasm-pack outputs (quiver.js, quiver.d.ts, quiver_bg.wasm, quiver_bg.wasm.d.ts), not the dist/ subdirectory. dist/ contains files (index.js, audio.js, worklet chunks, quiver-FQN7IICD.mjs) dated Jan 2026 from an entirely different bundler than the one the Makefile invokes ('make wasm' -> wasm-pack --target web, which writes quiver.js/.d.ts/quiver_bg.wasm to the package root per package.json's own 'main'/'files' fields). dist/ is neither the documented build output nor tracked source, so it is dead weight risking accidental commit of stale/wrong artifacts.

**Recommendation.** Add 'packages/@quiver/wasm/dist/' to .gitignore, and delete the stale directory locally (or confirm it's produced by some other, undocumented tool and update package.json 'main'/'files'/Makefile accordingly if dist/ is actually the intended distribution layout).

### Q196 — Doc-tests are 100% ignored, contradicting CLAUDE.md's 'doc tests are part of the test suite' claim

- **Severity:** medium  |  **Status:** unverified  |  **Dimension:** `hygiene`  |  **Location:** `src/presets.rs:10`
- **Remediation:** **Fixed** — Doc-tests are no longer 100% ignored; ignore blocks removed so doctests compile/run (0 ignored), matching CLAUDE.md's claim (wave-f/docs, lib.rs).

**Finding.** cargo test --all-features output: 'Doc-tests quiver ... test result: ok. 0 passed; 0 failed; 8 ignored'. All 8 doctest code blocks (src/combinator.rs lines 47 and 83, src/extended_io.rs line 946, src/presets.rs lines 10/133/171/184/201) are marked with the `ignore` fence attribute, so `make test-doc` / `cargo test --doc` never actually compiles or runs any example code. CLAUDE.md states 'Doc tests are part of the test suite' as if they provide coverage, but none execute, so any of these examples could silently rot without CI catching it.

**Recommendation.** Either fix the ignored doctests so they compile and run (using `no_run` instead of `ignore` where execution isn't safe, or `ignore` only with an inline comment explaining why), or update CLAUDE.md to state doc tests are currently disabled/non-functional.

### Q007 — Vco exponential FM scales a ±5V input as ±5 octaves with no linear/through-zero FM

- **Severity:** low  |  **Status:** unverified  |  **Dimension:** `math-oscillators`  |  **Location:** `src/modules.rs:70`
- **Remediation:** **Fixed** — Vco gained a linear/through-zero FM input (fm_lin) alongside the exponential path, fixing the ±5-octave-only FM scaling (wave-b/oscillators, modules/oscillators.rs).

**Finding.** freq = base·2^fm (line 70) with fm being the raw CvBipolar input (−5..+5V). A full-scale FM signal therefore multiplies frequency by 2^±5 = ×32/÷32, five octaves — an enormous, purely exponential swing. There is no linear (through-zero) FM path, so audio-rate FM produces inharmonic, asymmetric sidebands rather than the classic linear-FM spectrum, and combined with the naive (non-band-limited) waveforms the sidebands alias heavily. Exponential FM is a legitimate design choice but the ±5-octave depth with no index/attenuation baked in is unusual and easy to misuse.

**Recommendation.** Scale the FM input (e.g. 1V/oct with an explicit index) and add an optional linear-FM input that adds to the phase increment for through-zero FM synthesis.

### Q008 — C4 reference constant 261.63 slightly off from documented 261.6256 Hz

- **Severity:** low  |  **Status:** unverified  |  **Dimension:** `math-oscillators`  |  **Location:** `src/modules.rs:69`
- **Remediation:** **Fixed** — All oscillators anchored to shared precise C4_HZ constant (modules/common.rs); final stray 261.63 literal in AnalogVco's live pitch path replaced in wave-g/fix-dsp with anchor regression test.

**Finding.** All oscillators hard-code base = 261.63 (lines 69, 2226, 2352, 4945, 5147) whereas the port/system spec documents C4 = 261.6256 Hz. The error is (261.63−261.6256)/261.6256 ≈ 1.7e-5, about 0.029 cents — inaudible in isolation but a fixed global tuning offset shared by every oscillator, and it contradicts the stated 0V reference.

**Recommendation.** Define a single `const C4_HZ: f64 = 261.6255653...` and reference it everywhere for exactness and consistency.

### Q017 — Dynamics detectors leave denormal tails at silence (no flush)

- **Severity:** low  |  **Status:** unverified  |  **Dimension:** `math-dynamics`  |  **Location:** `src/modules.rs:1207`
- **Remediation:** **Fixed** — flush_denorm applied to the detector one-poles in Limiter/Compressor/etc., removing denormal tails at silence (wave-b/dynamics, modules/dynamics.rs).

**Finding.** In Limiter (1207), Compressor (1413), NoiseGate (1306), EnvelopeFollower (1504) the release update env = coef·env + (1−coef)·abs decays toward abs_input; when abs_input=0 the envelope decays exponentially toward zero and lingers in the denormal range (coef≈0.9998). Sustained denormals can cause large per-sample CPU cost on some x86 targets, contradicting the ‘predictable performance / real-time’ guarantee. No flush-to-zero or DAZ handling is present.

**Recommendation.** After each update, if envelope (and gate_state) < ~1e-20 set it to 0.0, or add a tiny DC offset / enable FTZ, to bound the denormal tail.

### Q018 — ADSR envelope segments are linear (not exponential) and retrigger does not restart from zero

- **Severity:** low  |  **Status:** unverified  |  **Dimension:** `math-dynamics`  |  **Location:** `src/modules.rs:588`
- **Remediation:** **Fixed** — ADSR gained an exponential curve mode via a new shape input port and retrigger that restarts from zero (wave-b/dynamics, modules/dynamics.rs).

**Finding.** Attack/decay/release use fixed per-sample increments (level+=attack_rate etc., lines 588,595,605), yielding straight-line segments rather than the one-pole exponential (level += (target−level)·(1−exp(−1/(t·fs)))) of analog ADSRs — audibly different, especially percussive tails. Separately, retrigger (line 570) sets stage=Attack without resetting level, so a retrigger during sustain ramps upward from the sustain level rather than restarting from 0; acceptable but inconsistent with a ‘retrigger’ that many expect to restart the contour.

**Recommendation.** Offer an exponential mode using coef=1−exp(−1/(t·fs)) toward per-stage targets; document that retrigger continues from the current level (or reset level to 0 on retrig for a classic AD restart).

### Q019 — VCA is attenuation-only and linear; cannot amplify

- **Severity:** low  |  **Status:** unverified  |  **Dimension:** `math-dynamics`  |  **Location:** `src/modules.rs:675`
- **Remediation:** **Fixed** — Vca gained a response port (id2) plus gain (id3) so it can amplify and apply square-law response, not just linear attenuation (wave-b/dynamics, modules/dynamics.rs).

**Finding.** gain = cv.clamp(0,10)/10 ∈ [0,1] (line 675), so out = in·gain can only reduce level; an ‘amplifier’ with maximum unity gain is a misnomer and prevents CV-boost use. Response is strictly linear with no exponential/VCA-curve option (CLAUDE asks for linear vs exponential response), and negative CV is clamped to 0 rather than offering through-zero/inverting behavior.

**Recommendation.** Add a gain scale (>1) or exponential-response mode (e.g. gain = pow(cv/10, k) or dB-linear), and consider a through-zero option for ring-mod-style use.

### Q023 — Vibrato writes before reading, giving a one-sample-shorter delay than the other delays

- **Severity:** low  |  **Status:** unverified  |  **Dimension:** `math-timefx`  |  **Location:** `src/modules.rs:1988`
- **Remediation:** **Fixed** — Vibrato reads before writing the delay, restoring the exact delay length matching the other delay modules (wave-b/timefx, modules/timefx.rs).

**Finding.** Vibrato writes input to buffer then advances write_pos (1988-1989) BEFORE calling read_interpolated (1996), whereas DelayLine/Flanger/Chorus read then write. After the increment, delay_int=1 makes read_pos1 = write_pos-1 = the slot just written (current input), so the effective minimum delay is 0 samples rather than 1, and every tap is one sample shorter than the nominal delay_ms. Harmless for vibrato's 1-19 ms range but an inconsistency that would matter if this ordering were copied to a feedback delay.

**Recommendation.** Reorder to read_interpolated first, then write and advance write_pos, matching DelayLine/Flanger for a consistent, exact delay.

### Q024 — Reverb stereo-spread offset is a fixed sample count, not scaled with sample rate

- **Severity:** low  |  **Status:** unverified  |  **Dimension:** `math-timefx`  |  **Location:** `src/modules.rs:5655`
- **Remediation:** **Fixed** — Reverb stereo-spread offset now scales with sample rate (Libm round) instead of a fixed sample count (wave-b/timefx, modules/timefx.rs).

**Finding.** Comb/allpass lengths are scaled by ratio = sample_rate/44100 in update_tunings (5748-5757), but STEREO_SPREAD=23 (5655) is added as a raw sample count (5864, 5893). At 96 kHz the intended ~0.5 ms decorrelation offset shrinks to ~0.24 ms, so the stereo image narrows at higher sample rates. Cosmetic, not a correctness/stability issue (Freeverb topology is otherwise faithful and stable).

**Recommendation.** Scale STEREO_SPREAD by the same sample-rate ratio when computing right-channel lengths, e.g. (STEREO_SPREAD as f64 * ratio) as usize.

### Q032 — Bitcrusher quantizer truncates (floor) → DC bias and full-scale extra level

- **Severity:** low  |  **Status:** unverified  |  **Dimension:** `math-nonlinear`  |  **Location:** `src/modules.rs:1582`
- **Remediation:** **Fixed** — Bitcrusher quantizer uses mid-tread rounding over an integer code count, removing DC bias and the full-scale extra level (wave-b/nonlinear, modules/nonlinear.rs).

**Finding.** Line 1582 uses floor(normalized·levels)/levels — a truncating (mid-rise-toward-zero) quantizer. Truncation biases every sample downward by ~0.5 LSB, injecting a constant DC offset that grows as bits decrease. Also, at full-scale input (+5V → normalized=1.0), floor(levels)=levels yields quantized=1.0, one step beyond the intended 0..(levels-1)/levels range, so the top code is asymmetric. A mid-tread rounding quantizer would be unbiased.

**Recommendation.** Use round() (mid-tread) or floor(x+0.5)/levels, and clamp normalized to just under 1.0, to remove DC bias and the extra top level.

### Q033 — PitchShifter high pitch-up crosses write pointer; no oversampling

- **Severity:** low  |  **Status:** unverified  |  **Dimension:** `math-nonlinear`  |  **Location:** `src/modules.rs:5291`
- **Remediation:** **Fixed** — PitchShifter grain read margin bounded so the read pointer never crosses the write pointer on high pitch-up (wave-b/nonlinear, modules/nonlinear.rs).

**Finding.** rate = 2^(semitones/12) reaches 4 at +24 semitones (5291). grain_pos advances by rate each sample (5308); over one window of window_samples it moves up to 4·window while write advances only window, so the read pointer overtakes the write pointer within a grain, reading not-yet-written/stale samples → glitches at large pitch-up. Additionally, none of these shapers/pitch processors oversample, so all generate aliasing, and this limitation is undocumented. (The 50%-overlap Hann COLA itself is correct — unity gain.)

**Recommendation.** Document the aliasing/latency limitations; optionally bound pitch-up or increase buffer margin so a grain cannot lap the write pointer.

### Q041 — Comparator/quantizers lack true hysteresis, allowing boundary chatter

- **Severity:** low  |  **Status:** unverified  |  **Dimension:** `math-utilities`  |  **Location:** `src/modules.rs:3964`
- **Remediation:** **Fixed** — Comparator/quantizers gained true stateful hysteresis (hold last state), eliminating boundary chatter (wave-b/utilities, modules/logic.rs).

**Finding.** Comparator's comment says "hysteresis" but implements a static ±0.01V deadband (gt: a>b+0.01, lt: a<b-0.01) with no state — a signal dithering around b still toggles gt/eq/lt every sample. Similarly Quantizer (3354) and ScaleQuantizer (2493) round to nearest with no hysteresis, so a CV sitting exactly between two scale notes chatters between them, emitting a continuous stream of spurious change-triggers (ScaleQuantizer trigger at line 2514). Real hardware quantizers add hysteresis to prevent this.

**Recommendation.** Add per-module state: only change the quantized note / comparator output when the input crosses the boundary by more than a hysteresis band beyond the last committed value.

### Q042 — Euclidean accent uses pre-rotation step counter, can accent silent steps

- **Severity:** low  |  **Status:** unverified  |  **Dimension:** `math-utilities`  |  **Location:** `src/modules.rs:2641`
- **Remediation:** **Fixed** — Euclidean accent gated on the first actual pulse of each rotated pattern so it can no longer accent silent steps (wave-b/utilities, modules/sequencing.rs).

**Finding.** The accent output fires when self.step == 0, but pattern lookup uses rotated_step = (self.step + rotation) % steps (line 2636). With nonzero rotation, counter 0 maps to a rotated pattern slot that may be a rest, so the accent (line 2641) can fire on a step where out=0 (no pulse), and the true downbeat of the rotated pattern is never accented. Rotation is also limited to 0..steps-1 via (rotation_cv*(steps-1)) truncation, excluding a full-wrap.

**Recommendation.** Gate the accent on the actual pulse (only when self.pattern[rotated_step] is true) and reference the rotated index for downbeat detection.

### Q047 — tanh_sat origin gain exceeds unity, boosting level through the Saturator

- **Severity:** low  |  **Status:** unverified  |  **Dimension:** `math-analog`  |  **Location:** `src/analog.rs:19`
- **Remediation:** **Fixed** — tanh_sat origin gain compensated to unity, so the Saturator no longer boosts small-signal level (wave-b/analog, analog.rs).

**Finding.** tanh_sat(x,drive)=tanh(x*drive)/tanh(drive). Derivative at x=0 is drive/tanh(drive) which is >1 for all drive>0 (1.31 at drive=1, 2.07 at drive=2). So a small signal is amplified rather than passed at unity. Saturator (line 665) does tanh_sat(in/5,drive)*5 with default drive=1, applying a ~31% small-signal gain boost, not transparent low-level behavior expected of a warmth/saturation stage.

**Recommendation.** Normalize to unit slope at origin: divide by drive instead of (or in addition to) tanh(drive), e.g. tanh(x*drive)/(drive) is still not unity; simplest is tanh(x*drive)/drive*... Use f(x)=tanh(x*drive)/tanh(drive) only for hard normalization but pre-scale input by tanh(drive)/drive to restore unity origin gain.

### Q048 — Thermal model time constants are uncalibrated; drift never settles or becomes audible

- **Severity:** low  |  **Status:** unverified  |  **Dimension:** `math-analog`  |  **Location:** `src/analog.rs:201`
- **Remediation:** **Fixed** — Thermal model recalibrated to a documented phenomenological form (tau=40s, 1c/degC) so drift settles and becomes audible (wave-b/analog, analog.rs).

**Finding.** update() uses forward Euler with real dt: temp += (energy*heat_rate - (temp-ambient)*cool_rate)*dt. Default cool_rate=0.001 gives tau=1/cool_rate=1000 s (~16 min) to equilibrium, so 'thermal drift' effectively never moves on a musical timescale. Equilibrium offset = energy*(heat_rate/cool_rate)=energy*10 degC, and detuning is offset*0.001 (line 551), all magic constants with no stated physical basis. The test (line 781) 'passes' only because heating produces a ~1e-4 offset that trivially stays under 0.01.

**Recommendation.** Document that this is a phenomenological model, and pick heat/cool rates giving a plausible audible thermal-drift time constant (seconds to minutes) with a stated detuning sensitivity, so the effect is both realistic and testable.

### Q049 — asym_sat on saw is described as 'slight' but compresses amplitude ~24%

- **Severity:** low  |  **Status:** unverified  |  **Dimension:** `math-analog`  |  **Location:** `src/analog.rs:578`
- **Remediation:** **Fixed** — AnalogVco saw asymmetry made gentle and gain-compensated, ending the ~24% amplitude compression (wave-b/analog, analog.rs).

**Finding.** The saw (range -1..1) is passed through asym_sat(saw+dc,1.0,0.98)=tanh(x) for x>=0. tanh(1)=0.762, so the +/-1 saw peaks are squashed to +/-0.76 before the *5 scaling (line 599), i.e. output peaks at ~+/-3.8V instead of +/-5V, and the linear ramp is noticeably curved. The comment calls this 'slight' asymmetric saturation; the level loss and waveform bending are substantial and the near-symmetric drives (1.0 vs 0.98) add almost no even-harmonic asymmetry.

**Recommendation.** Use a much gentler shaper (e.g. drive ~0.1-0.2) and a larger pos/neg drive asymmetry to get subtle even harmonics without a 24% level drop, or compensate the output gain. Align the comment with the actual effect.

### Q053 — `>>>`, `***`, `&&&` are described as operators but no operator overloads exist

- **Severity:** low  |  **Status:** unverified  |  **Dimension:** `math-combinators`  |  **Location:** `src/combinator.rs:26`
- **Remediation:** **Documented as intended** — >>> *** &&& are kept as conceptual/notational documentation with no Rust operator overloads, deliberately, per the combinator docs (wave-b/combinator).

**Finding.** lib.rs, CLAUDE.md, and combinator.rs repeatedly present `>>>` (chain), `***` (parallel), `&&&` (fanout) as usable operators. Verified there are zero `impl Shr/BitXor/BitAnd` (or any `core::ops`) in combinator.rs — and `>>>` is not even a Rust operator token. The actual API is the methods `.then`, `.parallel`, `.fanout`. Users copying the operator notation from the docs get compile errors. The notation is borrowed from Haskell/Control.Arrow purely as illustration but reads as real API.

**Recommendation.** Either implement `Shr`/`BitAnd` overloads (`a >> b` for chain, etc.) to match the marketed notation, or state explicitly that `>>>`/`***`/`&&&` are conceptual names realized as the `.then`/`.parallel`/`.fanout` methods.

### Q054 — Feedback first-tick value and combine-argument order are undocumented

- **Severity:** low  |  **Status:** unverified  |  **Dimension:** `math-combinators`  |  **Location:** `src/combinator.rs:330`
- **Remediation:** **Fixed** — Feedback's contract documented: combine(external_input, previous_output) with the first-tick value defined (wave-b/combinator, combinator.rs).

**Finding.** `Feedback::tick` computes `combine(input, self.delay_buffer.clone())` then stores output into `delay_buffer` (lines 330-334). This is a correct causal unit-delay, and reset/set_sample_rate both propagate (lines 337-344) — genuinely well done. But the doc comments (lines 191, 314) never state (a) that on the first tick the feedback path sees `M::Out::default()` (0.0 for f64), nor (b) the argument order of `combine` (external input first, delayed output second). Users writing an asymmetric combine function must read the source to know which argument is the feedback signal.

**Recommendation.** Document that `combine`'s second argument is the previous tick's output (default/zero on the first tick) and the first is the external input; add a doctest showing a one-pole feedback to lock the contract.

### Q059 — RNG documented as 'Xorshift128+' but is actually xoroshiro128+

- **Severity:** low  |  **Status:** unverified  |  **Dimension:** `math-simd-rng`  |  **Location:** `src/rng.rs:4`
- **Remediation:** **Fixed** — RNG docs corrected from 'Xorshift128+' to the actual 'xoroshiro128+' (wave-b/simd-rng, rng.rs).

**Finding.** Module doc (line 4) and struct doc (lines 25-27) say 'Xorshift128+'. The implementation (lines 77-79) is `s1^=s0; s0 = rotl(s0,24) ^ s1 ^ (s1<<16); s1 = rotl(s1,37); result = s0+s1` — the exact xoroshiro128+ update with Blackman/Vigna constants (24,16,37). The jump constants (line 113) are also the xoroshiro128+ 2^64-jump polynomial. These are different algorithms (xorshift128+ has no rotations). The code is a correct xoroshiro128+; only the label is wrong, which misleads readers about statistical properties and provenance.

**Recommendation.** Rename references to 'xoroshiro128+' in the module and struct docs and any type/comment naming.

### Q060 — RingBuffer uses modulo, not power-of-two masking, in the per-sample audio path

- **Severity:** low  |  **Status:** unverified  |  **Dimension:** `math-simd-rng`  |  **Location:** `src/simd.rs:516`
- **Remediation:** **Fixed** — RingBuffer storage rounded up to a power of two and wrapped with a bitmask instead of modulo in the per-sample path (wave-b/simd-rng, simd.rs).

**Finding.** write() (line 516) does `self.write_pos = (self.write_pos + 1) % self.size` and read() (line 525) `(self.write_pos + self.size - delay - 1) % self.size`. `size` is an arbitrary `capacity`, so these are integer divisions executed every sample in delay lines — the slowest integer op, in the real-time path. The module header advertises SIMD/performance focus; a mask on a power-of-two-rounded capacity would eliminate the division. Correctness is fine (indices verified: read(0) = write_pos-1 = most recent, matching test line 661).

**Recommendation.** Round capacity up to a power of two and replace `% size` with `& (size-1)`, or use branchless wrap (`if pos>=size { pos-=size }`) to avoid the division per sample.

### Q061 — RingBuffer::new(0).write() panics (OOB index and modulo-by-zero)

- **Severity:** low  |  **Status:** unverified  |  **Dimension:** `math-simd-rng`  |  **Location:** `src/simd.rs:513`
- **Remediation:** **Fixed** — RingBuffer internal storage clamped to at least one slot, so new(0).write() no longer panics (wave-b/simd-rng, simd.rs).

**Finding.** RingBuffer::new(0) (line 494) builds an empty buffer with size 0; is_empty() returns true and is tested (line 856). But write() (lines 513-517) unconditionally does `self.buffer[self.write_pos]` → index-out-of-bounds panic on the empty Vec, and `(self.write_pos + 1) % self.size` → divide-by-zero panic. No guard exists. A zero-capacity buffer is nonsensical but constructible via the public API, so this is a reachable panic rather than a compile-time impossibility.

**Recommendation.** Either assert `capacity > 0` in `new`, clamp to at least 1, or early-return in write/read when `size == 0`.

### Q062 — Tests omit simd/non-simd equivalence and RNG known-answer vectors

- **Severity:** low  |  **Status:** unverified  |  **Dimension:** `math-simd-rng`  |  **Location:** `src/rng.rs:239`
- **Remediation:** **Fixed** — Added xoroshiro128+ known-answer tests and a simd/scalar equivalence test (wave-b/simd-rng, rng.rs/simd.rs).

**Finding.** rng tests (lines 238-353) assert determinism, range, and mean but never check next_u64 against published xoroshiro128+ reference vectors, so a constant/rotation typo would pass silently. simd tests (lines 594-914) exercise whichever add/mul variant the active feature set compiles, but nothing asserts the `#[cfg(feature="simd")]` unrolled path yields the same results as the scalar path (they can only be compared across two builds). Range test at line 263 also can't detect that next_f64 never quite reaches meaningful coverage, only membership.

**Recommendation.** Add a known-answer test seeding a fixed state and comparing next_u64 to reference xoroshiro128+ output, and a test (or property test) asserting scalar and simd block ops agree on random-length, non-multiple-of-4 blocks.

### Q070 — No per-voice DSP reset on allocation or steal (state leakage / steal clicks)

- **Severity:** low  |  **Status:** unverified  |  **Dimension:** `math-polyphony`  |  **Location:** `src/polyphony.rs:250`
- **Remediation:** **Fixed** — Voices reset DSP state on allocation/steal, removing state leakage and steal clicks (wave-d/polyphony, polyphony.rs).

**Finding.** On both fresh allocation (l.243) and steal (l.250) only the Voice metadata is set via note_on (l.90-98); the corresponding voice_patch is never reset. Voice::note_on sets trigger=1.0 for one sample (l.96), but the patch's filter/delay/reverb state and oscillator phase from the previous note persist. For a stolen voice mid-tail this concatenates the old signal into the new note (click / audible bleed); envelope retrigger depends entirely on downstream modules honoring the one-sample trigger. There is no 'clean retrigger' of DSP state.

**Recommendation.** Optionally reset (or fast-fade) the stolen voice's patch state on steal, or add a short declick ramp; document whether oscillator phase is intentionally preserved.

### Q071 — Retrigger path skips LRU update

- **Severity:** low  |  **Status:** unverified  |  **Dimension:** `math-polyphony`  |  **Location:** `src/polyphony.rs:236`
- **Remediation:** **Fixed** — The retrigger path now updates the LRU order (wave-d/polyphony, polyphony.rs).

**Finding.** When a note already sounding is retriggered, note_on returns early (l.234-238) without calling update_lru, unlike the free-voice (l.244) and steal (l.251) paths. The retriggered voice keeps its stale position near the front of lru_queue, so it is treated as least-recently-used. It cannot be mis-selected by find_free_voice (which filters is_free, l.302), but the LRU ordering no longer reflects actual recency, which can skew subsequent round-robin free-voice choices.

**Recommendation.** Call self.update_lru(voice.index) on the retrigger branch as well for consistent recency tracking.

### Q072 — AllocationMode doc comments for Highest/LowestPriority are mislabeled

- **Severity:** low  |  **Status:** unverified  |  **Dimension:** `math-polyphony`  |  **Location:** `src/polyphony.rs:33`
- **Remediation:** **Fixed** — AllocationMode Highest/LowestPriority doc comments corrected to match behavior (wave-d/polyphony, polyphony.rs).

**Finding.** HighestPriority's doc says 'Lowest priority - higher notes steal lower notes' (l.33) and LowestPriority says 'Highest priority - lower notes steal higher notes' (l.35) - the leading phrases are swapped relative to the variant names. The implementations (l.324-339) match the names (HighestPriority steals the lowest note when the new note is higher), so the code is right but the comments are contradictory and will mislead users choosing a mode. Also note: if the new note is not higher (Highest) / lower (Lowest) than any existing note, the filter is empty and the note is silently dropped.

**Recommendation.** Fix the doc comments to match the variant semantics, and document the drop-on-no-eligible-victim behavior.

### Q082 — ParamRange::Exponential produces NaN when min>0 and max<=0

- **Severity:** low  |  **Status:** unverified  |  **Dimension:** `correct-graph`  |  **Location:** `src/port.rs:435`
- **Remediation:** **Fixed** — ParamRange::Exponential guards against NaN when min>0 and max<=0 (wave-b graph/port overhaul, port.rs).

**Finding.** Exponential::apply only special-cases `min <= 0` (falls back to linear `clamped*max`, line 437-440); when `min > 0` it computes `min * pow(max/min, clamped)` (line 441). If a caller constructs Exponential{min:20, max:-1} (or any max<=0 with min>0), `max/min` is negative and `pow(negative, fractional)` returns NaN, which then propagates silently into frequency/time controls. There is no validation that max>min>0.

**Recommendation.** Validate/clamp so that Exponential requires 0<min<=max, or guard the negative-base case explicitly and fall back to linear.

### Q090 — to_def discards metadata and version handling is unused (no forward-compat)

- **Severity:** low  |  **Status:** unverified  |  **Dimension:** `correct-serialize`  |  **Location:** `src/serialize.rs:1269`
- **Remediation:** **Fixed** — to_def preserves PatchMeta metadata and CURRENT_PATCH_VERSION is documented as advisory for forward-compat (wave-e/serialize, serialize.rs).

**Finding.** to_def hardcodes version:1 and sets author/description/tags to None/empty and parameters to an empty map (1269-1278), so converting a loaded PatchDef through a live Patch and back loses all metadata. The version field is never inspected on load: from_json/from_def ignore it and validate() only checks version>=1 (1448), so there is no migration path or rejection of unknown future versions — the 'forward compatibility' the field documents is not implemented.

**Recommendation.** Preserve metadata in Patch (or thread it through to_def), and either act on version in from_def (migrate/reject) or document it as advisory only.

### Q097 — dispose()/processor never free the engine or stop the processor

- **Severity:** low  |  **Status:** unverified  |  **Dimension:** `correct-wasm`  |  **Location:** `packages/@quiver/wasm/src/audio.ts:165`
- **Remediation:** **Fixed** — dispose() now frees the engine and stops the processor via the free/destroy lifecycle (wave-e/wasm-ts, packages/@quiver).

**Finding.** dispose() (audio.ts:165-168) calls node.disconnect() and node.port.close() but never signals the worklet to free its QuiverEngine. QuiverProcessor.process() always `return true` (worklet.ts:279), so the processor never self-terminates; it only stops when the browser GCs the whole node. The WASM engine's linear memory is held until then, and there is no explicit engine.free().

**Recommendation.** Add a 'destroy' message handled in the worklet that calls engine.free() and returns false from process() thereafter; call it from dispose() before closing the port.

### Q098 — tick() typed as tuple but returns a Float64Array at runtime; doc/API name drift

- **Severity:** low  |  **Status:** unverified  |  **Dimension:** `correct-wasm`  |  **Location:** `packages/@quiver/react/src/hooks.ts:75`
- **Remediation:** **Fixed** — tick() TS types corrected to the real Float64Array runtime return, fixing the tuple doc/API drift (wave-e/wasm-ts, packages/@quiver/types).

**Finding.** hooks.ts:75 types `tick(): [number, number]`, but Rust returns `Box<[f64]>` (engine.rs:457-460), which wasm-bindgen marshals as a Float64Array — indexing works but Array.isArray/tuple-destructure semantics differ and it is a fresh allocation per call. Separately, wasm/CLAUDE.md documents get_patch()/clear()/disconnect(cable_id) while the real methods are save_patch/clear_patch/disconnect(from,to) (engine.rs:117,132,300), so the doc contract does not match the bindings.

**Recommendation.** Type tick() as Float64Array, and update wasm/CLAUDE.md to the actual method names/signatures.

### Q103 — OscPattern matching is a simplified stub that mis-implements OSC wildcard semantics

- **Severity:** low  |  **Status:** unverified  |  **Dimension:** `correct-rtio`  |  **Location:** `src/extended_io.rs:195`
- **Remediation:** **Fixed** — OscPattern rewritten with OSC 1.0 semantics (component-scoped * and ?), replacing the mis-implemented stub (wave-b/rtio, extended_io.rs).

**Finding.** matches() (self-described "Simplified matching") treats PatternSegment::Wildcard as `return true` matching the entire remainder (lines 212-214), so `/synth/*` matches `/synth/a/b/c`, whereas OSC `*` matches within a single path component only. CharClass has no range support: `[a-z]` is parsed as the literal set {a,-,z} (parser at 167-181 pushes chars verbatim, no `-` handling), and an unterminated `[` silently swallows the rest of the pattern. No exclamation-negation either. Not a memory-safety issue (all safe Vec/char ops, no raw byte parsing so no overrun), but bindings will match/ignore addresses incorrectly.

**Recommendation.** Implement per-component glob matching (component-scoped `*`/`?`, `[a-z]` ranges, `[!..]` negation, `{a,b}` alternation) per the OSC 1.0 spec, or document the limited grammar explicitly.

### Q104 — LevelMeterState peak-hold never truly holds after first decay

- **Severity:** low  |  **Status:** unverified  |  **Dimension:** `correct-rtio`  |  **Location:** `src/observer.rs:709`
- **Remediation:** **Fixed** — LevelMeterState resets samples_since_peak so peak-hold actually holds after the first decay (wave-b/rtio, visual.rs).

**Finding.** In update(), when peak_db <= peak_hold_db the code increments samples_since_peak and, once it exceeds peak_hold_samples, sets peak_hold_db = peak_db (lines 710-714) but never resets samples_since_peak. So on every subsequent call the counter stays above threshold and peak_hold_db is continuously overwritten with the current peak_db — the hold collapses to a follower with no hold time after the first expiry, and it snaps instantly rather than decaying.

**Recommendation.** Reset samples_since_peak to 0 after applying the decay, and decay peak_hold_db gradually (e.g. a dB/sample falloff) instead of jumping to peak_db.

### Q111 — Block-processing path is unused and still allocates

- **Severity:** low  |  **Status:** unverified  |  **Dimension:** `perf-tickpath`  |  **Location:** `src/port.rs:514`
- **Remediation:** **Fixed** — Added an allocation-free tick_block path (BlockPortValues::frame_into), proven by a counting-allocator test (0 allocs/1000 ticks) (wave-c/perf, graph.rs, tests/zero_alloc.rs).

**Finding.** Patch exposes no block tick (grep of graph.rs finds only per-sample tick), so simd.rs/AudioBlock infra never accelerates the graph. The default GraphModule::process_block (port.rs:514-526) also allocates per frame: it calls inputs.frame(i) which builds a new PortValues+map (port.rs:392-400) and creates a fresh out_frame each frame, so even the block API does not deliver allocation-free processing.

**Recommendation.** Add a real block tick on Patch that processes contiguous sample buffers per port across a block with dyn dispatch amortized once per block, and make BlockPortValues::frame borrow into preallocated storage rather than allocating a PortValues per frame.

### Q126 — Inconsistent error/return policy: Result vs panic vs silent no-op; PatchError not non_exhaustive

- **Severity:** low  |  **Status:** unverified  |  **Dimension:** `elegance-api`  |  **Location:** `src/graph.rs:163`
- **Remediation:** **Fixed** — PatchError made non_exhaustive and carries node/port/cycle context in Display, unifying the error/return policy (wave-b graph/port overhaul, graph.rs).

**Finding.** The public surface mixes three failure styles: `connect`/`remove`/`disconnect` return `Result<_,PatchError>`; `in_`/`out` panic; `set_param`/`set_output`/`set_position` silently no-op on bad NodeId. `PatchError` is a hand-rolled enum with manual Display and is not `#[non_exhaustive]`, so adding a variant is a breaking change for downstream match arms. No `thiserror` usage. This makes the library's contract hard to reason about and evolve.

**Recommendation.** Mark `PatchError` `#[non_exhaustive]` (adopt thiserror for Display), make `set_output`/`set_param` return `Result` or document the silent-ignore, and offer fallible port lookups so nothing on the happy path panics.

### Q127 — Prelude glob pollution: crate-root re-export of a ~150-name prelude

- **Severity:** low  |  **Status:** unverified  |  **Dimension:** `elegance-api`  |  **Location:** `src/lib.rs:196`
- **Remediation:** **Documented as intended** — The crate-root prelude glob re-export is kept and documented as intentional convenience (wave-b graph/port overhaul, lib.rs).

**Finding.** `pub use prelude::*;` at the crate root re-exports the entire prelude (≈150 items: combinators, ports, graph, every module, ChordType, WavetableType, DotStyle, TriggerMode, SIMD, RNG…) both as `quiver::X` and `quiver::prelude::X`. There is no lean import path — `use quiver::prelude::*` pulls everything, and the flat crate root duplicates it, raising name-collision risk in user code and cluttering docs. Enum-associated helper types (ArpPattern, ValueFormat) sit at top level beside core types with no grouping.

**Recommendation.** Keep the curated `prelude` but drop the blanket `pub use prelude::*` at the root; expose modules (`quiver::modules::Vco`) for targeted imports and reserve the root for a handful of truly core types.

### Q128 — Constructor conventions are inconsistent, and sample_rate is passed redundantly

- **Severity:** low  |  **Status:** unverified  |  **Dimension:** `elegance-api`  |  **Location:** `src/modules.rs:751`
- **Remediation:** **Documented as intended** — Convention codified in DEVELOPMENT.md + book (SR-dependent modules take sample_rate in new(); set_sample_rate authoritative). Full fn new() audit: one benign exception (Crosstalk) documented (wave-g/fix-ci).

**Finding.** Constructors vary with no rule: `Vco::new(sample_rate)`, `Svf::new(sample_rate)`, `Adsr::new(sample_rate)` take rate; `Vca::new()`, `StereoOutput::new()` take nothing; `Offset::new(offset)` takes a value. Yet `Patch::add` already calls `module.set_sample_rate(self.sample_rate)` (graph.rs:322), so `Vco::new(44100.0)` inside a 44100 patch passes the rate twice (see quick_taste.rs:11,14). A user cannot predict from the name whether `new` needs a rate, and the double-specification invites mismatched rates.

**Recommendation.** Standardize on `Module::new()` with sample rate supplied exclusively via `add`/`set_sample_rate`, or make all `new(sample_rate)` uniform; document the single source of truth for sample rate.

### Q133 — read_interpolated duplicated verbatim across four modules

- **Severity:** low  |  **Status:** unverified  |  **Dimension:** `elegance-internals`  |  **Location:** `src/modules.rs:1640`
- **Remediation:** **Fixed** — read_interpolated de-duplicated into one shared helper used by the four delay-based modules in the B-0 modules.rs split (modules/common.rs).

**Finding.** The wrapping linear-interpolated delay read is identical at 1640-1647 and 1946-1953 and structurally the same at 899-911 and 1027-1037 (the free-fn variant takes buffer/write_pos as args). All compute `read_pos1=(write_pos+len-delay_int)%len`, `read_pos2=…-1`, `s1*(1-frac)+s2*frac`. Four independent copies means a future fix (e.g. Hermite interpolation, or guarding `delay_int` overflow) must be applied four times and can drift.

**Recommendation.** Keep only the free-function form `read_interpolated(buffer, write_pos, delay)` and have Chorus/Flanger/DelayLine/Phaser call it.

### Q134 — Single 9915-line modules.rs should be a module directory

- **Severity:** low  |  **Status:** unverified  |  **Dimension:** `elegance-internals`  |  **Location:** `src/modules.rs:1`
- **Remediation:** **Fixed** — The 9915-line modules.rs was split into a modules/ directory of domain files (oscillators, filters, dynamics, etc.) in the B-0 refactor.

**Finding.** All 58 modules plus 172 tests live in one file (11,760 lines with mdk). CLAUDE.md already groups them into clear categories (Oscillators, Filters, Envelopes, Effects, Delays, Utilities, Logic/CV, Sequencing). A single file this size hurts compile-time incrementality, code navigation, and review diff locality, and makes the duplication above easy to miss.

**Recommendation.** Split into `src/modules/{osc,filter,env,dynamics,effect,delay,util,logic,sequence,mod}.rs` re-exported from `modules/mod.rs`; move shared idioms into `modules/common.rs` during the split.

### Q135 — Inconsistent naming for edge-detection state fields (last_ vs prev_)

- **Severity:** low  |  **Status:** unverified  |  **Dimension:** `elegance-internals`  |  **Location:** `src/modules.rs:66`
- **Remediation:** **Fixed** — Edge-detection state fields unified on the EdgeDetector prev_* convention across analog.rs/dynamics.rs/utilities.rs (last_sync, last_gate, last_clock, last_reset, last_trigger renamed) in wave-g/fix-dsp.

**Finding.** Edge-detector state uses two conventions: `self.last_*` appears 34× (last_sync, last_reset, last_gate, last_trigger, last_clock, last_retrig) and `self.prev_*` 12× (prev_gate, prev_clock, prev_sync, prev_reset). Both name the identical 'value on previous tick' concept, sometimes for the same signal (last_clock at 2627 vs prev_clock at 3053). This friction compounds the copy-paste edge-detection logic and makes grep-based reasoning harder.

**Recommendation.** Standardize on one prefix (e.g. `prev_`) and, better, encapsulate in a small `EdgeDetect { prev: f64 }` helper with a `rising(cur)` method, eliminating both the naming split and the 12 inline `> t && prev <= t` copies.

### Q140 — Unconditional cdylib crate-type conflates/amplifies the no_std breakage and burdens every build

- **Severity:** low  |  **Status:** unverified  |  **Dimension:** `elegance-features`  |  **Location:** `Cargo.toml:13`
- **Remediation:** **Documented as intended** — cdylib crate-type is kept (wasm-pack hard-requires it) and the decision documented, with an rlib override used for the no_std checks (wave-f/hygiene, Cargo.toml).

**Finding.** `[lib] crate-type = ["cdylib", "rlib"]` (line 13) is unconditional, not limited to wasm builds. Because cdylib is a final linked artifact, `cargo check --no-default-features` and `--features alloc` additionally emit 'no global memory allocator found', '#[panic_handler] function required', and 'unwinding panics are not supported without std' (3 of the 14 errors in finding #1). Verified by copying the crate to a scratch dir with crate-type reduced to `["rlib"]`: those 3 lang-item errors disappeared, leaving only the genuine libm-bypass E0599 errors. This also means every ordinary `cargo build` (std consumers included) compiles an unused cdylib artifact.

**Recommendation.** Restrict cdylib production to the wasm build path (e.g. build via `wasm-pack`/`cargo rustc --crate-type=cdylib --features wasm` rather than declaring it unconditionally in Cargo.toml), so plain library checks/builds aren't forced through final-artifact lang-item resolution.

### Q148 — Sidechain routing exists only on Compressor; no generic ducking or shared mechanism

- **Severity:** low  |  **Status:** unverified  |  **Dimension:** `complete-domain`  |  **Location:** `src/modules.rs:1369`
- **Remediation:** **Implemented** — Added a generic Ducker plus sidechain key ports on Limiter (id4) and NoiseGate (id5), extending sidechaining beyond Compressor (wave-e/new-modules, modules/dynamics.rs).

**Finding.** Compressor alone has a dedicated `sidechain` input port (modules.rs:1369, consumed at 1398-1413 as the envelope-detector source instead of its own input). NoiseGate (1285-1340) and Limiter (1151-1188) - the other two dynamics modules that would commonly benefit from external key input for ducking/gating - expose no equivalent port. There's also no standalone 'sidechain duck' utility module for the common EDM-style ducking patch outside a compressor.

**Recommendation.** Add an optional sidechain/key input to NoiseGate and Limiter mirroring Compressor's pattern, or add a small dedicated `Ducker` module.

### Q149 — Wavefolder is a fully working module but orphaned outside modules.rs, hurting discoverability

- **Severity:** low  |  **Status:** unverified  |  **Dimension:** `complete-domain`  |  **Location:** `src/analog.rs:679`
- **Remediation:** **Fixed** — Wavefolder moved into modules/nonlinear (with an analog re-export for compatibility), fixing its discoverability (B-0 modules.rs split).

**Finding.** `Wavefolder` (analog.rs:679-725) is a complete, tested GraphModule (type_id 'wavefolder', registered in serialize.rs:792-798 and presets.rs:693) implementing classic West-Coast folding, but it lives in analog.rs among drift/saturation utilities rather than alongside all other 55+ DSP modules in modules.rs where the CLAUDE.md module inventory and most users would look for it. It is real and usable (present, not missing) but its placement makes it easy to overlook as a synthesis building block.

**Recommendation.** Move Wavefolder to modules.rs alongside Distortion/Bitcrusher, or at minimum cross-reference it in modules.rs docs so it's discoverable as a standard waveshaping tool.

### Q150 — No mid/side encode-decode utilities for stereo-bus processing

- **Severity:** low  |  **Status:** unverified  |  **Dimension:** `complete-domain`  |  **Location:** `src/modules.rs:1`
- **Remediation:** **Implemented** — Added MidSideEncode and MidSideDecode (with width control) for stereo-bus processing (wave-e/new-modules, modules).

**Finding.** The module inventory has Mixer, Attenuverter, Crossfader, PrecisionAdder and a stereo-aware Reverb/Chorus, but no Mid/Side encoder or decoder (M = (L+R)/2, S = (L-R)/2 and inverse) anywhere in modules.rs, simd.rs, or elsewhere. This is a standard building block for stereo-width control, mastering-style EQ, and dual-mono correction that a modular-style stereo toolkit would typically include; it is currently unreachable without hand-writing PrecisionAdder/Attenuverter combinations manually per patch.

**Recommendation.** Add a small `MidSideEncode`/`MidSideDecode` pair of GraphModules (or a combined module) implementing M=(L+R)*0.5, S=(L-R)*0.5 and the inverse.

### Q156 — MSRV inconsistent across docs (1.70 vs 1.78)

- **Severity:** low  |  **Status:** unverified  |  **Dimension:** `complete-docs`  |  **Location:** `docs/src/getting-started/installation.md:7`
- **Remediation:** **Fixed** — MSRV made consistent at 1.78 across docs, resolving the 1.70 vs 1.78 discrepancy (wave-f/docs).

**Finding.** docs/src/getting-started/installation.md:7 states '**Rust 1.70+** (2021 edition)' while README.md's badge (README.md:7) reads 'rust-1.78%2B' and CLAUDE.md's CI/CD Pipeline section states 'MSRV check (Rust 1.78)'. Cargo.toml has no `rust-version` field pinned at all (only `edition = "2021"`), so neither number is enforced by the build, but the two docs disagree with each other on the actual minimum.

**Recommendation.** Pin `rust-version = "1.78"` in Cargo.toml (matching the CI MSRV job) and update installation.md's prerequisite to match, so cargo itself enforces the documented minimum.

### Q165 — No property-based testing anywhere in the crate

- **Severity:** low  |  **Status:** unverified  |  **Dimension:** `complete-tests`  |  **Location:** `Cargo.toml:1`
- **Remediation:** **Fixed** — Added property-based (proptest) tests to the crate (wave-f/tests, tests/proptest).

**Finding.** Cargo.toml's [dev-dependencies] lists only `approx` and `criterion`; there is no `proptest` or `quickcheck` dependency, and no such tests exist. All DSP stability checks (the '_bounded' tests in modules.rs) use a small number of hand-picked point values (e.g. resonance=0.9, input=20.0) rather than randomized/generated ranges, so parameter combinations outside the chosen points (e.g. resonance=0.99, cutoff near Nyquist) are unverified.

**Recommendation.** Introduce proptest for at least the filter and quantizer modules, generating random cutoff/resonance/input-amplitude combinations and asserting output stays finite and bounded over N ticks.

### Q171 — README Feature Flags table omits `wasm` feature documented in CLAUDE.md/Cargo.toml

- **Severity:** low  |  **Status:** unverified  |  **Dimension:** `usable-examples`  |  **Location:** `README.md:98`
- **Remediation:** **Fixed** — README Feature Flags table now documents the wasm feature (wave-f/docs, README.md).

**Finding.** README.md:98-102 lists only std/alloc/simd in the Feature Flags table, while Cargo.toml (verified lines ~46-53) and CLAUDE.md both document a fourth `wasm` feature. A newcomer reading only the README doesn't learn that WASM bindings are gated behind `--features wasm`.

**Recommendation.** Add the `wasm` row to the README feature table pointing to packages/@quiver/wasm and demos/browser.

### Q179 — Orphaned dist/ build output in @quiver/wasm doesn't match current source or any script

- **Severity:** low  |  **Status:** unverified  |  **Dimension:** `usable-ts`  |  **Location:** `packages/@quiver/wasm/dist:1`
- **Remediation:** **Fixed** — The stale @quiver/wasm/dist/ artifact was removed and gitignored via packages/@quiver/*/dist/ (wave-e/wasm-ts, .gitignore).

**Finding.** packages/@quiver/wasm/dist/ is untracked (per `git status`) and contains files like audio-manager.js/mjs and quiver-worklet.js/quiver-worker.js that reference an `audio-manager.ts` source which does not exist in src/ (only audio.ts, index.ts, worklet.ts are present). No script in package.json (build/build:dev/prepublishOnly all just shell to `make wasm`) produces this directory, indicating a stale artifact from a different/earlier build pipeline that was never reconciled or cleaned up, and will confuse a contributor who inspects it expecting it to reflect current source.

**Recommendation.** Delete the stale dist/ directory, add it to .gitignore explicitly, and once a real build step is added (finding #1) make sure it's the sole source of dist/ output.

### Q187 — Patch has no Debug impl for println-style inspection

- **Severity:** low  |  **Status:** unverified  |  **Dimension:** `usable-errors`  |  **Location:** `src/graph.rs:257`
- **Remediation:** **Fixed** — Patch gained a Debug impl for println-style inspection (wave-b graph/port overhaul, graph.rs).

**Finding.** The `Patch` struct (line 257) and its internal `Node` (line 156, containing `Box<dyn GraphModule>`) have no `#[derive(Debug)]` or manual `impl Debug`, and `GraphModule` (src/port.rs:506) does not require Debug. So `println!("{:?}", patch)` — the first thing most Rust developers reach for when a patch behaves unexpectedly — simply doesn't compile, pushing users toward reading the observer/introspection APIs, which are less discoverable and undocumented as debugging tools in connect-modules.md.

**Recommendation.** Add a manual `impl fmt::Debug for Patch` that prints node names/types, cable list, output_node, and validation_mode/warnings, without requiring GraphModule: Debug.

### Q188 — SpectrumAnalyzer::peak_frequency() panics on NaN input instead of degrading gracefully

- **Severity:** low  |  **Status:** unverified  |  **Dimension:** `usable-errors`  |  **Location:** `src/visual.rs:787`
- **Remediation:** **Fixed** — SpectrumAnalyzer::peak_frequency() degrades gracefully on NaN bins instead of panicking (wave-b/rtio, visual.rs).

**Finding.** `peak_frequency()` does `self.spectrum.iter().enumerate().max_by(|(_,a),(_,b)| a.partial_cmp(b).unwrap())` (line 787). If any bin is NaN (plausible after divergent feedback, filter self-oscillation blow-up, or a 0/0 in FFT normalization elsewhere in the signal path — the very failure this tool exists to diagnose), `partial_cmp` returns None and `.unwrap()` panics. The one tool meant to help debug 'why is my patch behaving strangely' can itself crash instead of reporting a bogus/degenerate reading.

**Recommendation.** Replace with `.max_by(|(_,a),(_,b)| a.partial_cmp(b).unwrap_or(Ordering::Equal))` or filter out NaN bins before the max_by, returning a sentinel frequency (e.g. 0.0) on all-NaN input.

### Q197 — Cargo.toml missing readme/documentation fields for crates.io publish readiness

- **Severity:** low  |  **Status:** unverified  |  **Dimension:** `hygiene`  |  **Location:** `Cargo.toml:9`
- **Remediation:** **Fixed** — Added readme and documentation fields to Cargo.toml for crates.io publish readiness (wave-f/hygiene, Cargo.toml).

**Finding.** Cargo.toml has name, version, edition, authors, description, license, repository, keywords, categories (lines 2-10) but no 'readme' field pointing at README.md and no explicit 'documentation' field. crates.io will publish without them (falling back to docs.rs default), but the crates.io package page won't render the README, and `cargo publish --dry-run` typically warns about the missing readme field.

**Recommendation.** Add `readme = "README.md"` and optionally `documentation = "https://docs.rs/quiver"` to the [package] section.

## Refuted findings

### Input mixing sums all cables ignoring SignalKind; two gates sum to 10V

- **Claimed severity:** high  |  **Dimension:** `correct-graph`  |  **Location:** `src/graph.rs`

**Refutation.** Code fact is true: gather_inputs (graph.rs:698-712) sums every cable regardless of SignalKind, and is_summable()/gate_threshold() (port.rs:64-81) are dead — grep finds zero callers outside port.rs. But the claimed harm doesn't materialize. Every gate/trigger/clock consumer thresholds at 2.5V and detects edges relative to that same threshold: Adsr (modules.rs:564-566 gate_high=gate>2.5, rising=high&&last<=2.5), sync (72), logic (3768,3816,3864), seq (3052). Summing two 5V gates→10V still reads high, and edge/falling detection keys on "were all inputs <2.5 before", i.e. correct logical-OR semantics — no spurious/missed edges. Comparator (3960-3972) takes CV, not gates. V/Oct and CV ARE summable by design (is_summable true), so "precision-adder conflation" is wrong. No module reads raw gate voltage proportionally. Not a bug under normal usage; "high" unjustified.
