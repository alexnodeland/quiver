/**
 * Quiver Audio Manager
 *
 * Manages the WASM engine and SharedArrayBuffer communication with AudioWorklet.
 *
 * Architecture:
 * - WASM engine runs in main thread (setInterval callback)
 * - Audio data written to SharedArrayBuffer ring buffer
 * - AudioWorklet reads from buffer and outputs to speakers
 *
 * This approach avoids the complexity of running WASM in AudioWorklet
 * while maintaining low-latency audio output.
 */

import { initWasm } from './index';

// Buffer header offsets (must match worklet-processor.ts)
const WRITE_INDEX_OFFSET = 0;
const READ_INDEX_OFFSET = 1;
const BUFFER_SIZE_OFFSET = 2;
const UNDERRUN_COUNT_OFFSET = 3;
const HEADER_SIZE_WORDS = 4;
const HEADER_SIZE_BYTES = HEADER_SIZE_WORDS * 4;

/**
 * Options for creating an AudioManager
 */
export interface AudioManagerOptions {
  /** Buffer size in frames (default: 4096) */
  bufferSize?: number;
  /** Processing callback interval in ms (default: 10) */
  processIntervalMs?: number;
  /** URL to the worklet processor script */
  workletUrl: string;
}

/**
 * Callback for underrun events
 */
export type UnderrunCallback = (count: number) => void;

/**
 * AudioManager - Manages WASM engine and audio output
 */
export class AudioManager {
  private audioContext: AudioContext;
  private workletNode: AudioWorkletNode | null = null;
  private engine: any = null;
  private sharedBuffer: SharedArrayBuffer | null = null;
  private headerView: Int32Array | null = null;
  private audioData: Float32Array | null = null;
  private bufferSizeFrames: number;
  private processIntervalMs: number;
  private workletUrl: string;
  private processIntervalId: ReturnType<typeof setInterval> | null = null;
  private isRunning = false;
  private onUnderrun: UnderrunCallback | null = null;

  constructor(audioContext: AudioContext, options: AudioManagerOptions) {
    this.audioContext = audioContext;
    this.bufferSizeFrames = options.bufferSize ?? 4096;
    this.processIntervalMs = options.processIntervalMs ?? 10;
    this.workletUrl = options.workletUrl;
  }

  /**
   * Initialize the audio manager
   */
  async init(): Promise<void> {
    // Check for SharedArrayBuffer support
    if (typeof SharedArrayBuffer === 'undefined') {
      throw new Error(
        'SharedArrayBuffer is not available. ' +
          'Ensure the page is served with COOP/COEP headers: ' +
          'Cross-Origin-Opener-Policy: same-origin, ' +
          'Cross-Origin-Embedder-Policy: require-corp'
      );
    }

    // Initialize WASM
    await initWasm();
    const { QuiverEngine } = await import('../quiver');
    this.engine = new QuiverEngine(this.audioContext.sampleRate);

    // Create shared buffer
    // Layout: header (16 bytes) + audio data (bufferSize * 2 channels * 4 bytes/sample)
    const audioDataSize = this.bufferSizeFrames * 2 * 4;
    this.sharedBuffer = new SharedArrayBuffer(HEADER_SIZE_BYTES + audioDataSize);
    this.headerView = new Int32Array(this.sharedBuffer, 0, HEADER_SIZE_WORDS);
    this.audioData = new Float32Array(
      this.sharedBuffer,
      HEADER_SIZE_BYTES,
      this.bufferSizeFrames * 2
    );

    // Initialize header
    Atomics.store(this.headerView, WRITE_INDEX_OFFSET, 0);
    Atomics.store(this.headerView, READ_INDEX_OFFSET, 0);
    Atomics.store(this.headerView, BUFFER_SIZE_OFFSET, this.bufferSizeFrames);
    Atomics.store(this.headerView, UNDERRUN_COUNT_OFFSET, 0);

    // Load and create worklet
    await this.audioContext.audioWorklet.addModule(this.workletUrl);
    this.workletNode = new AudioWorkletNode(this.audioContext, 'quiver-processor', {
      numberOfInputs: 0,
      numberOfOutputs: 1,
      outputChannelCount: [2],
    });

    // Handle messages from worklet
    this.workletNode.port.onmessage = (event) => {
      if (event.data.type === 'underrun' && this.onUnderrun) {
        this.onUnderrun(event.data.count);
      }
    };

    // Wait for worklet to be ready
    await new Promise<void>((resolve, reject) => {
      const timeout = setTimeout(() => {
        reject(new Error('Worklet initialization timeout'));
      }, 5000);

      const handler = (event: MessageEvent) => {
        if (event.data.type === 'ready') {
          clearTimeout(timeout);
          this.workletNode!.port.removeEventListener('message', handler);
          resolve();
        }
      };

      this.workletNode!.port.addEventListener('message', handler);
      this.workletNode!.port.start();

      // Send buffer to worklet
      this.workletNode!.port.postMessage({
        type: 'init_buffer',
        buffer: this.sharedBuffer,
      });
    });
  }

  /**
   * Get the AudioWorkletNode for connecting to audio graph
   */
  get node(): AudioWorkletNode {
    if (!this.workletNode) {
      throw new Error('AudioManager not initialized. Call init() first.');
    }
    return this.workletNode;
  }

  /**
   * Get the underlying Quiver engine
   */
  getEngine(): any {
    if (!this.engine) {
      throw new Error('AudioManager not initialized. Call init() first.');
    }
    return this.engine;
  }

  /**
   * Set callback for underrun events
   */
  setUnderrunCallback(callback: UnderrunCallback): void {
    this.onUnderrun = callback;
  }

  /**
   * Start audio processing
   */
  start(): void {
    if (this.isRunning) return;
    if (!this.engine || !this.headerView || !this.audioData) {
      throw new Error('AudioManager not initialized. Call init() first.');
    }

    this.isRunning = true;

    // Start processing loop
    this.processIntervalId = setInterval(() => {
      this.processAudio();
    }, this.processIntervalMs);
  }

  /**
   * Stop audio processing
   */
  stop(): void {
    if (!this.isRunning) return;
    this.isRunning = false;

    if (this.processIntervalId) {
      clearInterval(this.processIntervalId);
      this.processIntervalId = null;
    }
  }

  /**
   * Process audio samples and fill the ring buffer
   */
  private processAudio(): void {
    if (!this.engine || !this.headerView || !this.audioData) return;

    const writeIndex = Atomics.load(this.headerView, WRITE_INDEX_OFFSET);
    const readIndex = Atomics.load(this.headerView, READ_INDEX_OFFSET);

    // Calculate available space in buffer
    let available = readIndex - writeIndex - 1;
    if (available < 0) {
      available += this.bufferSizeFrames;
    }

    // Don't overfill - leave some headroom
    const targetFrames = Math.min(available, 512);
    if (targetFrames <= 0) return;

    try {
      // Process a block of samples
      const stereoOutput = this.engine.process_block(targetFrames);

      // Write to ring buffer
      let writePos = writeIndex;
      for (let i = 0; i < targetFrames; i++) {
        const bufferOffset = (writePos % this.bufferSizeFrames) * 2;
        this.audioData[bufferOffset] = stereoOutput[i * 2];
        this.audioData[bufferOffset + 1] = stereoOutput[i * 2 + 1];
        writePos++;
      }

      // Update write index atomically
      Atomics.store(this.headerView, WRITE_INDEX_OFFSET, writePos % this.bufferSizeFrames);
    } catch (e) {
      console.error('Audio processing error:', e);
    }
  }

  /**
   * Load a patch into the engine
   */
  loadPatch(patch: unknown): void {
    if (!this.engine) {
      throw new Error('AudioManager not initialized. Call init() first.');
    }
    this.engine.load_patch(patch);
    this.engine.compile();
  }

  /**
   * Set a parameter value
   */
  setParam(nodeId: string, paramIndex: number, value: number): void {
    if (!this.engine) return;
    this.engine.set_param(nodeId, paramIndex, value);
  }

  /**
   * Set a parameter by name
   */
  setParamByName(nodeId: string, paramName: string, value: number): void {
    if (!this.engine) return;
    this.engine.set_param_by_name(nodeId, paramName, value);
  }

  /**
   * Add a module to the patch
   */
  addModule(typeId: string, name: string): void {
    if (!this.engine) return;
    this.engine.add_module(typeId, name);
  }

  /**
   * Remove a module from the patch
   */
  removeModule(name: string): void {
    if (!this.engine) return;
    this.engine.remove_module(name);
  }

  /**
   * Connect two ports
   */
  connect(from: string, to: string): void {
    if (!this.engine) return;
    this.engine.connect(from, to);
  }

  /**
   * Connect with attenuation
   */
  connectAttenuated(from: string, to: string, attenuation: number): void {
    if (!this.engine) return;
    this.engine.connect_attenuated(from, to, attenuation);
  }

  /**
   * Disconnect two ports
   */
  disconnect(from: string, to: string): void {
    if (!this.engine) return;
    this.engine.disconnect(from, to);
  }

  /**
   * Compile the patch
   */
  compile(): void {
    if (!this.engine) return;
    this.engine.compile();
  }

  /**
   * Reset the engine
   */
  reset(): void {
    if (!this.engine) return;
    this.engine.reset();

    // Reset worklet buffer indices
    if (this.workletNode) {
      this.workletNode.port.postMessage({ type: 'reset' });
    }
  }

  /**
   * Set the output module
   */
  setOutput(name: string): void {
    if (!this.engine) return;
    this.engine.set_output(name);
  }

  /**
   * MIDI note on
   */
  midiNoteOn(note: number, velocity: number): void {
    if (!this.engine) return;
    this.engine.midi_note_on(note, velocity);
  }

  /**
   * MIDI note off
   */
  midiNoteOff(note: number, velocity: number): void {
    if (!this.engine) return;
    this.engine.midi_note_off(note, velocity);
  }

  /**
   * MIDI CC
   */
  midiCC(cc: number, value: number): void {
    if (!this.engine) return;
    this.engine.midi_cc(cc, value);
  }

  /**
   * Dispose of all resources
   */
  dispose(): void {
    this.stop();

    if (this.workletNode) {
      this.workletNode.disconnect();
      this.workletNode.port.close();
      this.workletNode = null;
    }

    if (this.engine) {
      this.engine.free();
      this.engine = null;
    }

    this.sharedBuffer = null;
    this.headerView = null;
    this.audioData = null;
  }
}

/**
 * Create an AudioManager with default configuration
 */
export async function createAudioManager(
  audioContext: AudioContext,
  workletUrl: string
): Promise<AudioManager> {
  const manager = new AudioManager(audioContext, { workletUrl });
  await manager.init();
  return manager;
}
