import * as ___quiver from '../quiver';
export { InitInput, InitOutput, QuiverEngine, QuiverError, SyncInitInput } from '../quiver';
export { QuiverAudioNode, QuiverAudioNodeOptions, createQuiverAudio, createQuiverAudioNode } from './audio.js';

/**
 * Initialize the WASM module.
 *
 * Must be called (and awaited) before constructing a {@link QuiverEngine} on the
 * main thread. Safe to call repeatedly — subsequent calls return the same promise.
 *
 * @example
 * ```typescript
 * import { initWasm, QuiverEngine } from '@quiver-dsp/wasm';
 *
 * await initWasm();
 * const engine = new QuiverEngine(44100);
 * ```
 */
declare function initWasm(): Promise<void>;
/**
 * Create a new {@link QuiverEngine}, ensuring WASM is initialized first.
 *
 * This is a **main-thread, non-audio** engine (catalog, validation, offline
 * rendering, tests). For playback use {@link createQuiverAudio}.
 *
 * @param sampleRate Audio sample rate (e.g. 44100, 48000).
 */
declare function createEngine(sampleRate: number): Promise<___quiver.QuiverEngine>;

export { createEngine, initWasm };
