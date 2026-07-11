/**
 * Audio context utilities for Quiver WASM.
 *
 * These helpers set up real-time audio in the browser using an AudioWorklet. The
 * Quiver engine runs *inside* the worklet's render thread; every control call here
 * (`loadPatch`, `setParam`, `connect`, ...) posts a message to that engine. This is
 * the single supported real-time path — do not also create a main-thread engine and
 * expect the worklet to play it.
 *
 * ## Realtime caveat (honest)
 *
 * The worklet applies structural edits (add/remove module, connect, set output)
 * between render quanta: its `onmessage` handler and `process()` run on the same
 * thread, so `process()` never observes a half-applied *single* message. Compilation
 * is lazy — the engine recompiles on the next block when the graph is dirty, and
 * outputs silence (never a crash) if an intermediate graph fails to compile. For an
 * atomic multi-step change, prefer {@link QuiverAudioNode.loadPatch}, which swaps the
 * whole patch in one message.
 */

/**
 * Options for creating a Quiver audio node.
 */
export interface QuiverAudioNodeOptions {
  /** URL to the compiled worklet script (the package's `dist/worklet.js`). */
  workletUrl: string | URL;
  /** URL to the `quiver_bg.wasm` binary. Fetched on the main thread and handed to
   * the worklet (worklets cannot fetch/import modules themselves). */
  wasmUrl: string | URL;
  /** Output channel count (default: 2 for stereo). */
  outputChannels?: number;
}

/**
 * Interface for the Quiver AudioWorklet node.
 *
 * All mutating methods are fire-and-forget message posts to the worklet engine;
 * `loadPatch` / `compile` / `savePatch` return promises that resolve when the
 * worklet acknowledges.
 */
export interface QuiverAudioNode {
  /** The underlying AudioWorkletNode (connect this to your graph). */
  node: AudioWorkletNode;
  /** The AudioContext. */
  context: AudioContext;
  /** Load a patch into the worklet engine (atomic swap). */
  loadPatch: (patch: unknown) => Promise<void>;
  /** Save the worklet engine's current patch as a PatchDef object. */
  savePatch: (name: string) => Promise<unknown>;
  /** Set a parameter value by numeric index. */
  setParam: (nodeId: string, paramIndex: number, value: number) => void;
  /** Add a module to the patch. */
  addModule: (typeId: string, name: string) => void;
  /** Remove a module from the patch. */
  removeModule: (name: string) => void;
  /** Connect two ports ("module.port"). */
  connect: (from: string, to: string, attenuation?: number, offset?: number) => void;
  /** Disconnect two ports ("module.port"). */
  disconnect: (from: string, to: string) => void;
  /** Set the patch's output module (its port 0/1 become L/R). */
  setOutput: (name: string) => void;
  /** Inject the engine-owned MIDI CV source modules (midi_voct, midi_gate, ...). */
  addMidiInputs: () => void;
  /** MIDI note on (drives the shared midi_* CV sources). */
  midiNoteOn: (note: number, velocity: number) => void;
  /** MIDI note off. */
  midiNoteOff: (note: number, velocity: number) => void;
  /** MIDI control change (CC1 drives midi_mod). */
  midiCc: (cc: number, value: number) => void;
  /** MIDI pitch bend (-1..1, drives midi_bend). */
  midiPitchBend: (value: number) => void;
  /** Recompile the patch (usually unnecessary — compilation is lazy). */
  compile: () => Promise<void>;
  /** Reset all module state. */
  reset: () => void;
  /** Free the worklet engine and stop the processor, then disconnect the node. */
  dispose: () => void;
}

/**
 * Create a Quiver AudioWorklet node.
 *
 * @example
 * ```typescript
 * import { createQuiverAudioNode } from '@quiver/wasm';
 * import workletUrl from '@quiver/wasm/dist/worklet.js?url';
 * import wasmUrl from '@quiver/wasm/quiver_bg.wasm?url';
 *
 * const ctx = new AudioContext();
 * const quiver = await createQuiverAudioNode(ctx, { workletUrl, wasmUrl });
 * await quiver.loadPatch(myPatch);
 * quiver.node.connect(ctx.destination);
 * ```
 */
export async function createQuiverAudioNode(
  audioContext: AudioContext,
  options: QuiverAudioNodeOptions
): Promise<QuiverAudioNode> {
  const { workletUrl, wasmUrl, outputChannels = 2 } = options;

  // Fetch the wasm binary on the main thread — the worklet cannot fetch it.
  const wasmBytes = await fetch(String(wasmUrl)).then((r) => r.arrayBuffer());

  // Load the worklet module (self-contained bundle including the wasm-bindgen glue).
  await audioContext.audioWorklet.addModule(String(workletUrl));

  const node = new AudioWorkletNode(audioContext, 'quiver-processor', {
    numberOfInputs: 0,
    numberOfOutputs: 1,
    outputChannelCount: [outputChannels],
  });

  node.port.start();

  // Await the worklet's readiness (after it initSync's the wasm and creates the
  // engine). We transfer the ArrayBuffer to avoid a copy.
  await new Promise<void>((resolve, reject) => {
    const timeout = setTimeout(
      () => reject(new Error('Quiver worklet initialization timeout')),
      10000
    );
    const handler = (event: MessageEvent) => {
      if (event.data?.type === 'ready') {
        clearTimeout(timeout);
        node.port.removeEventListener('message', handler);
        resolve();
      } else if (event.data?.type === 'error') {
        clearTimeout(timeout);
        node.port.removeEventListener('message', handler);
        reject(new Error(event.data.error));
      }
    };
    node.port.addEventListener('message', handler);
    node.port.postMessage(
      { type: 'init', wasmBytes, sampleRate: audioContext.sampleRate },
      [wasmBytes]
    );
  });

  // Await a specific acknowledgement message type once.
  const awaitResponse = <T = void>(
    trigger: () => void,
    okType: string,
    extract?: (data: Record<string, unknown>) => T
  ): Promise<T> =>
    new Promise<T>((resolve, reject) => {
      const handler = (event: MessageEvent) => {
        if (event.data?.type === okType) {
          node.port.removeEventListener('message', handler);
          resolve(extract ? extract(event.data) : (undefined as T));
        } else if (event.data?.type === 'error') {
          node.port.removeEventListener('message', handler);
          reject(new Error(event.data.error));
        }
      };
      node.port.addEventListener('message', handler);
      trigger();
    });

  return {
    node,
    context: audioContext,
    loadPatch: (patch) =>
      awaitResponse(
        () => node.port.postMessage({ type: 'load_patch', patch }),
        'patch_loaded'
      ),
    savePatch: (name) =>
      awaitResponse<unknown>(
        () => node.port.postMessage({ type: 'save_patch', name }),
        'patch_saved',
        (data) => data.patch
      ),
    setParam: (nodeId, paramIndex, value) =>
      node.port.postMessage({ type: 'set_param', nodeId, paramIndex, value }),
    addModule: (typeId, name) =>
      node.port.postMessage({ type: 'add_module', typeId, name }),
    removeModule: (name) => node.port.postMessage({ type: 'remove_module', name }),
    connect: (from, to, attenuation, offset) =>
      node.port.postMessage({ type: 'connect', from, to, attenuation, offset }),
    disconnect: (from, to) => node.port.postMessage({ type: 'disconnect', from, to }),
    setOutput: (name) => node.port.postMessage({ type: 'set_output', name }),
    addMidiInputs: () => node.port.postMessage({ type: 'add_midi_inputs' }),
    midiNoteOn: (note, velocity) =>
      node.port.postMessage({ type: 'midi_note_on', note, velocity }),
    midiNoteOff: (note, velocity) =>
      node.port.postMessage({ type: 'midi_note_off', note, velocity }),
    midiCc: (cc, value) => node.port.postMessage({ type: 'midi_cc', cc, value }),
    midiPitchBend: (value) => node.port.postMessage({ type: 'midi_pitch_bend', value }),
    compile: () =>
      awaitResponse(() => node.port.postMessage({ type: 'compile' }), 'compiled'),
    reset: () => node.port.postMessage({ type: 'reset' }),
    dispose: () => {
      // Tell the worklet to free its engine and stop; then detach the node.
      node.port.postMessage({ type: 'destroy' });
      node.disconnect();
    },
  };
}

/**
 * Create an AudioContext with a Quiver node already connected to the destination.
 *
 * @example
 * ```typescript
 * const quiver = await createQuiverAudio({ workletUrl, wasmUrl });
 * await quiver.loadPatch(myPatch);
 * // Audio is now playing through the speakers.
 * ```
 */
export async function createQuiverAudio(
  options: QuiverAudioNodeOptions
): Promise<QuiverAudioNode> {
  const audioContext = new AudioContext();
  const quiver = await createQuiverAudioNode(audioContext, options);
  quiver.node.connect(audioContext.destination);
  return quiver;
}
