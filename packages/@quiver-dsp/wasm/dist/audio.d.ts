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
interface QuiverAudioNodeOptions {
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
interface QuiverAudioNode {
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
 * import { createQuiverAudioNode } from '@quiver-dsp/wasm';
 * import workletUrl from '@quiver-dsp/wasm/dist/worklet.js?url';
 * import wasmUrl from '@quiver-dsp/wasm/quiver_bg.wasm?url';
 *
 * const ctx = new AudioContext();
 * const quiver = await createQuiverAudioNode(ctx, { workletUrl, wasmUrl });
 * await quiver.loadPatch(myPatch);
 * quiver.node.connect(ctx.destination);
 * ```
 */
declare function createQuiverAudioNode(audioContext: AudioContext, options: QuiverAudioNodeOptions): Promise<QuiverAudioNode>;
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
declare function createQuiverAudio(options: QuiverAudioNodeOptions): Promise<QuiverAudioNode>;

export { type QuiverAudioNode, type QuiverAudioNodeOptions, createQuiverAudio, createQuiverAudioNode };
