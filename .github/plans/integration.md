# External Integration

Plugin formats and platform integrations requiring external dependencies.

---

## Plugin Formats

### VST3

**Crate**: `vst3-sys`

Required:
- [ ] Implement VST3 wrapper using `PluginProcessor` trait
- [ ] Parameter mapping to VST3 parameters
- [ ] State save/load via VST3 preset chunks
- [ ] Example VST3 plugin project

### Audio Unit (macOS/iOS)

**Crate**: `coreaudio-rs`

Required:
- [ ] Implement AU wrapper using `PluginProcessor` trait
- [ ] AUv3 support for iOS
- [ ] Example AU plugin project

### LV2 (Linux)

**Crate**: `lv2`

Required:
- [ ] Implement LV2 wrapper using `PluginProcessor` trait
- [ ] LV2 atom/MIDI handling
- [ ] Example LV2 plugin project

---

## WASM/Browser

### Testing Infrastructure

- [ ] Headless browser tests (Playwright or Puppeteer)
- [ ] Performance benchmarks in browser
- [ ] Cross-browser compatibility testing (Chrome, Firefox, Safari)

### Package Publishing

- [ ] npm package publishing workflow
- [ ] TypeScript type definitions verification
- [ ] CDN distribution (unpkg, jsdelivr)

### Example Project

- [ ] Minimal browser synth example
- [ ] AudioWorklet setup boilerplate
- [ ] MIDI input handling in browser

---

## Implementation Notes

### Plugin Wrapper Pattern

All plugin formats should use the existing `PluginProcessor` trait:

```rust
impl PluginProcessor for MyPlugin {
    fn initialize(&mut self, sample_rate: f64, max_block_size: usize);
    fn process(&mut self, inputs: &[&[f32]], outputs: &mut [&mut [f32]], context: &mut ProcessContext);
    fn reset(&mut self);
    fn set_parameter(&mut self, id: u32, value: f64);
    fn get_parameter(&self, id: u32) -> f64;
    // ...
}
```

The format-specific wrapper handles:
- Format-specific initialization
- Parameter discovery and automation
- State serialization format
- MIDI message translation

### Browser AudioWorklet Pattern

```javascript
class QuiverProcessor extends AudioWorkletProcessor {
  constructor() {
    super();
    this.engine = new QuiverEngine(sampleRate);
    this.engine.load_patch(patchJson);
    this.engine.compile();
  }

  process(inputs, outputs, parameters) {
    const output = outputs[0];
    const samples = this.engine.process_block(128);

    for (let i = 0; i < 128; i++) {
      output[0][i] = samples[i * 2];
      output[1][i] = samples[i * 2 + 1];
    }
    return true;
  }
}
```
