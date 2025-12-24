# Examples

This directory contains runnable Rust examples demonstrating Quiver's capabilities.

## Running Examples

```bash
# Run a specific example
cargo run --example <name>

# Or using make
make run-example NAME=<name>
make quick-taste              # Shortcut for quick_taste

# Build all examples
make examples
```

## Example Categories

### Getting Started
| Example | Description |
|---------|-------------|
| `quick_taste` | Minimal example showing core workflow |
| `first_patch` | First patch tutorial with detailed comments |
| `simple_patch` | Simple patch demonstration |

### Synthesis Tutorials
| Example | Description |
|---------|-------------|
| `tutorial_subtractive` | Classic subtractive synthesis (VCO → VCF → VCA) |
| `tutorial_envelope` | ADSR envelope usage and modulation |
| `tutorial_filter_mod` | Filter modulation with LFO |
| `tutorial_fm` | FM synthesis techniques |
| `tutorial_polyphony` | Polyphonic synthesis with voice allocation |
| `tutorial_sequenced_bass` | Step sequencer-driven bass patch |

### How-To Guides
| Example | Description |
|---------|-------------|
| `howto_custom_module` | Creating custom DSP modules |
| `howto_midi` | MIDI integration and control |
| `howto_serialization` | Save/load patches as JSON |
| `howto_visualization` | Using scope and spectrum analyzer |

## Example Structure

Each example follows a similar pattern:

```rust
use quiver::prelude::*;

fn main() {
    // 1. Create patch at sample rate
    let mut patch = Patch::new(44100.0);

    // 2. Add modules
    let vco = patch.add("vco", Vco::new(44100.0));
    let output = patch.add("output", StereoOutput::new());

    // 3. Connect modules
    patch.connect(vco.out("saw"), output.in_("left")).unwrap();

    // 4. Set output and compile
    patch.set_output(output.id());
    patch.compile().unwrap();

    // 5. Process audio
    for _ in 0..44100 {
        let (left, right) = patch.tick();
        // Use samples...
    }
}
```

## Writing New Examples

When adding examples:

1. **Add to Cargo.toml** (if not using default example discovery):
   ```toml
   [[example]]
   name = "my_example"
   path = "examples/my_example.rs"
   ```

2. **Include module doc comment** explaining what the example demonstrates:
   ```rust
   //! My Example
   //!
   //! Demonstrates XYZ feature of Quiver.
   //!
   //! Run with: cargo run --example my_example
   ```

3. **Use clear comments** to explain key concepts

4. **Keep examples focused** - demonstrate one concept at a time

5. **Test the example** runs successfully:
   ```bash
   cargo run --example my_example
   ```

## Notes

- Examples require the `std` feature (default)
- Some examples may produce audio output (check console)
- Examples are tested in CI to ensure they compile and run
