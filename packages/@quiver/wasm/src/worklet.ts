/**
 * AudioWorkletProcessor for Quiver
 *
 * This runs in the audio thread and processes audio using the WASM engine.
 *
 * Usage:
 * 1. Build this file separately as a worker script
 * 2. Register it with audioContext.audioWorklet.addModule(workletUrl)
 * 3. Create an AudioWorkletNode with processorName 'quiver-processor'
 */

// This file uses the AudioWorkletProcessor API which is available in AudioWorklet scope
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

/**
 * Message types for communication with the main thread
 */
interface InitMessage {
  type: 'init';
  wasmUrl?: string;
}

interface LoadPatchMessage {
  type: 'load_patch';
  patch: unknown;
}

interface SetParamMessage {
  type: 'set_param';
  nodeId: string;
  paramIndex: number;
  value: number;
}

interface AddModuleMessage {
  type: 'add_module';
  typeId: string;
  name: string;
}

interface RemoveModuleMessage {
  type: 'remove_module';
  name: string;
}

interface ConnectMessage {
  type: 'connect';
  from: string;
  to: string;
  attenuation?: number;
  offset?: number;
}

interface DisconnectMessage {
  type: 'disconnect';
  from: string;
  to: string;
}

interface CompileMessage {
  type: 'compile';
}

interface ResetMessage {
  type: 'reset';
}

type WorkletMessage =
  | InitMessage
  | LoadPatchMessage
  | SetParamMessage
  | AddModuleMessage
  | RemoveModuleMessage
  | ConnectMessage
  | DisconnectMessage
  | CompileMessage
  | ResetMessage;

/**
 * Response messages from the worklet
 */
interface ReadyResponse {
  type: 'ready';
}

interface ErrorResponse {
  type: 'error';
  error: string;
}

interface PatchLoadedResponse {
  type: 'patch_loaded';
}

interface CompiledResponse {
  type: 'compiled';
}

type WorkletResponse =
  | ReadyResponse
  | ErrorResponse
  | PatchLoadedResponse
  | CompiledResponse;

/**
 * QuiverProcessor - AudioWorklet processor for Quiver
 *
 * This processor runs in the audio thread and processes audio samples
 * using the Quiver WASM engine.
 */
class QuiverProcessor extends AudioWorkletProcessor {
  private engine: any = null;
  private wasmReady = false;
  private pendingMessages: WorkletMessage[] = [];

  constructor() {
    super();

    this.port.onmessage = async (event: MessageEvent<WorkletMessage>) => {
      const message = event.data;

      if (message.type === 'init') {
        await this.handleInit(message);
      } else if (this.wasmReady && this.engine) {
        this.handleMessage(message);
      } else {
        // Queue messages until WASM is ready
        this.pendingMessages.push(message);
      }
    };
  }

  private async handleInit(message: InitMessage): Promise<void> {
    try {
      // Import the WASM module
      // Note: In production, you'd import from the built WASM package
      // For now, this assumes the WASM module is available globally or via importScripts
      const wasmModule = await import('./quiver.js' as any);
      await wasmModule.default();

      // Create engine with the audio thread's sample rate
      const { QuiverEngine } = wasmModule;
      this.engine = new QuiverEngine(sampleRate);
      this.wasmReady = true;

      // Process any queued messages
      for (const msg of this.pendingMessages) {
        this.handleMessage(msg);
      }
      this.pendingMessages = [];

      this.sendResponse({ type: 'ready' });
    } catch (e) {
      this.sendResponse({
        type: 'error',
        error: String(e),
      });
    }
  }

  private handleMessage(message: WorkletMessage): void {
    if (!this.engine) return;

    try {
      switch (message.type) {
        case 'load_patch':
          this.engine.load_patch(message.patch);
          this.engine.compile();
          this.sendResponse({ type: 'patch_loaded' });
          break;

        case 'set_param':
          this.engine.set_param(message.nodeId, message.paramIndex, message.value);
          break;

        case 'add_module':
          this.engine.add_module(message.typeId, message.name);
          break;

        case 'remove_module':
          this.engine.remove_module(message.name);
          break;

        case 'connect':
          if (message.attenuation !== undefined && message.offset !== undefined) {
            this.engine.connect_modulated(
              message.from,
              message.to,
              message.attenuation,
              message.offset
            );
          } else if (message.attenuation !== undefined) {
            this.engine.connect_attenuated(
              message.from,
              message.to,
              message.attenuation
            );
          } else {
            this.engine.connect(message.from, message.to);
          }
          break;

        case 'disconnect':
          this.engine.disconnect(message.from, message.to);
          break;

        case 'compile':
          this.engine.compile();
          this.sendResponse({ type: 'compiled' });
          break;

        case 'reset':
          this.engine.reset();
          break;
      }
    } catch (e) {
      this.sendResponse({
        type: 'error',
        error: String(e),
      });
    }
  }

  private sendResponse(response: WorkletResponse): void {
    this.port.postMessage(response);
  }

  process(
    inputs: Float32Array[][],
    outputs: Float32Array[][],
    _parameters: Record<string, Float32Array>
  ): boolean {
    if (!this.wasmReady || !this.engine) {
      // Output silence while not ready
      return true;
    }

    const output = outputs[0];
    if (!output || output.length < 2) {
      return true;
    }

    const left = output[0];
    const right = output[1];
    const numSamples = left.length;

    try {
      // Process a block of samples
      const stereoOutput = this.engine.process_block(numSamples);

      // Deinterleave stereo output
      for (let i = 0; i < numSamples; i++) {
        left[i] = stereoOutput[i * 2];
        right[i] = stereoOutput[i * 2 + 1];
      }
    } catch (e) {
      // Output silence on error to avoid audio glitches
      left.fill(0);
      right.fill(0);
      console.error('Quiver processing error:', e);
    }

    // Keep processor alive
    return true;
  }
}

// Register the processor
registerProcessor('quiver-processor', QuiverProcessor);
