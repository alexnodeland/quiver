# Render Offline to WAV

Render a patch offline—faster than real-time—and write the result to a
standard `.wav` file you can play in any audio player. Requires the `std`
feature (enabled by default).

## Quick Version

`render_to_wav` does everything in one call:

```rust,ignore
use quiver::prelude::*;   // re-exports render and render_to_wav
use std::path::Path;

let mut patch = Patch::new(44100.0);
// ... add modules, connect, set_output ...

// Render 2 seconds and write a 16-bit PCM stereo WAV
render_to_wav(&mut patch, 2.0, Path::new("target/my_patch.wav"))?;
```

The patch is compiled lazily if needed, so a freshly-built patch renders
without an explicit `compile()` call.

## Rendering to Buffers

`render` returns raw `(left, right)` sample buffers so you can analyze or
post-process before writing:

```rust,ignore
// Number of frames = round(seconds * patch.sample_rate())
let (left, right) = render(&mut patch, 1.0);

let peak = left.iter().map(|s| s.abs()).fold(0.0_f64, f64::max);
println!("Generated {} samples, peak {:.2}V", left.len(), peak);
```

Rendering drives the same per-sample engine as `patch.tick()` with no
per-frame allocation—the samples are identical to ticking one sample at a
time.

## Writing Buffers with `write_wav`

`write_wav` writes any pair of channel buffers (not in the prelude—import it
from `quiver::render`):

```rust,ignore
use quiver::render::write_wav;

write_wav(Path::new("target/out.wav"), 44100, &left, &right)?;
```

If the channel lengths differ, the shorter length is used. The output is
always 16-bit PCM stereo with the sample rate you pass.

## Watch the Sample Scale

WAV samples are full-scale `[-1.0, 1.0]`, but Quiver's `Audio` ports follow
the modular-synth ±5V convention. Both `render_to_wav` and `write_wav` treat
the buffers as full-scale and **clamp** anything outside ±1.0—so a raw ±5V
signal will clip hard at full scale.

Scale down before writing, either in the patch (e.g. through a `Vca` or
`Attenuverter`) or on the rendered buffers:

```rust,ignore
// ±5V modular convention -> ±1.0 full scale
let to_full_scale = |buf: &[f64]| -> Vec<f64> { buf.iter().map(|s| s / 5.0).collect() };
write_wav(path, 44100, &to_full_scale(&left), &to_full_scale(&right))?;
```

Resonant filters can briefly overshoot 5V on sharp attacks; divide by a
little more (e.g. 6.0) to leave headroom. There is no automatic
normalization—what you pass is what gets written.

## Sequencing While Rendering

Because `render` advances the patch in-place, you can call it repeatedly
while changing control values between calls—for example driving pitch and
gate via `ExternalInput`:

```rust,ignore
for &note in &[48u8, 52, 55, 60] {
    pitch_cv.set((note as f64 - 60.0) / 12.0);
    gate_cv.set(5.0);
    let (l, r) = render(&mut patch, 0.2);   // note on
    left_all.extend(l);
    right_all.extend(r);

    gate_cv.set(0.0);
    let (l, r) = render(&mut patch, 0.05);  // release tail
    left_all.extend(l);
    right_all.extend(r);
}

write_wav(path, 44100, &left_all, &right_all)?;
```

## Complete Example

The `render_wav` example renders a sequenced arpeggio through a resonant
filter to `target/render_wav.wav`. Run it with
`cargo run --example render_wav`:

```rust,ignore
{{#include ../../../examples/render_wav.rs}}
```

For the minimal patch-to-WAV workflow, see `quick_taste`
(`cargo run --example quick_taste`), which writes `target/quick_taste.wav`.
