# Quiver Development Plan

This document outlines the development roadmap and current status for the Quiver modular audio synthesis library.

## Current Status

**Version**: 0.4.0 (Phase 4 Complete)

The library now includes all Phase 1 (Core Foundation), Phase 2 (Hardware Fidelity), Phase 3 (Analog Modeling Refinement), and Phase 4 (Advanced Features) features:

### Completed Features

#### Layer 1: Typed Combinators
- [x] Core `Module` trait with `tick()` method
- [x] `Chain` - sequential composition (`>>>`)
- [x] `Parallel` - parallel composition (`***`)
- [x] `Fanout` - split to parallel processors (`&&&`)
- [x] `Feedback` - feedback loop with unit delay
- [x] `Map` / `Contramap` - signal transformation
- [x] `Split` / `Merge` - tuple splitting and combining
- [x] `Swap` - tuple element swap
- [x] `First` / `Second` - apply to tuple element
- [x] `Identity` / `Constant` - basic primitives

#### Layer 2: Port System
- [x] `SignalKind` enum (Audio, VoiceOct, Gate, Trigger, CvUnipolar, CvBipolar)
- [x] `PortDef` with metadata (default values, attenuverter support)
- [x] `PortSpec` for module port definitions
- [x] `PortValues` for runtime signal storage
- [x] `GraphModule` trait for port-based modules
- [x] `ModulatedParam` with `ParamRange` (Linear, Exponential, V/Oct)

#### Layer 3: Patch Graph
- [x] `Patch` graph container with SlotMap-based storage
- [x] `NodeHandle` for ergonomic module access
- [x] `Cable` connections with optional attenuation
- [x] Topological sort (Kahn's algorithm) for processing order
- [x] Cycle detection
- [x] Input summing (multiple cables to one input)
- [x] Mult support (one output to multiple inputs)

#### Core DSP Modules
- [x] `Vco` - Voltage-controlled oscillator (sine, saw, triangle, pulse)
- [x] `Lfo` - Low-frequency oscillator
- [x] `Svf` - State-variable filter (LP, BP, HP, Notch)
- [x] `Adsr` - Envelope generator
- [x] `Vca` - Voltage-controlled amplifier
- [x] `Mixer` - Multi-channel mixer
- [x] `Offset` - DC offset / voltage source
- [x] `UnitDelay` - Single-sample delay
- [x] `NoiseGenerator` - White and pink noise
- [x] `StepSequencer` - 8-step CV/gate sequencer
- [x] `StereoOutput` - Final output module

#### Utility Modules
- [x] `SampleAndHold` - Sample input on trigger
- [x] `SlewLimiter` - Rate limiting / portamento
- [x] `Quantizer` - V/Oct to scale quantization (8 scales)
- [x] `Clock` - Master tempo clock with divisions
- [x] `Attenuverter` - Signal attenuation/inversion
- [x] `Multiple` - Signal splitter (1 to 4)

#### Analog Modeling
- [x] Saturation functions (`tanh_sat`, `soft_clip`, `wavefold`)
- [x] `ComponentModel` - Component tolerance simulation
- [x] `ThermalModel` - Temperature drift modeling
- [x] Noise generators (white, pink, power supply hum)
- [x] `AnalogVco` - VCO with analog imperfections
- [x] `Saturator` - Saturation effect module
- [x] `Wavefolder` - Wavefolding effect module

#### External I/O
- [x] `AtomicF64` - Lock-free thread-safe values
- [x] `ExternalInput` - External signal injection
- [x] `ExternalOutput` - Signal extraction
- [x] `MidiState` - MIDI to CV/Gate conversion

#### Serialization
- [x] `PatchDef` - JSON-serializable patch definition
- [x] `ModuleDef` / `CableDef` - Module and cable definitions
- [x] `ModuleRegistry` - Factory pattern for module instantiation
- [x] Round-trip serialization (save/load patches)

#### Infrastructure
- [x] CI pipeline (GitHub Actions)
- [x] Code formatting (rustfmt)
- [x] Linting (clippy)
- [x] 78 unit tests

#### Phase 2: Hardware Fidelity (Complete)

1. **Normalled Connections**
   - [x] `normalled_to` field in `PortDef`
   - [x] Automatic default routing when inputs are unpatched
   - [x] StereoOutput right channel normalled to left

2. **Signal Kind Validation**
   - [x] `ValidationMode` enum (None, Warn, Strict)
   - [x] `set_validation_mode()` method on `Patch`
   - [x] Signal compatibility checking with detailed warnings
   - [x] `SignalMismatch` error for Strict mode

3. **Phase 2 Modules**
   - [x] `RingModulator` - Four-quadrant multiplier for metallic sounds
   - [x] `Crossfader` - Equal-power crossfade with stereo panning
   - [x] `LogicAnd` / `LogicOr` / `LogicXor` / `LogicNot` - Gate logic
   - [x] `Comparator` - CV comparison with gt/lt/eq outputs
   - [x] `Rectifier` - Full-wave, half-wave, and absolute value
   - [x] `PrecisionAdder` - High-precision V/Oct summing
   - [x] `VcSwitch` - Voltage-controlled signal router
   - [x] `BernoulliGate` - Probabilistic trigger router
   - [x] `Min` / `Max` - Signal minimum/maximum

4. **Improved Modulation**
   - [x] Extended `Cable` with `offset` field (-10V to +10V)
   - [x] `connect_modulated()` method for attenuation + offset
   - [x] Attenuverter range (-2.0 to 2.0) for signal inversion
   - [x] Updated serialization for modulated cables

#### Phase 3: Analog Modeling Refinement (Complete)

1. **Enhanced VCO Modeling**
   - [x] `VoctTrackingModel` - V/Oct tracking errors with octave-dependent drift
   - [x] `HighFrequencyRolloff` - Frequency-dependent amplitude rolloff
   - [x] Improved oscillator sync with soft ramp for smoother transients
   - [x] Enhanced `AnalogVco` integrating all new modeling features

2. **Filter Improvements**
   - [x] Self-oscillation capability in `Svf` at high resonance (>0.95)
   - [x] Keyboard tracking inputs (`keytrack`, `keytrack_amt`) for Svf
   - [x] `DiodeLadderFilter` - 24dB/oct ladder filter with diode saturation

3. **Improved Noise Models**
   - [x] Correlated stereo noise outputs in `NoiseGenerator`
   - [x] `Crosstalk` - Channel crosstalk simulation with HF emphasis
   - [x] `GroundLoop` - 50/60 Hz hum with harmonics and thermal modulation

#### Phase 4: Advanced Features (Complete)

1. **Polyphony Support**
   - [x] `VoiceAllocator` - Voice allocation with multiple algorithms
   - [x] `AllocationMode` - Round-robin, oldest-steal, quietest-steal, priority modes
   - [x] `Voice` - Per-voice state management (note, velocity, gate, trigger)
   - [x] `PolyPatch` - Polyphonic patch container
   - [x] `VoiceInput` - Per-voice CV injection module
   - [x] `VoiceMixer` - Multi-voice summing
   - [x] `UnisonConfig` - Unison/spread with detune and stereo panning

2. **Performance Optimization**
   - [x] `AudioBlock` - SIMD-aligned audio buffer
   - [x] SIMD vectorization (with `simd` feature flag)
   - [x] `BlockProcessor` - Block-oriented processing utilities
   - [x] `LazySignal` / `LazyBlock` - Lazy evaluation framework
   - [x] `StereoBlock` - Stereo audio block with pan/mix operations
   - [x] `RingBuffer` - Efficient delay line implementation
   - [x] `ProcessContext` - Block processing context with timing info

3. **Extended I/O**
   - [x] OSC protocol support (`OscMessage`, `OscPattern`, `OscReceiver`, `OscBinding`)
   - [x] `OscInput` - OSC to CV module
   - [x] Plugin wrapper infrastructure (`PluginWrapper`, `PluginParameter`, `PluginInfo`)
   - [x] `AudioBusConfig` - Plugin audio bus configuration
   - [x] Web Audio interface (`WebAudioProcessor`, `WebAudioWorklet`, `WebAudioConfig`)
   - [x] Stereo interleave/deinterleave utilities for Web Audio

---

## Development Roadmap

### Phase 5: Ecosystem

1. **Module Development Kit**
   - Module template generator
   - Testing harness
   - Documentation generator

2. **Preset Library**
   - Classic synth patches
   - Sound design examples
   - Tutorial patches

3. **Visual Tools**
   - Patch visualization (DOT/GraphViz export)
   - Parameter automation recording
   - Scope/analyzer modules

---

## Contributing

Contributions are welcome! Here are areas where help is particularly appreciated:

- **DSP algorithms**: More accurate filter models, oscillator antialiasing
- **Testing**: Audio comparison tests, performance benchmarks
- **Documentation**: API docs, tutorials, examples
- **Modules**: Implementations of classic hardware module behaviors

---

## Architecture Decisions

### Why SlotMap?
The patch graph uses `slotmap::SlotMap` for node storage because:
- O(1) insertion and removal
- Stable handles (IDs don't change when other nodes are removed)
- Memory efficient
- Serde support for serialization

### Why f64?
All audio processing uses `f64` for:
- Headroom for accumulation without clipping
- Precision for filters and feedback
- Consistent with scientific computing conventions
- Down-convert to f32 only at I/O boundaries

### Why No `no_std`?
Current implementation requires `std` for:
- `HashMap` in `PortValues` and serialization
- Random number generation
- Potential future threading support

Consider adding a `no_std` feature in the future with alternative implementations.

---

## Testing Guidelines

- Unit tests should cover all module behaviors
- Test edge cases: zero input, maximum input, NaN handling
- Audio quality tests should verify no unexpected clipping or artifacts
- Performance tests should measure processing overhead
