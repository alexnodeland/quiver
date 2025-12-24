/**
 * Quiver AudioWorklet Processor
 *
 * This processor runs in the audio thread and reads audio samples from
 * a SharedArrayBuffer that is filled by the main thread WASM engine.
 *
 * Architecture:
 * - Main thread runs WASM engine and fills SharedArrayBuffer with audio
 * - AudioWorklet reads from SharedArrayBuffer and outputs to speakers
 * - Uses Atomics for thread-safe synchronization
 *
 * Buffer Layout:
 * - Bytes 0-3: Write index (Uint32)
 * - Bytes 4-7: Read index (Uint32)
 * - Bytes 8-11: Buffer size in frames (Uint32)
 * - Bytes 12-15: Underrun count (Uint32)
 * - Bytes 16+: Audio data (interleaved stereo float32)
 */

// AudioWorklet global declarations
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

// Buffer header offsets (in 32-bit words)
const WRITE_INDEX_OFFSET = 0;
const READ_INDEX_OFFSET = 1;
const BUFFER_SIZE_OFFSET = 2;
const UNDERRUN_COUNT_OFFSET = 3;
const HEADER_SIZE_WORDS = 4;
const HEADER_SIZE_BYTES = HEADER_SIZE_WORDS * 4;

/**
 * Message types for communication with the main thread
 */
interface InitBufferMessage {
  type: 'init_buffer';
  buffer: SharedArrayBuffer;
}

interface ResetMessage {
  type: 'reset';
}

type WorkletMessage = InitBufferMessage | ResetMessage;

/**
 * Response messages to the main thread
 */
interface ReadyResponse {
  type: 'ready';
}

interface UnderrunResponse {
  type: 'underrun';
  count: number;
}

type WorkletResponse = ReadyResponse | UnderrunResponse;

/**
 * QuiverProcessor - Reads audio from SharedArrayBuffer
 *
 * This processor is designed to be lock-free and real-time safe.
 * It uses atomic operations to safely read the ring buffer.
 */
class QuiverProcessor extends AudioWorkletProcessor {
  private sharedBuffer: SharedArrayBuffer | null = null;
  private headerView: Int32Array | null = null;
  private audioData: Float32Array | null = null;
  private bufferSizeFrames = 0;
  private lastUnderrunCount = 0;

  constructor() {
    super();

    this.port.onmessage = (event: MessageEvent<WorkletMessage>) => {
      this.handleMessage(event.data);
    };
  }

  private handleMessage(message: WorkletMessage): void {
    switch (message.type) {
      case 'init_buffer':
        this.initBuffer(message.buffer);
        break;

      case 'reset':
        this.reset();
        break;
    }
  }

  private initBuffer(buffer: SharedArrayBuffer): void {
    this.sharedBuffer = buffer;
    this.headerView = new Int32Array(buffer, 0, HEADER_SIZE_WORDS);
    this.bufferSizeFrames = Atomics.load(this.headerView, BUFFER_SIZE_OFFSET);

    // Audio data starts after header, each frame is 2 floats (stereo)
    const audioDataBytes = this.bufferSizeFrames * 2 * 4;
    this.audioData = new Float32Array(buffer, HEADER_SIZE_BYTES, this.bufferSizeFrames * 2);

    // Reset read index to match write index
    const writeIndex = Atomics.load(this.headerView, WRITE_INDEX_OFFSET);
    Atomics.store(this.headerView, READ_INDEX_OFFSET, writeIndex);

    this.sendResponse({ type: 'ready' });
  }

  private reset(): void {
    if (this.headerView) {
      const writeIndex = Atomics.load(this.headerView, WRITE_INDEX_OFFSET);
      Atomics.store(this.headerView, READ_INDEX_OFFSET, writeIndex);
    }
    this.lastUnderrunCount = 0;
  }

  private sendResponse(response: WorkletResponse): void {
    this.port.postMessage(response);
  }

  process(
    _inputs: Float32Array[][],
    outputs: Float32Array[][],
    _parameters: Record<string, Float32Array>
  ): boolean {
    const output = outputs[0];
    if (!output || output.length < 2) {
      return true;
    }

    const left = output[0];
    const right = output[1];
    const numSamples = left.length;

    // Not initialized yet - output silence
    if (!this.headerView || !this.audioData) {
      left.fill(0);
      right.fill(0);
      return true;
    }

    // Read indices atomically
    const writeIndex = Atomics.load(this.headerView, WRITE_INDEX_OFFSET);
    let readIndex = Atomics.load(this.headerView, READ_INDEX_OFFSET);

    // Calculate available frames
    let available = writeIndex - readIndex;
    if (available < 0) {
      available += this.bufferSizeFrames;
    }

    // Check for underrun
    if (available < numSamples) {
      // Not enough data - output silence and report underrun
      left.fill(0);
      right.fill(0);

      // Increment underrun counter atomically
      Atomics.add(this.headerView, UNDERRUN_COUNT_OFFSET, 1);
      const underrunCount = Atomics.load(this.headerView, UNDERRUN_COUNT_OFFSET);

      // Report underrun periodically
      if (underrunCount > this.lastUnderrunCount + 10) {
        this.sendResponse({ type: 'underrun', count: underrunCount });
        this.lastUnderrunCount = underrunCount;
      }

      return true;
    }

    // Read audio data from ring buffer
    for (let i = 0; i < numSamples; i++) {
      const bufferOffset = (readIndex % this.bufferSizeFrames) * 2;
      left[i] = this.audioData[bufferOffset];
      right[i] = this.audioData[bufferOffset + 1];
      readIndex++;
    }

    // Update read index atomically
    Atomics.store(this.headerView, READ_INDEX_OFFSET, readIndex % this.bufferSizeFrames);

    return true;
  }
}

// Register the processor
registerProcessor('quiver-processor', QuiverProcessor);
