// Quiver Browser Synth - Polyphonic Version
// This synth uses the modular patching architecture where all control
// is done through input ports via "knob" modules (Offset modules).
import init, { QuiverEngine } from '../../../packages/@quiver/wasm/quiver.js';

const NUM_VOICES = 4;

// Global state
let engine: QuiverEngine | null = null;
let audioContext: AudioContext | null = null;
let workletNode: ScriptProcessorNode | null = null;
let isRunning = false;
let currentWaveform = 'saw';

// Voice allocation
interface Voice {
  index: number;
  note: number | null;
  active: boolean;
}
let voices: Voice[] = [];
let noteToVoice: Map<number, number> = new Map(); // MIDI note -> voice index

// Waveform output port names on the VCO
const waveformPorts: Record<string, string> = {
  'saw': 'saw',
  'pulse': 'sqr',
  'tri': 'tri',
  'sine': 'sin'
};

// Find a free voice or steal the oldest one
function allocateVoice(note: number): number {
  // First, look for a free voice
  for (const voice of voices) {
    if (!voice.active) {
      return voice.index;
    }
  }
  // All voices are active, steal the first one (simple strategy)
  return 0;
}

// Switch waveform for all voices
function switchWaveform(newWaveform: string) {
  if (!engine || newWaveform === currentWaveform) return;

  const oldPortName = waveformPorts[currentWaveform];
  const newPortName = waveformPorts[newWaveform];

  if (!oldPortName || !newPortName) return;

  try {
    for (let i = 0; i < NUM_VOICES; i++) {
      engine.disconnect(`osc_${i}.${oldPortName}`, `filter_${i}.in`);
      engine.connect(`osc_${i}.${newPortName}`, `filter_${i}.in`);
    }
    engine.compile();
    currentWaveform = newWaveform;
    console.log('Switched waveform to:', newWaveform);
  } catch (error) {
    console.error('Failed to switch waveform:', error);
  }
}

// Elements
const statusEl = document.getElementById('status')!;
const startBtn = document.getElementById('start') as HTMLButtonElement;
const scopeCanvas = document.getElementById('scope') as HTMLCanvasElement;
const scopeCtx = scopeCanvas.getContext('2d')!;

// Audio buffer for scope
let scopeBuffer: Float32Array = new Float32Array(256);
let scopeIndex = 0;

// Initialize WASM
async function initWasm() {
  try {
    await init();
    statusEl.className = 'status ready';
    statusEl.textContent = 'WASM loaded - Click "Start Audio" to begin';
    startBtn.disabled = false;
  } catch (error) {
    statusEl.className = 'status error';
    statusEl.textContent = `Failed to load WASM: ${(error as Error).message}`;
    console.error('WASM init failed:', error);
  }
}

// Create a polyphonic synth patch
function createSynthPatch(engine: QuiverEngine) {
  const addModule = (typeId: string, name: string) => {
    console.log(`Adding module: ${typeId} as "${name}"`);
    engine.add_module(typeId, name);
  };

  const connect = (from: string, to: string) => {
    engine.connect(from, to);
  };

  // Initialize voice state
  voices = [];
  for (let i = 0; i < NUM_VOICES; i++) {
    voices.push({ index: i, note: null, active: false });
  }

  // ===== Create voices =====
  for (let i = 0; i < NUM_VOICES; i++) {
    // Sound generators and processors per voice
    addModule('vco', `osc_${i}`);
    addModule('svf', `filter_${i}`);
    addModule('adsr', `env_${i}`);
    addModule('vca', `amp_${i}`);

    // Per-voice knobs (pitch and gate change per note)
    addModule('offset', `pitch_${i}`);
    addModule('offset', `gate_${i}`);
  }

  // ===== Shared knob modules (same for all voices) =====
  addModule('offset', 'attack_knob');
  addModule('offset', 'decay_knob');
  addModule('offset', 'sustain_knob');
  addModule('offset', 'release_knob');
  addModule('offset', 'pw_knob');
  addModule('offset', 'detune_knob');
  addModule('offset', 'cutoff_knob');
  addModule('offset', 'resonance_knob');

  // Mixer to combine all voices
  addModule('mixer', 'voice_mixer');

  // Output
  addModule('stereo_output', 'out');

  // ===== Connect each voice =====
  for (let i = 0; i < NUM_VOICES; i++) {
    // Pitch and gate (per-voice)
    connect(`pitch_${i}.out`, `osc_${i}.voct`);
    connect(`gate_${i}.out`, `env_${i}.gate`);

    // Shared oscillator controls
    connect('pw_knob.out', `osc_${i}.pw`);
    connect('detune_knob.out', `osc_${i}.fm`);

    // Shared envelope controls
    connect('attack_knob.out', `env_${i}.attack`);
    connect('decay_knob.out', `env_${i}.decay`);
    connect('sustain_knob.out', `env_${i}.sustain`);
    connect('release_knob.out', `env_${i}.release`);

    // Audio path: VCO -> Filter -> VCA
    connect(`osc_${i}.saw`, `filter_${i}.in`);
    connect(`filter_${i}.lp`, `amp_${i}.in`);

    // Filter controls (shared)
    connect('cutoff_knob.out', `filter_${i}.cutoff`);
    connect('resonance_knob.out', `filter_${i}.res`);

    // Envelope -> VCA
    connect(`env_${i}.env`, `amp_${i}.cv`);

    // Voice output -> Mixer
    connect(`amp_${i}.out`, `voice_mixer.ch${i}`);
  }

  // Mixer -> Stereo Output
  connect('voice_mixer.out', 'out.left');
  connect('voice_mixer.out', 'out.right');

  // ===== Set initial knob values =====
  const msToCV = (ms: number) => Math.log10(Math.max(1, ms)) / 4;
  const hzToCV = (hz: number) => Math.log10(hz / 20) / 3;

  console.log('Setting initial knob values...');

  // ADSR (shared)
  engine.set_param('attack_knob', 0, msToCV(10));
  engine.set_param('decay_knob', 0, msToCV(200));
  engine.set_param('sustain_knob', 0, 0.5);
  engine.set_param('release_knob', 0, msToCV(300));

  // Oscillator (shared)
  engine.set_param('pw_knob', 0, 0.5);
  engine.set_param('detune_knob', 0, 0.0);

  // Filter (shared)
  engine.set_param('cutoff_knob', 0, hzToCV(2000));
  engine.set_param('resonance_knob', 0, 0.3);

  // Initialize all voice gates to off
  for (let i = 0; i < NUM_VOICES; i++) {
    engine.set_param(`pitch_${i}`, 0, 0.0);
    engine.set_param(`gate_${i}`, 0, 0.0);
  }

  // Set output module
  engine.set_output('out');

  // Compile the patch
  console.log('Compiling patch...');
  engine.compile();

  // Reset all module state to ensure clean start
  engine.reset();

  console.log('Polyphonic synth patch created successfully');
  console.log('Voices:', NUM_VOICES);
  console.log('Modules:', engine.module_count());
  console.log('Cables:', engine.cable_count());
}

// Process audio using ScriptProcessorNode
function createScriptProcessor(ctx: AudioContext): ScriptProcessorNode {
  const processor = ctx.createScriptProcessor(512, 0, 2);

  processor.onaudioprocess = (e) => {
    if (!engine || !isRunning) {
      const leftOut = e.outputBuffer.getChannelData(0);
      const rightOut = e.outputBuffer.getChannelData(1);
      leftOut.fill(0);
      rightOut.fill(0);
      return;
    }

    try {
      const samples = engine.process_block(512);
      const leftOut = e.outputBuffer.getChannelData(0);
      const rightOut = e.outputBuffer.getChannelData(1);
      const volume = parseFloat((document.getElementById('volume') as HTMLInputElement).value);

      // Scale down from modular levels (±5V) to audio levels
      // Divide by NUM_VOICES to prevent clipping when all voices play
      const scale = volume / 5.0 / Math.sqrt(NUM_VOICES);

      for (let i = 0; i < 512; i++) {
        leftOut[i] = samples[i * 2] * scale;
        rightOut[i] = samples[i * 2 + 1] * scale;

        // Update scope buffer
        scopeBuffer[scopeIndex] = leftOut[i];
        scopeIndex = (scopeIndex + 1) % scopeBuffer.length;
      }
    } catch (error) {
      console.error('Audio processing error:', error);
    }
  };

  return processor;
}

// Start audio
async function startAudio() {
  if (audioContext) return;

  try {
    audioContext = new AudioContext({ sampleRate: 44100 });

    if (audioContext.state === 'suspended') {
      await audioContext.resume();
    }

    engine = new QuiverEngine(audioContext.sampleRate);
    console.log('Engine created at', audioContext.sampleRate, 'Hz');

    createSynthPatch(engine);

    workletNode = createScriptProcessor(audioContext);
    workletNode.connect(audioContext.destination);

    isRunning = true;
    startBtn.textContent = 'Audio Running';
    startBtn.disabled = true;
    statusEl.textContent = `Audio running - ${NUM_VOICES} voice polyphony!`;

    requestAnimationFrame(drawScope);
  } catch (error) {
    statusEl.className = 'status error';
    const errorMsg = error instanceof Error ? error.message : String(error);
    statusEl.textContent = `Audio error: ${errorMsg}`;
    console.error('Failed to start audio:', error);
  }
}

// Draw oscilloscope
function drawScope() {
  if (!isRunning) return;

  scopeCtx.fillStyle = '#0a0a1a';
  scopeCtx.fillRect(0, 0, scopeCanvas.width, scopeCanvas.height);

  scopeCtx.strokeStyle = '#7b68ee';
  scopeCtx.lineWidth = 2;
  scopeCtx.beginPath();

  const sliceWidth = scopeCanvas.width / scopeBuffer.length;
  let x = 0;

  for (let i = 0; i < scopeBuffer.length; i++) {
    const v = scopeBuffer[(scopeIndex + i) % scopeBuffer.length];
    const y = (1 - v) * scopeCanvas.height / 2;

    if (i === 0) {
      scopeCtx.moveTo(x, y);
    } else {
      scopeCtx.lineTo(x, y);
    }

    x += sliceWidth;
  }

  scopeCtx.stroke();

  // Draw center line
  scopeCtx.strokeStyle = '#333';
  scopeCtx.lineWidth = 1;
  scopeCtx.beginPath();
  scopeCtx.moveTo(0, scopeCanvas.height / 2);
  scopeCtx.lineTo(scopeCanvas.width, scopeCanvas.height / 2);
  scopeCtx.stroke();

  requestAnimationFrame(drawScope);
}

// Note on - allocate a voice and set pitch/gate
function noteOn(note: number) {
  if (!engine || noteToVoice.has(note)) return;

  const voiceIndex = allocateVoice(note);
  const voice = voices[voiceIndex];

  // If stealing a voice, release the old note
  if (voice.note !== null) {
    noteToVoice.delete(voice.note);
  }

  // Assign this note to the voice
  voice.note = note;
  voice.active = true;
  noteToVoice.set(note, voiceIndex);

  // Convert MIDI note to V/Oct
  const vOct = (note - 60) / 12.0;

  // Set pitch and gate for this voice
  engine.set_param(`pitch_${voiceIndex}`, 0, vOct);
  engine.set_param(`gate_${voiceIndex}`, 0, 5.0);

  console.log(`Note on: ${note} -> voice ${voiceIndex}`);

  // Highlight key
  const key = document.querySelector(`[data-note="${note}"]`);
  if (key) key.classList.add('active');
}

// Note off - release the voice
function noteOff(note: number) {
  if (!engine) return;

  const voiceIndex = noteToVoice.get(note);
  if (voiceIndex === undefined) return;

  const voice = voices[voiceIndex];

  // Release the gate (triggers ADSR release)
  engine.set_param(`gate_${voiceIndex}`, 0, 0.0);

  // Mark voice as available (but keep note info for release phase)
  voice.active = false;
  voice.note = null;
  noteToVoice.delete(note);

  console.log(`Note off: ${note} -> voice ${voiceIndex} released`);

  // Un-highlight key
  const key = document.querySelector(`[data-note="${note}"]`);
  if (key) key.classList.remove('active');
}

// Setup keyboard controls
function setupKeyboard() {
  const keyMap: Record<string, number> = {
    'a': 60, 'w': 61, 's': 62, 'e': 63, 'd': 64,
    'f': 65, 't': 66, 'g': 67, 'y': 68, 'h': 69,
    'u': 70, 'j': 71, 'k': 72
  };

  document.addEventListener('keydown', (e) => {
    if (e.repeat) return;
    const note = keyMap[e.key.toLowerCase()];
    if (note !== undefined) {
      noteOn(note);
    }
  });

  document.addEventListener('keyup', (e) => {
    const note = keyMap[e.key.toLowerCase()];
    if (note !== undefined) {
      noteOff(note);
    }
  });

  // Mouse/touch on virtual keyboard
  const keyboard = document.getElementById('keyboard')!;
  keyboard.addEventListener('mousedown', (e) => {
    const key = (e.target as HTMLElement).closest('.key');
    if (key) {
      const note = parseInt(key.getAttribute('data-note')!);
      noteOn(note);
    }
  });

  document.addEventListener('mouseup', () => {
    // Release all notes
    for (const [note] of noteToVoice) {
      noteOff(note);
    }
  });
}

// Setup parameter controls
function setupControls() {
  const bindSlider = (id: string, callback: (value: number) => void) => {
    const slider = document.getElementById(id) as HTMLInputElement;
    const valueEl = document.getElementById(id + 'Value');

    const update = () => {
      const value = parseFloat(slider.value);
      if (valueEl) {
        valueEl.textContent = value.toFixed(value < 10 ? 2 : 0);
      }
      callback(value);
    };

    slider.addEventListener('input', update);
    update();
  };

  // Volume control
  bindSlider('volume', () => {});

  // Waveform selector
  const waveformSelect = document.getElementById('waveform') as HTMLSelectElement;
  waveformSelect.addEventListener('change', () => {
    switchWaveform(waveformSelect.value);
  });

  // Conversion functions
  const msToCV = (ms: number) => Math.log10(Math.max(1, ms)) / 4;
  const hzToCV = (hz: number) => Math.log10(hz / 20) / 3;

  // ADSR envelope controls (shared across all voices)
  bindSlider('attack', (ms) => {
    if (engine) engine.set_param('attack_knob', 0, msToCV(ms));
  });
  bindSlider('decay', (ms) => {
    if (engine) engine.set_param('decay_knob', 0, msToCV(ms));
  });
  bindSlider('sustain', (v) => {
    if (engine) engine.set_param('sustain_knob', 0, v);
  });
  bindSlider('release', (ms) => {
    if (engine) engine.set_param('release_knob', 0, msToCV(ms));
  });

  // Oscillator controls (shared)
  bindSlider('pulseWidth', (v) => {
    if (engine) engine.set_param('pw_knob', 0, v);
  });
  bindSlider('detune', (cents) => {
    const vOct = cents / 1200;
    if (engine) engine.set_param('detune_knob', 0, vOct);
  });

  // Filter controls (shared)
  bindSlider('cutoff', (hz) => {
    if (engine) engine.set_param('cutoff_knob', 0, hzToCV(hz));
  });
  bindSlider('resonance', (v) => {
    if (engine) engine.set_param('resonance_knob', 0, v);
  });
  bindSlider('filterEnv', () => {
    // TODO: Filter envelope amount needs VCA module
  });
}

// Initialize
async function main() {
  console.log('Initializing Quiver Browser Synth (Polyphonic)...');
  await initWasm();
  setupKeyboard();
  setupControls();
  startBtn.addEventListener('click', startAudio);
}

main();
