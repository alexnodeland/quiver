/**
 * Audio context utilities for Quiver WASM
 *
 * This module provides helpers for setting up real-time audio
 * in the browser using AudioWorklet.
 */

/**
 * Options for creating a Quiver audio node
 */
export interface QuiverAudioNodeOptions {
  /** URL to the compiled worklet script */
  workletUrl: string;
  /** Output channel count (default: 2 for stereo) */
  outputChannels?: number;
}

/**
 * Interface for the Quiver AudioWorklet node
 */
export interface QuiverAudioNode {
  /** The underlying AudioWorkletNode */
  node: AudioWorkletNode;
  /** The AudioContext */
  context: AudioContext;
  /** Load a patch into the engine */
  loadPatch: (patch: unknown) => Promise<void>;
  /** Set a parameter value */
  setParam: (nodeId: string, paramIndex: number, value: number) => void;
  /** Add a module to the patch */
  addModule: (typeId: string, name: string) => void;
  /** Remove a module from the patch */
  removeModule: (name: string) => void;
  /** Connect two ports */
  connect: (from: string, to: string, attenuation?: number, offset?: number) => void;
  /** Disconnect two ports */
  disconnect: (from: string, to: string) => void;
  /** Compile the patch */
  compile: () => Promise<void>;
  /** Reset all module state */
  reset: () => void;
  /** Disconnect from audio graph and cleanup */
  dispose: () => void;
}

/**
 * Create a Quiver AudioWorklet node
 *
 * @param audioContext The AudioContext to use
 * @param options Configuration options
 * @returns A QuiverAudioNode instance
 *
 * @example
 * ```typescript
 * const audioContext = new AudioContext();
 * const quiver = await createQuiverAudioNode(audioContext, {
 *   workletUrl: '/quiver-worklet.js',
 * });
 *
 * await quiver.loadPatch(myPatch);
 * quiver.node.connect(audioContext.destination);
 * ```
 */
export async function createQuiverAudioNode(
  audioContext: AudioContext,
  options: QuiverAudioNodeOptions
): Promise<QuiverAudioNode> {
  const { workletUrl, outputChannels = 2 } = options;

  // Load the worklet module
  await audioContext.audioWorklet.addModule(workletUrl);

  // Create the worklet node
  const node = new AudioWorkletNode(audioContext, 'quiver-processor', {
    numberOfInputs: 0,
    numberOfOutputs: 1,
    outputChannelCount: [outputChannels],
  });

  // Wait for WASM initialization
  await new Promise<void>((resolve, reject) => {
    const timeout = setTimeout(() => {
      reject(new Error('Quiver worklet initialization timeout'));
    }, 10000);

    const handler = (event: MessageEvent) => {
      if (event.data.type === 'ready') {
        clearTimeout(timeout);
        node.port.removeEventListener('message', handler);
        resolve();
      } else if (event.data.type === 'error') {
        clearTimeout(timeout);
        node.port.removeEventListener('message', handler);
        reject(new Error(event.data.error));
      }
    };

    node.port.addEventListener('message', handler);
    node.port.start();
    node.port.postMessage({ type: 'init' });
  });

  // Create helper functions
  const loadPatch = (patch: unknown): Promise<void> => {
    return new Promise((resolve, reject) => {
      const handler = (event: MessageEvent) => {
        if (event.data.type === 'patch_loaded') {
          node.port.removeEventListener('message', handler);
          resolve();
        } else if (event.data.type === 'error') {
          node.port.removeEventListener('message', handler);
          reject(new Error(event.data.error));
        }
      };
      node.port.addEventListener('message', handler);
      node.port.postMessage({ type: 'load_patch', patch });
    });
  };

  const setParam = (nodeId: string, paramIndex: number, value: number): void => {
    node.port.postMessage({ type: 'set_param', nodeId, paramIndex, value });
  };

  const addModule = (typeId: string, name: string): void => {
    node.port.postMessage({ type: 'add_module', typeId, name });
  };

  const removeModule = (name: string): void => {
    node.port.postMessage({ type: 'remove_module', name });
  };

  const connectPorts = (
    from: string,
    to: string,
    attenuation?: number,
    offset?: number
  ): void => {
    node.port.postMessage({ type: 'connect', from, to, attenuation, offset });
  };

  const disconnectPorts = (from: string, to: string): void => {
    node.port.postMessage({ type: 'disconnect', from, to });
  };

  const compile = (): Promise<void> => {
    return new Promise((resolve, reject) => {
      const handler = (event: MessageEvent) => {
        if (event.data.type === 'compiled') {
          node.port.removeEventListener('message', handler);
          resolve();
        } else if (event.data.type === 'error') {
          node.port.removeEventListener('message', handler);
          reject(new Error(event.data.error));
        }
      };
      node.port.addEventListener('message', handler);
      node.port.postMessage({ type: 'compile' });
    });
  };

  const reset = (): void => {
    node.port.postMessage({ type: 'reset' });
  };

  const dispose = (): void => {
    node.disconnect();
    node.port.close();
  };

  return {
    node,
    context: audioContext,
    loadPatch,
    setParam,
    addModule,
    removeModule,
    connect: connectPorts,
    disconnect: disconnectPorts,
    compile,
    reset,
    dispose,
  };
}

/**
 * Create a simple audio setup with Quiver connected to the destination
 *
 * @param workletUrl URL to the compiled worklet script
 * @returns A QuiverAudioNode connected to the audio destination
 *
 * @example
 * ```typescript
 * const quiver = await createQuiverAudio('/quiver-worklet.js');
 * await quiver.loadPatch(myPatch);
 * // Audio is now playing through speakers
 * ```
 */
export async function createQuiverAudio(
  workletUrl: string
): Promise<QuiverAudioNode> {
  const audioContext = new AudioContext();
  const quiver = await createQuiverAudioNode(audioContext, { workletUrl });

  // Connect to destination
  quiver.node.connect(audioContext.destination);

  return quiver;
}
