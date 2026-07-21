/**
 * AudioWorkletProcessor for Quiver.
 *
 * Runs in the audio render thread and processes audio with the WASM engine. Built
 * (by tsup) into a single self-contained `dist/worklet.js` with the wasm-bindgen
 * glue inlined, because AudioWorklet module scripts cannot import other modules or
 * fetch at runtime. The `.wasm` *bytes* are fetched on the main thread and handed to
 * this processor in the `init` message, then loaded synchronously via `initSync`.
 *
 * ## Realtime model (Q095)
 *
 * `onmessage` and `process()` run on the same thread, so a single control message is
 * applied fully between two render quanta — `process()` never sees a half-applied
 * mutation. Compilation is lazy: structural edits mark the graph dirty and the engine
 * recompiles on the next `process()` (outputting silence, never crashing, if an
 * intermediate graph fails to compile). For atomic multi-step changes use
 * `load_patch`, which swaps the whole patch in one message.
 */

// MUST come first: AudioWorkletGlobalScope has no TextDecoder/TextEncoder, and the
// glue below constructs both at module top level. ESM evaluates imports in order.
import './worklet-polyfill';
// The wasm-bindgen glue. Bundled into this file by tsup (kept in the bundle, not
// externalized) so the worklet is a single importless module script. We use the
// NAMED synchronous `initSync` (not the default async init) so the engine is ready
// the moment the `init` message is handled — no fetch/await inside the worklet.
import { initSync, QuiverEngine } from '../quiver';

// AudioWorklet globals (only present in AudioWorkletGlobalScope).
declare class AudioWorkletProcessor {
  port: MessagePort;
  constructor();
  process(
    inputs: Float32Array[][],
    outputs: Float32Array[][],
    parameters: Record<string, Float32Array>
  ): boolean;
}
declare function registerProcessor(
  name: string,
  processorCtor: new () => AudioWorkletProcessor
): void;
declare const sampleRate: number;

interface InitMessage {
  type: 'init';
  wasmBytes: ArrayBuffer;
  sampleRate?: number;
}
type WorkletMessage =
  | InitMessage
  | { type: 'load_patch'; patch: unknown }
  | { type: 'save_patch'; name: string }
  | { type: 'set_param'; nodeId: string; paramIndex: number; value: number }
  | { type: 'add_module'; typeId: string; name: string }
  | { type: 'remove_module'; name: string }
  | { type: 'connect'; from: string; to: string; attenuation?: number; offset?: number }
  | { type: 'disconnect'; from: string; to: string }
  | { type: 'set_output'; name: string }
  | { type: 'add_midi_inputs' }
  | { type: 'midi_note_on'; note: number; velocity: number }
  | { type: 'midi_note_off'; note: number; velocity: number }
  | { type: 'midi_cc'; cc: number; value: number }
  | { type: 'midi_pitch_bend'; value: number }
  | { type: 'compile' }
  | { type: 'reset' }
  | { type: 'destroy' };

// Every control message from the main thread (except the untracked `init`) carries a
// monotonic `requestId` that we echo on the matching ack/error, so the caller can
// correlate each response to the exact request that issued it.
type IncomingMessage = WorkletMessage & { requestId?: number };

type EngineInstance = InstanceType<typeof QuiverEngine>;

class QuiverProcessor extends AudioWorkletProcessor {
  private engine: EngineInstance | null = null;
  private ready = false;
  private destroyed = false;
  private pending: IncomingMessage[] = [];

  constructor() {
    super();
    this.port.onmessage = (event: MessageEvent<IncomingMessage>) => {
      const message = event.data;
      if (message.type === 'init') {
        this.handleInit(message);
      } else if (message.type === 'destroy') {
        this.handleDestroy();
      } else if (this.ready && this.engine) {
        this.handleMessage(message);
      } else {
        this.pending.push(message);
      }
    };
  }

  private handleInit(message: InitMessage): void {
    try {
      // Synchronous init from the bytes handed over by the main thread.
      initSync({ module: message.wasmBytes });
      this.engine = new QuiverEngine(message.sampleRate ?? sampleRate);
      this.ready = true;
      for (const msg of this.pending) this.handleMessage(msg);
      this.pending = [];
      this.port.postMessage({ type: 'ready' });
    } catch (e) {
      this.port.postMessage({ type: 'error', error: String(e) });
    }
  }

  private handleDestroy(): void {
    if (this.engine) {
      // Guard against double-free / use-after-free.
      const engine = this.engine;
      this.engine = null;
      this.ready = false;
      try {
        engine.free();
      } catch {
        // Already freed — ignore.
      }
    }
    this.destroyed = true;
  }

  private handleMessage(message: IncomingMessage): void {
    const engine = this.engine;
    if (!engine) return;
    // Echoed back on the ack/error so the main thread can correlate the response.
    const requestId = message.requestId;
    try {
      switch (message.type) {
        case 'load_patch':
          // Atomic whole-patch swap.
          engine.load_patch(message.patch);
          engine.compile();
          this.port.postMessage({ type: 'patch_loaded', requestId });
          break;
        case 'save_patch':
          this.port.postMessage({
            type: 'patch_saved',
            patch: engine.save_patch(message.name),
            requestId,
          });
          break;
        case 'set_param':
          engine.set_param(message.nodeId, message.paramIndex, message.value);
          break;
        case 'add_module':
          engine.add_module(message.typeId, message.name);
          break;
        case 'remove_module':
          engine.remove_module(message.name);
          break;
        case 'connect':
          if (message.attenuation !== undefined && message.offset !== undefined) {
            engine.connect_modulated(
              message.from,
              message.to,
              message.attenuation,
              message.offset
            );
          } else if (message.attenuation !== undefined) {
            engine.connect_attenuated(message.from, message.to, message.attenuation);
          } else {
            engine.connect(message.from, message.to);
          }
          break;
        case 'disconnect':
          engine.disconnect(message.from, message.to);
          break;
        case 'set_output':
          engine.set_output(message.name);
          break;
        case 'add_midi_inputs':
          engine.add_midi_inputs();
          break;
        case 'midi_note_on':
          engine.midi_note_on(message.note, message.velocity);
          break;
        case 'midi_note_off':
          engine.midi_note_off(message.note, message.velocity);
          break;
        case 'midi_cc':
          engine.midi_cc(message.cc, message.value);
          break;
        case 'midi_pitch_bend':
          engine.midi_pitch_bend(message.value);
          break;
        case 'compile':
          // Lazy recompile happens automatically on the next process(); this is an
          // explicit trigger for callers that want to surface compile errors now.
          engine.compile();
          this.port.postMessage({ type: 'compiled', requestId });
          break;
        case 'reset':
          engine.reset();
          break;
      }
    } catch (e) {
      // Correlate the error to its request so it rejects only the matching awaited
      // promise (or, for fire-and-forget ops, none).
      this.port.postMessage({ type: 'error', error: String(e), requestId });
    }
  }

  process(
    _inputs: Float32Array[][],
    outputs: Float32Array[][],
    _parameters: Record<string, Float32Array>
  ): boolean {
    // Returning false lets the browser stop calling us and collect the node.
    if (this.destroyed) return false;

    const output = outputs[0];
    if (!this.ready || !this.engine || !output || output.length < 2) {
      return true; // Silence while not ready.
    }

    const left = output[0];
    const right = output[1];
    const numSamples = left.length;

    try {
      // View into WASM memory — read it immediately, before any other engine call.
      const stereo = this.engine.process_block(numSamples);
      for (let i = 0; i < numSamples; i++) {
        left[i] = stereo[i * 2];
        right[i] = stereo[i * 2 + 1];
      }
    } catch (e) {
      left.fill(0);
      right.fill(0);
      // eslint-disable-next-line no-console
      console.error('Quiver processing error:', e);
    }

    return true;
  }
}

registerProcessor('quiver-processor', QuiverProcessor);
