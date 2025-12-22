# Observable Streaming

Quiver provides real-time data streams for building responsive visualizations like level meters, oscilloscopes, and spectrum analyzers.

## Observable Types

| Type | Description | Data |
|------|-------------|------|
| `Param` | Parameter value changes | `{ value: f64 }` |
| `Level` | Audio level metering | `{ rms_db: f64, peak_db: f64 }` |
| `Gate` | Binary on/off state | `{ active: bool }` |
| `Scope` | Waveform samples | `{ samples: f32[] }` |
| `Spectrum` | FFT magnitude | `{ bins: f32[], freq_range: [f32, f32] }` |

## Subscribing to Updates

### Subscribe

```typescript
engine.subscribe([
  // Level meter on output module, port 0
  { type: 'level', node_id: 'output', port_id: 0 },

  // Oscilloscope on VCO output
  { type: 'scope', node_id: 'vco', port_id: 0, buffer_size: 512 },

  // Gate state on LFO square output
  { type: 'gate', node_id: 'lfo', port_id: 1 },

  // Spectrum analyzer
  { type: 'spectrum', node_id: 'output', port_id: 0, fft_size: 256 },

  // Parameter tracking
  { type: 'param', node_id: 'vco', param_id: '0' }
]);
```

### Unsubscribe

```typescript
// Unsubscribe by ID
engine.unsubscribe([
  'level:output:0',
  'scope:vco:0'
]);

// Clear all subscriptions
engine.clear_subscriptions();
```

## Polling Updates

Call `poll_updates()` in your render loop to receive accumulated updates:

```typescript
function animate() {
  // Get all pending updates since last poll
  const updates = engine.poll_updates();

  for (const update of updates) {
    switch (update.type) {
      case 'param':
        handleParamUpdate(update.node_id, update.param_id, update.value);
        break;

      case 'level':
        handleLevelUpdate(update.node_id, update.port_id,
                          update.rms_db, update.peak_db);
        break;

      case 'gate':
        handleGateUpdate(update.node_id, update.port_id, update.active);
        break;

      case 'scope':
        handleScopeUpdate(update.node_id, update.port_id, update.samples);
        break;

      case 'spectrum':
        handleSpectrumUpdate(update.node_id, update.port_id,
                             update.bins, update.freq_range);
        break;
    }
  }

  requestAnimationFrame(animate);
}

requestAnimationFrame(animate);
```

## Update Deduplication

The observer automatically deduplicates updates:

- Only the **latest** value for each subscription is kept
- At most **1000 pending updates** are buffered
- Oldest updates are dropped if buffer overflows

This ensures the UI always shows current state without flooding.

## Level Metering

Level updates provide RMS and peak measurements in decibels:

```typescript
// Subscribe to level
engine.subscribe([
  { type: 'level', node_id: 'output', port_id: 0 }
]);

// Handle updates
function handleLevelUpdate(nodeId, portId, rmsDb, peakDb) {
  // rmsDb: Root-mean-square level (-inf to 0 dB)
  // peakDb: Peak level (-inf to 0 dB)

  // Map to meter height (0-100%)
  const rmsHeight = Math.max(0, (rmsDb + 60) / 60 * 100);
  const peakHeight = Math.max(0, (peakDb + 60) / 60 * 100);

  meterElement.style.setProperty('--rms', `${rmsHeight}%`);
  meterElement.style.setProperty('--peak', `${peakHeight}%`);
}
```

### Level Meter Configuration

The observer uses a 128-sample buffer by default (~3ms at 44.1kHz), providing smooth metering at 60Hz update rate.

## Gate Detection

Gate updates fire on state changes with hysteresis:

- **On threshold**: > 2.5V
- **Off threshold**: < 0.5V

```typescript
engine.subscribe([
  { type: 'gate', node_id: 'lfo', port_id: 1 }
]);

function handleGateUpdate(nodeId, portId, active) {
  ledElement.classList.toggle('active', active);
}
```

## Oscilloscope Display

Scope updates provide a buffer of waveform samples:

```typescript
engine.subscribe([
  { type: 'scope', node_id: 'vco', port_id: 0, buffer_size: 512 }
]);

function handleScopeUpdate(nodeId, portId, samples) {
  const canvas = scopeCanvas;
  const ctx = canvas.getContext('2d');
  const width = canvas.width;
  const height = canvas.height;

  ctx.clearRect(0, 0, width, height);
  ctx.beginPath();

  for (let i = 0; i < samples.length; i++) {
    const x = (i / samples.length) * width;
    const y = (1 - (samples[i] + 1) / 2) * height;

    if (i === 0) ctx.moveTo(x, y);
    else ctx.lineTo(x, y);
  }

  ctx.stroke();
}
```

### Buffer Size

Choose buffer size based on your needs:

| Size | Duration @ 44.1kHz | Use Case |
|------|-------------------|----------|
| 128 | 2.9ms | Fast updates, percussion |
| 256 | 5.8ms | General purpose |
| 512 | 11.6ms | Smooth waveforms |
| 1024 | 23.2ms | Low-frequency LFOs |

## Spectrum Analyzer

Spectrum updates provide FFT magnitude bins in dB:

```typescript
engine.subscribe([
  { type: 'spectrum', node_id: 'output', port_id: 0, fft_size: 256 }
]);

function handleSpectrumUpdate(nodeId, portId, bins, freqRange) {
  // bins: magnitude in dB for each frequency bin (-100 to 0)
  // freqRange: [minHz, maxHz] (e.g., [0, 22050])

  const canvas = spectrumCanvas;
  const ctx = canvas.getContext('2d');
  const width = canvas.width;
  const height = canvas.height;
  const binWidth = width / bins.length;

  ctx.clearRect(0, 0, width, height);

  for (let i = 0; i < bins.length; i++) {
    // Map dB to height (clamped to -60dB floor)
    const db = Math.max(-60, bins[i]);
    const barHeight = ((db + 60) / 60) * height;

    ctx.fillRect(
      i * binWidth,
      height - barHeight,
      binWidth - 1,
      barHeight
    );
  }
}
```

### FFT Configuration

| FFT Size | Bins | Freq Resolution @ 44.1kHz |
|----------|------|--------------------------|
| 128 | 64 | 344 Hz |
| 256 | 128 | 172 Hz |
| 512 | 256 | 86 Hz |
| 1024 | 512 | 43 Hz |

The DFT uses a Hann window to reduce spectral leakage.

## React Hooks

The `@quiver/react` package provides hooks for common patterns:

```tsx
import {
  useQuiverLevel,
  useQuiverScope,
  useQuiverGate,
  useQuiverSpectrum
} from '@quiver/react';

function OutputMeter({ engine }) {
  const { rms_db, peak_db } = useQuiverLevel(engine, 'output', 0);
  return <Meter rms={rms_db} peak={peak_db} />;
}

function VcoScope({ engine }) {
  const { samples } = useQuiverScope(engine, 'vco', 0, 512);
  return <Oscilloscope samples={samples} />;
}

function LfoLed({ engine }) {
  const { active } = useQuiverGate(engine, 'lfo', 1);
  return <Led on={active} />;
}
```

## Performance Tips

1. **Subscribe only to what you display** - Unused subscriptions waste CPU
2. **Use appropriate buffer sizes** - Larger = less CPU, slower updates
3. **Throttle UI updates** - 60fps is usually sufficient
4. **Batch DOM updates** - Use `requestAnimationFrame` grouping
5. **Consider Web Workers** - Offload FFT visualization to worker
