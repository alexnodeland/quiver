# WASM Stack Audit Report

**Date:** 2024-12-24
**Scope:** Full audit of the WASM stack from Rust library to browser demo
**Status:** Gaps Identified - Action Required

---

## Executive Summary

The Quiver WASM stack provides a functional foundation for browser-based modular synthesis, but several architectural and implementation gaps prevent it from being production-ready. The most critical issues are:

1. **Deprecated Audio API** - Uses `ScriptProcessorNode` instead of `AudioWorklet`
2. **Type System Fragmentation** - Manual TypeScript types can drift from Rust-generated types
3. **Incomplete Package Dependencies** - Missing inter-package dependencies
4. **Unfinished MIDI Integration** - MIDI values stored but not routed to audio

This document catalogs all identified gaps with prioritized solutions.

---

## Table of Contents

1. [Rust WASM Bindings](#1-rust-wasm-bindings-srcwasm)
2. [TypeScript Packages](#2-typescript-packages-packagesquiver)
3. [Browser Demo](#3-browser-demo-demosbrowser)
4. [Cross-Layer Integration](#4-cross-layer-integration)
5. [Process & Workflow](#5-process--workflow)
6. [Prioritized Action Plan](#6-prioritized-action-plan)

---

## 1. Rust WASM Bindings (`src/wasm/`)

### 1.1 QuiverError Type Not Used

**Location:** `src/wasm/error.rs:8-42`, `src/wasm/engine.rs`

**Gap:** The `QuiverError` struct is defined with proper WASM bindings but never used. All error handling in `engine.rs` uses `JsValue::from_str()` instead.

**Current Code:**
```rust
// error.rs - Defined but unused
#[wasm_bindgen]
pub struct QuiverError {
    message: String,
}

// engine.rs - Uses string errors instead
pub fn add_module(&mut self, type_id: &str, name: &str) -> Result<(), JsValue> {
    // ...
    .ok_or_else(|| JsValue::from_str(&format!("Unknown module type: {}", type_id)))?;
}
```

**Impact:**
- No structured error types for JavaScript consumers
- Harder to programmatically handle different error types
- Inconsistent with Rust error handling patterns

**Solution:**
```rust
// Use QuiverError consistently
impl From<QuiverError> for JsValue {
    fn from(e: QuiverError) -> Self {
        JsValue::from_str(&e.message)
    }
}

pub fn add_module(&mut self, type_id: &str, name: &str) -> Result<(), QuiverError> {
    self.registry
        .instantiate(type_id, self.sample_rate)
        .ok_or_else(|| QuiverError::from(format!("Unknown module type: {}", type_id)))?;
    // ...
}
```

**Effort:** Low (2-4 hours)

---

### 1.2 MIDI Values Not Routed to Patch

**Location:** `src/wasm/engine.rs:506-567`

**Gap:** MIDI note, velocity, gate, CC, and pitch bend values are stored in the `QuiverEngine` struct but never connected to the audio patch. Users must manually read these values and set parameters.

**Current Code:**
```rust
pub fn midi_note_on(&mut self, note: u8, velocity: u8) -> Result<(), JsValue> {
    let v_oct = (note as f64 - 60.0) / 12.0;
    let vel = velocity as f64 / 127.0;

    // Values stored but not connected to patch
    self.midi_note = Some(v_oct);
    self.midi_velocity = Some(vel);
    self.midi_gate = true;
    Ok(())
}
```

**Impact:**
- Users must poll MIDI values and manually set parameters
- No automatic MIDI-to-CV conversion
- Breaks expected modular synth workflow

**Solution:**
```rust
// Option A: Use ExternalInput modules
pub fn midi_note_on(&mut self, note: u8, velocity: u8) -> Result<(), JsValue> {
    let v_oct = (note as f64 - 60.0) / 12.0;
    let vel = velocity as f64 / 127.0;

    // Store values
    self.midi_note = Some(v_oct);
    self.midi_velocity = Some(vel);
    self.midi_gate = true;

    // Route to external inputs if configured
    if let Some(note_input) = &self.midi_note_input {
        note_input.set(v_oct);
    }
    if let Some(vel_input) = &self.midi_velocity_input {
        vel_input.set(vel);
    }
    if let Some(gate_input) = &self.midi_gate_input {
        gate_input.set(5.0); // Gate high
    }
    Ok(())
}

// Option B: Add dedicated MIDI input module
pub fn create_midi_input(&mut self, name: &str) -> Result<(), JsValue> {
    // Creates a module with voct, velocity, gate outputs
    // Automatically updated by midi_note_on/off
}
```

**Effort:** Medium (1-2 days)

---

### 1.3 Single-Sample Observer Collection

**Location:** `src/wasm/engine.rs:481`

**Gap:** `collect_from_patch` is called after `process_block` but only captures the final sample value, not true block-based level metering.

**Current Code:**
```rust
pub fn process_block(&mut self, num_samples: usize) -> js_sys::Float32Array {
    for i in 0..num_samples {
        let (left, right) = self.patch.tick();
        // ... output handling
    }

    // Only collects last sample state
    self.observer.collect_from_patch(&self.patch);
    output
}
```

**Impact:**
- Level meters don't reflect true RMS/peak over the block
- Scope displays may miss transients
- Inaccurate visualization data

**Solution:**
```rust
pub fn process_block(&mut self, num_samples: usize) -> js_sys::Float32Array {
    // Collect samples for observer during processing
    let mut observer_samples: Vec<(f32, f32)> = Vec::with_capacity(num_samples);

    for i in 0..num_samples {
        let (left, right) = self.patch.tick();
        observer_samples.push((left as f32, right as f32));
        // ... output handling
    }

    // Pass full block to observer
    self.observer.collect_block_from_patch(&self.patch, &observer_samples);
    output
}
```

**Effort:** Medium (4-8 hours)

---

## 2. TypeScript Packages (`packages/@quiver/`)

### 2.1 AudioWorklet Not Functional

**Location:** `packages/@quiver/wasm/src/worklet.ts:153`

**Gap:** The worklet processor attempts to import WASM via ES modules, which isn't supported in AudioWorklet scope in most browsers.

**Current Code:**
```typescript
private async handleInit(message: InitMessage): Promise<void> {
    // This import won't work in AudioWorklet scope
    const wasmModule = await import('./quiver.js' as any);
    await wasmModule.default();
    // ...
}
```

**Impact:**
- AudioWorklet-based audio processing doesn't work
- Forces use of deprecated ScriptProcessorNode
- Main thread audio processing causes glitches

**Solution:**

**Option A: Use Comlink for Worklet Communication**
```typescript
// worklet-loader.ts
import * as Comlink from 'comlink';

export async function createQuiverWorklet(audioContext: AudioContext) {
    // Load WASM in main thread
    await initWasm();
    const engine = new QuiverEngine(audioContext.sampleRate);

    // Use SharedArrayBuffer for audio data
    const inputBuffer = new SharedArrayBuffer(512 * 4 * 2);
    const outputBuffer = new SharedArrayBuffer(512 * 4 * 2);

    await audioContext.audioWorklet.addModule('/quiver-worklet.js');
    const node = new AudioWorkletNode(audioContext, 'quiver-processor');

    // Send buffer references to worklet
    node.port.postMessage({ inputBuffer, outputBuffer });

    return { node, engine };
}
```

**Option B: Inline WASM in Worklet**
```typescript
// Build step: encode WASM as base64
// worklet.ts
const wasmBase64 = '...'; // Injected at build time

class QuiverProcessor extends AudioWorkletProcessor {
    async init() {
        const wasmBytes = Uint8Array.from(atob(wasmBase64), c => c.charCodeAt(0));
        const wasmModule = await WebAssembly.instantiate(wasmBytes, imports);
        // ...
    }
}
```

**Option C: Use wasm-bindgen-rayon for Worklet Support**
```toml
# Cargo.toml
[dependencies]
wasm-bindgen-rayon = "1.0"
```

**Effort:** High (3-5 days)

---

### 2.2 Type Drift Between Rust and TypeScript

**Location:** `packages/@quiver/types/src/index.ts`

**Gap:** TypeScript types are manually maintained separately from Rust's tsify-generated types. No mechanism ensures they stay synchronized.

**Current State:**
```
Rust (src/observer.rs)          TypeScript (@quiver/types)
─────────────────────           ─────────────────────────
ObservableValue (tsify)    →    ObservableValue (manual)
SubscriptionTarget (tsify) →    SubscriptionTarget (manual)
```

**Impact:**
- Types can silently diverge
- Runtime errors when types don't match
- Maintenance burden

**Solution:**

**Option A: Use Generated Types Only**
```typescript
// packages/@quiver/types/src/index.ts
// Re-export from generated WASM types
export type {
    ObservableValue,
    SubscriptionTarget,
    PatchDef,
    ModuleDef,
    // ...
} from '@quiver/wasm/quiver';

// Add utility functions that work with these types
export function validatePatchDef(patch: PatchDef): ValidationResult {
    // ...
}
```

**Option B: Generate Types at Build Time**
```json
// package.json
{
    "scripts": {
        "generate-types": "ts-node scripts/extract-types.ts",
        "build": "npm run generate-types && tsup ..."
    }
}
```

**Option C: Runtime Type Validation**
```typescript
import { z } from 'zod';

export const PatchDefSchema = z.object({
    version: z.number(),
    name: z.string(),
    modules: z.array(ModuleDefSchema),
    cables: z.array(CableDefSchema),
    parameters: z.record(z.number()),
});

export type PatchDef = z.infer<typeof PatchDefSchema>;
```

**Effort:** Medium (1-2 days)

---

### 2.3 Incomplete ModuleTypeId Union

**Location:** `packages/@quiver/types/src/index.ts:461-509`

**Gap:** The `ModuleTypeId` type is missing many modules that exist in the Rust registry.

**Missing Modules:**
- Oscillators: `supersaw`, `wavetable`, `formant_osc`, `karplus_strong`
- Effects: `chorus`, `flanger`, `phaser`, `tremolo`, `vibrato`, `distortion`, `bitcrusher`, `reverb`, `pitch_shifter`, `vocoder`, `granular`
- Delays: `delay_line`
- Sequencing: `arpeggiator`, `euclidean`, `chord_memory`
- Dynamics: `compressor`, `limiter`, `noise_gate`, `envelope_follower`
- EQ: `parametric_eq`

**Solution:**
```typescript
export type ModuleTypeId =
    // Oscillators
    | 'vco'
    | 'analog_vco'
    | 'lfo'
    | 'supersaw'
    | 'wavetable'
    | 'formant_osc'
    | 'karplus_strong'
    | 'noise'
    // Filters
    | 'svf'
    | 'diode_ladder'
    // Envelopes
    | 'adsr'
    | 'envelope_follower'
    // ... (complete list)
    ;

// Or generate from Rust
// scripts/generate-module-types.ts
```

**Effort:** Low (1-2 hours)

---

### 2.4 Missing Package Dependencies

**Location:** `packages/@quiver/react/package.json`

**Gap:** `@quiver/react` dynamically imports `@quiver/wasm` but doesn't declare it as a dependency.

**Current Code:**
```typescript
// hooks.ts:356
const { createEngine } = await import('@quiver/wasm');
```

```json
// package.json - Missing @quiver/wasm
{
    "dependencies": {
        "@quiver/types": "workspace:*"
    },
    "peerDependencies": {
        "@xyflow/react": ">=12.0.0",
        "react": ">=18.0.0"
    }
}
```

**Solution:**
```json
{
    "dependencies": {
        "@quiver/types": "workspace:*"
    },
    "peerDependencies": {
        "@quiver/wasm": ">=0.1.0",
        "@xyflow/react": ">=12.0.0",
        "react": ">=18.0.0"
    }
}
```

**Effort:** Trivial (5 minutes)

---

### 2.5 QuiverEngine Interface Mismatch

**Location:** `packages/@quiver/react/src/hooks.ts:26-85`

**Gap:** The `QuiverEngine` TypeScript interface doesn't accurately reflect the WASM API.

**Examples:**
```typescript
// Interface says:
get_catalog(): CatalogResponse;

// WASM actually returns:
get_catalog(): Result<JsValue, JsValue>; // Needs JSON parsing
```

**Solution:**
```typescript
// Accurate interface
export interface QuiverEngine {
    // Methods that return JsValue need parsing
    get_catalog(): unknown; // Returns JsValue, parse with serde_wasm_bindgen

    // Or wrap in utility
    getCatalog(): CatalogResponse {
        return this._engine.get_catalog() as CatalogResponse;
    }
}
```

**Effort:** Low (2-4 hours)

---

## 3. Browser Demo (`demos/browser/`)

### 3.1 Uses Deprecated ScriptProcessorNode

**Location:** `demos/browser/src/main.ts:213-262`

**Gap:** Uses `createScriptProcessor()` which is deprecated, runs on the main thread, and causes audio glitches.

**Current Code:**
```typescript
function createScriptProcessor(ctx: AudioContext): ScriptProcessorNode {
    const processor = ctx.createScriptProcessor(512, 0, 2);
    processor.onaudioprocess = (e) => {
        // Runs on main thread - blocks UI
    };
    return processor;
}
```

**Impact:**
- Audio glitches during UI activity
- Higher latency
- Will eventually be removed from browsers

**Solution:**
```typescript
// Create proper AudioWorklet
async function createAudioProcessor(ctx: AudioContext): Promise<AudioWorkletNode> {
    await ctx.audioWorklet.addModule('/quiver-processor.js');

    const node = new AudioWorkletNode(ctx, 'quiver-processor', {
        numberOfInputs: 0,
        numberOfOutputs: 1,
        outputChannelCount: [2],
    });

    // Initialize WASM in worklet
    node.port.postMessage({ type: 'init', sampleRate: ctx.sampleRate });

    return node;
}
```

**Effort:** High (depends on 2.1 AudioWorklet solution)

---

### 3.2 Direct Path Import Instead of Package

**Location:** `demos/browser/src/main.ts:3`

**Gap:** Imports WASM via relative path instead of npm package.

**Current Code:**
```typescript
import init, { QuiverEngine } from '../../../packages/@quiver/wasm/quiver.js';
```

**Impact:**
- Fragile path that breaks if directory structure changes
- Can't test npm package distribution
- Different from how users would consume the package

**Solution:**
```typescript
// Use package import
import { initWasm, QuiverEngine } from '@quiver/wasm';

// vite.config.ts
export default defineConfig({
    resolve: {
        alias: {
            '@quiver/wasm': path.resolve(__dirname, '../../packages/@quiver/wasm'),
        },
    },
});
```

**Effort:** Low (1-2 hours)

---

### 3.3 Filter Envelope Not Implemented

**Location:** `demos/browser/src/main.ts:739-741`

**Gap:** Filter envelope amount control has a TODO comment and isn't functional.

**Current Code:**
```typescript
bindSlider('filterEnv', () => {
    // TODO: Filter envelope amount needs VCA module
});
```

**Solution:**
```typescript
// Add envelope-to-filter modulation in patch creation
for (let i = 0; i < NUM_VOICES; i++) {
    addModule('vca', `filter_env_amt_${i}`);
    connect(`env_${i}.env`, `filter_env_amt_${i}.in`);
    connect(`filter_env_amt_${i}.out`, `filter_${i}.cutoff`);
}

// Then in control binding
bindSlider('filterEnv', (amount) => {
    for (let i = 0; i < NUM_VOICES; i++) {
        engine.set_param(`filter_env_amt_${i}`, 0, amount);
    }
});
```

**Effort:** Low (1-2 hours)

---

### 3.4 No Audio Error Recovery

**Location:** `demos/browser/src/main.ts:256-258`

**Gap:** Audio processing errors are caught and logged but there's no recovery mechanism.

**Current Code:**
```typescript
} catch (error) {
    console.error('Audio processing error:', error);
}
```

**Impact:**
- Audio stops permanently on any error
- User must reload page
- No feedback to user

**Solution:**
```typescript
let consecutiveErrors = 0;
const MAX_CONSECUTIVE_ERRORS = 10;

processor.onaudioprocess = (e) => {
    try {
        // ... processing
        consecutiveErrors = 0;
    } catch (error) {
        console.error('Audio processing error:', error);
        consecutiveErrors++;

        // Output silence
        e.outputBuffer.getChannelData(0).fill(0);
        e.outputBuffer.getChannelData(1).fill(0);

        if (consecutiveErrors >= MAX_CONSECUTIVE_ERRORS) {
            showErrorToast('Audio engine error. Click to restart.');
            isRunning = false;
        }
    }
};

function restartAudio() {
    engine?.reset();
    engine?.compile();
    consecutiveErrors = 0;
    isRunning = true;
}
```

**Effort:** Low (2-4 hours)

---

## 4. Cross-Layer Integration

### 4.1 No Workspace Tooling

**Gap:** Each package is independent with no monorepo tooling.

**Current State:**
```
packages/
├── @quiver/wasm/     # Independent npm project
├── @quiver/types/    # Independent npm project
└── @quiver/react/    # Independent npm project
```

**Impact:**
- No coordinated builds
- No dependency graph optimization
- Manual version management

**Solution:**
```json
// pnpm-workspace.yaml
packages:
  - 'packages/@quiver/*'
  - 'demos/*'

// package.json (root)
{
    "private": true,
    "workspaces": ["packages/@quiver/*", "demos/*"],
    "scripts": {
        "build": "turbo run build",
        "test": "turbo run test",
        "lint": "turbo run lint"
    },
    "devDependencies": {
        "turbo": "^2.0.0"
    }
}

// turbo.json
{
    "pipeline": {
        "build": {
            "dependsOn": ["^build"],
            "outputs": ["dist/**"]
        }
    }
}
```

**Effort:** Medium (4-8 hours)

---

### 4.2 SharedArrayBuffer CORS Headers Missing

**Gap:** AudioWorklet with SharedArrayBuffer requires specific CORS headers not configured.

**Required Headers:**
```
Cross-Origin-Opener-Policy: same-origin
Cross-Origin-Embedder-Policy: require-corp
```

**Solution:**
```typescript
// vite.config.ts
export default defineConfig({
    server: {
        headers: {
            'Cross-Origin-Opener-Policy': 'same-origin',
            'Cross-Origin-Embedder-Policy': 'require-corp',
        },
    },
});
```

**Effort:** Trivial (5 minutes, but requires architecture changes to use SharedArrayBuffer)

---

### 4.3 No TypeScript in CI Pipeline

**Location:** `.github/workflows/ci.yml`

**Gap:** TypeScript packages aren't built or type-checked in CI.

**Solution:**
```yaml
# .github/workflows/ci.yml
jobs:
  typescript:
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@v4
      - uses: pnpm/action-setup@v2
      - uses: actions/setup-node@v4
        with:
          node-version: '20'
          cache: 'pnpm'
      - run: pnpm install
      - run: pnpm run build
      - run: pnpm run lint
      - run: pnpm run test
```

**Effort:** Low (1-2 hours)

---

## 5. Process & Workflow

### 5.1 No Integration Tests

**Gap:** Browser tests test WASM in isolation but don't test the full npm package consumption flow.

**Solution:**
```typescript
// tests/integration/npm-consumption.spec.ts
import { test, expect } from '@playwright/test';

test('can consume @quiver/wasm from npm', async ({ page }) => {
    // Test app that imports from npm packages
    await page.goto('/integration-test-app');

    // Verify WASM loads
    await expect(page.locator('#status')).toHaveText('Ready');

    // Verify engine works
    await page.click('#create-patch');
    await expect(page.locator('#module-count')).toHaveText('1');
});
```

**Effort:** Medium (1 day)

---

### 5.2 No Versioning Strategy

**Gap:** All packages are at 0.1.0 with no synchronization mechanism.

**Solution:**
```json
// Use changesets for coordinated versioning
// .changeset/config.json
{
    "changelog": "@changesets/cli/changelog",
    "commit": false,
    "fixed": [["@quiver/wasm", "@quiver/types", "@quiver/react"]],
    "linked": [],
    "access": "public",
    "baseBranch": "main"
}
```

```bash
# Usage
pnpm changeset        # Create changeset for changes
pnpm changeset version  # Bump versions
pnpm changeset publish  # Publish to npm
```

**Effort:** Low (2-4 hours)

---

### 5.3 Missing Package Documentation

**Gap:** No documentation on how to use the npm packages together.

**Solution:** Create `packages/README.md`:
```markdown
# Quiver NPM Packages

## Quick Start

```bash
npm install @quiver/wasm @quiver/react
```

```tsx
import { useQuiverEngine, useQuiverPatch } from '@quiver/react';

function Synth() {
    const { engine, isReady } = useQuiverEngine(44100);
    const { loadPatch } = useQuiverPatch(engine);

    // ...
}
```
```

**Effort:** Low (2-4 hours)

---

## 6. Prioritized Action Plan

### Phase 1: Critical Fixes (Week 1) ✅ COMPLETED

| Priority | Task | Effort | Impact | Status |
|----------|------|--------|--------|--------|
| P0 | Fix @quiver/react missing @quiver/wasm dependency | 5 min | Blocks usage | ✅ Done |
| P0 | Add CORS headers for SharedArrayBuffer | 5 min | Enables Worklet | ✅ Done |
| P1 | Complete ModuleTypeId union type | 2 hrs | Type safety | ✅ Done |
| P1 | Fix QuiverEngine interface mismatch | 4 hrs | Type safety | ✅ Done |

### Phase 2: Architecture (Weeks 2-3) ✅ COMPLETED

| Priority | Task | Effort | Impact | Status |
|----------|------|--------|--------|--------|
| P1 | Implement proper AudioWorklet | 3-5 days | Audio quality | ✅ Done |
| P1 | Set up monorepo tooling (pnpm + turbo) | 4-8 hrs | DX | ✅ Done |
| P2 | Unify TypeScript types with generated | 1-2 days | Maintainability | ✅ Done |
| P2 | Add TypeScript to CI | 2 hrs | Quality | ✅ Done |

**Phase 2 Implementation Notes:**
- AudioWorklet: Implemented SharedArrayBuffer-based architecture in `worklet-processor.ts` and `AudioManager` class
- Monorepo: Added pnpm-workspace.yaml, turbo.json, and updated all package.json files
- CI: Added TypeScript typecheck job and migrated browser-tests to pnpm
- Types: Added comprehensive documentation linking TypeScript types to Rust sources, created type compatibility check script

### Phase 3: Features (Weeks 4-5) ✅ COMPLETED

| Priority | Task | Effort | Impact | Status |
|----------|------|--------|--------|--------|
| P2 | Complete MIDI-to-CV routing | 1-2 days | Functionality | ✅ Done |
| P2 | Use QuiverError consistently | 4 hrs | Error handling | ✅ Done |
| P2 | Add block-based observer collection | 4-8 hrs | Visualization | ✅ Done |
| P3 | Implement filter envelope in demo | 2 hrs | Demo quality | ✅ Done |

**Phase 3 Implementation Notes:**
- MIDI-to-CV: Added `create_midi_input()` and `create_midi_cc_input()` methods that create ExternalInput modules connected to Arc<AtomicF64> values. MIDI messages now automatically update these atomics, routing V/Oct, gate, velocity, pitch bend, and mod wheel to the patch.
- QuiverError: Added `into_js()` helper method for converting QuiverError to JsValue. The wasm_bindgen derive already provides automatic JsValue conversion.
- Block-based observer: Added `collect_block()` method to StateObserver that accumulates samples during block processing for accurate RMS/peak metering. process_block now collects all samples and passes them to the observer.
- Filter envelope: Added filter envelope VCA per voice and filter_env_amt offset module. Envelope now modulates filter cutoff via the SVF's fm input.

### Phase 4: Polish (Week 6)

| Priority | Task | Effort | Impact |
|----------|------|--------|--------|
| P3 | Add audio error recovery | 4 hrs | Reliability |
| P3 | Add integration tests | 1 day | Quality |
| P3 | Set up changesets versioning | 4 hrs | Process |
| P3 | Write package documentation | 4 hrs | Adoption |

---

## Appendix: File Reference

| File | Issues |
|------|--------|
| `src/wasm/engine.rs` | 1.1, 1.2, 1.3 |
| `src/wasm/error.rs` | 1.1 |
| `packages/@quiver/wasm/src/worklet.ts` | 2.1 |
| `packages/@quiver/wasm/src/index.ts` | 2.1 |
| `packages/@quiver/types/src/index.ts` | 2.2, 2.3 |
| `packages/@quiver/react/package.json` | 2.4 |
| `packages/@quiver/react/src/hooks.ts` | 2.4, 2.5 |
| `demos/browser/src/main.ts` | 3.1, 3.2, 3.3, 3.4 |
| `demos/browser/vite.config.ts` | 4.2 |
| `.github/workflows/ci.yml` | 4.3 |
