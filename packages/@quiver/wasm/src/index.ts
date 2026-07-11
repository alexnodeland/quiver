/**
 * @quiver/wasm - WASM bindings for Quiver modular synthesizer
 *
 * This package provides the core WASM engine for running Quiver in a browser
 * environment, plus the AudioWorklet helpers that are the *only* supported way to
 * get real-time audio out of it.
 *
 * ## Two ways to use it
 *
 * 1. **Real-time audio (recommended):** {@link createQuiverAudio} /
 *    {@link createQuiverAudioNode}. The Quiver engine runs *inside* the
 *    AudioWorklet render thread; you drive it with `loadPatch`, `setParam`,
 *    `connect`, etc. which post messages to that engine. This is the single
 *    working audio path.
 *
 * 2. **Direct engine (no audio thread):** {@link createEngine} returns a
 *    {@link QuiverEngine} you call synchronously (catalog browsing, offline
 *    rendering, tests). Do **not** wire this instance up expecting the worklet to
 *    play it — the worklet owns its own engine.
 */

// Re-export the wasm-bindgen bindings: the QuiverEngine class value *and* its
// generated type, so consumers (and @quiver/react) get the real type instead of a
// hand-maintained duplicate. The glue lives at the package root (built by
// wasm-pack) and is kept external from this bundle.
export { QuiverEngine, QuiverError } from '../quiver';
export type { InitInput, InitOutput, SyncInitInput } from '../quiver';

// Re-export the AudioWorklet helpers — the canonical real-time audio API.
export {
  createQuiverAudioNode,
  createQuiverAudio,
  type QuiverAudioNode,
  type QuiverAudioNodeOptions,
} from './audio';

// Initialize the WASM module (idempotent).
let wasmInitPromise: Promise<unknown> | null = null;

/**
 * Initialize the WASM module.
 *
 * Must be called (and awaited) before constructing a {@link QuiverEngine} on the
 * main thread. Safe to call repeatedly — subsequent calls return the same promise.
 *
 * @example
 * ```typescript
 * import { initWasm, QuiverEngine } from '@quiver/wasm';
 *
 * await initWasm();
 * const engine = new QuiverEngine(44100);
 * ```
 */
export async function initWasm(): Promise<void> {
  if (!wasmInitPromise) {
    wasmInitPromise = import('../quiver').then((wasm) => wasm.default());
  }
  await wasmInitPromise;
}

/**
 * Create a new {@link QuiverEngine}, ensuring WASM is initialized first.
 *
 * This is a **main-thread, non-audio** engine (catalog, validation, offline
 * rendering, tests). For playback use {@link createQuiverAudio}.
 *
 * @param sampleRate Audio sample rate (e.g. 44100, 48000).
 */
export async function createEngine(
  sampleRate: number
): Promise<import('../quiver').QuiverEngine> {
  await initWasm();
  const { QuiverEngine } = await import('../quiver');
  return new QuiverEngine(sampleRate);
}
