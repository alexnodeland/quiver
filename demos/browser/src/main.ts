// Quiver Browser Synth - Polyphonic Version with Enhanced Visualizations
// Retro-futuristic modular synthesizer demo
import init, { QuiverEngine } from '../../../packages/@quiver/wasm/quiver.js';

const NUM_VOICES = 4;
const FFT_SIZE = 256;

// Global state
let engine: QuiverEngine | null = null;
let audioContext: AudioContext | null = null;
let workletNode: ScriptProcessorNode | null = null;
let analyserNode: AnalyserNode | null = null;
let isRunning = false;
let currentWaveform = 'saw';
let visualizationMode: 'scope' | 'lissajous' | 'bars' = 'scope';

// Voice allocation
interface Voice {
  index: number;
  note: number | null;
  active: boolean;
}
let voices: Voice[] = [];
let noteToVoice: Map<number, number> = new Map();

// Audio buffers for visualization
let scopeBufferL: Float32Array = new Float32Array(512);
let scopeBufferR: Float32Array = new Float32Array(512);
let scopeIndex = 0;
let peakL = 0;
let peakR = 0;
let peakDecay = 0.95;

// FFT data for spectrum analyzer
let frequencyData = new Uint8Array(FFT_SIZE / 2);

// Waveform output port names on the VCO
const waveformPorts: Record<string, string> = {
  'saw': 'saw',
  'pulse': 'sqr',
  'tri': 'tri',
  'sine': 'sin'
};

// Find a free voice or steal the oldest one
function allocateVoice(note: number): number {
  for (const voice of voices) {
    if (!voice.active) {
      return voice.index;
    }
  }
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
  } catch (error) {
    console.error('Failed to switch waveform:', error);
  }
}

// Elements
const statusEl = document.getElementById('status')!;
const startBtn = document.getElementById('start') as HTMLButtonElement;
const scopeCanvas = document.getElementById('scope') as HTMLCanvasElement;
const spectrumCanvas = document.getElementById('spectrum') as HTMLCanvasElement;
const scopeCtx = scopeCanvas.getContext('2d')!;
const spectrumCtx = spectrumCanvas.getContext('2d')!;

// Set canvas resolution
function setupCanvas(canvas: HTMLCanvasElement, ctx: CanvasRenderingContext2D) {
  const rect = canvas.getBoundingClientRect();
  const dpr = window.devicePixelRatio || 1;
  canvas.width = rect.width * dpr;
  canvas.height = rect.height * dpr;
  ctx.scale(dpr, dpr);
  return { width: rect.width, height: rect.height };
}

// Initialize WASM
async function initWasm() {
  try {
    await init();
    statusEl.className = 'ready';
    statusEl.textContent = 'Ready // Click to Initialize';
    startBtn.disabled = false;
  } catch (error) {
    statusEl.className = 'error';
    statusEl.textContent = `WASM Error: ${(error as Error).message}`;
    console.error('WASM init failed:', error);
  }
}

// Create a polyphonic synth patch
function createSynthPatch(engine: QuiverEngine) {
  const addModule = (typeId: string, name: string) => {
    engine.add_module(typeId, name);
  };

  const connect = (from: string, to: string) => {
    engine.connect(from, to);
  };

  voices = [];
  for (let i = 0; i < NUM_VOICES; i++) {
    voices.push({ index: i, note: null, active: false });
  }

  // Create voices
  for (let i = 0; i < NUM_VOICES; i++) {
    addModule('vco', `osc_${i}`);
    addModule('svf', `filter_${i}`);
    addModule('adsr', `env_${i}`);
    addModule('vca', `amp_${i}`);
    addModule('offset', `pitch_${i}`);
    addModule('offset', `gate_${i}`);
  }

  // Shared knob modules
  addModule('offset', 'attack_knob');
  addModule('offset', 'decay_knob');
  addModule('offset', 'sustain_knob');
  addModule('offset', 'release_knob');
  addModule('offset', 'pw_knob');
  addModule('offset', 'detune_knob');
  addModule('offset', 'cutoff_knob');
  addModule('offset', 'resonance_knob');
  addModule('mixer', 'voice_mixer');
  addModule('stereo_output', 'out');

  // Connect each voice
  for (let i = 0; i < NUM_VOICES; i++) {
    connect(`pitch_${i}.out`, `osc_${i}.voct`);
    connect(`gate_${i}.out`, `env_${i}.gate`);
    connect('pw_knob.out', `osc_${i}.pw`);
    connect('detune_knob.out', `osc_${i}.fm`);
    connect('attack_knob.out', `env_${i}.attack`);
    connect('decay_knob.out', `env_${i}.decay`);
    connect('sustain_knob.out', `env_${i}.sustain`);
    connect('release_knob.out', `env_${i}.release`);
    connect(`osc_${i}.saw`, `filter_${i}.in`);
    connect(`filter_${i}.lp`, `amp_${i}.in`);
    connect('cutoff_knob.out', `filter_${i}.cutoff`);
    connect('resonance_knob.out', `filter_${i}.res`);
    connect(`env_${i}.env`, `amp_${i}.cv`);
    connect(`amp_${i}.out`, `voice_mixer.ch${i}`);
  }

  connect('voice_mixer.out', 'out.left');
  connect('voice_mixer.out', 'out.right');

  // Set initial knob values
  const msToCV = (ms: number) => Math.log10(Math.max(1, ms)) / 4;
  const hzToCV = (hz: number) => Math.log10(hz / 20) / 3;

  engine.set_param('attack_knob', 0, msToCV(10));
  engine.set_param('decay_knob', 0, msToCV(200));
  engine.set_param('sustain_knob', 0, 0.5);
  engine.set_param('release_knob', 0, msToCV(300));
  engine.set_param('pw_knob', 0, 0.5);
  engine.set_param('detune_knob', 0, 0.0);
  engine.set_param('cutoff_knob', 0, hzToCV(2000));
  engine.set_param('resonance_knob', 0, 0.3);

  for (let i = 0; i < NUM_VOICES; i++) {
    engine.set_param(`pitch_${i}`, 0, 0.0);
    engine.set_param(`gate_${i}`, 0, 0.0);
  }

  engine.set_output('out');
  engine.compile();
  engine.reset();

  // Update stats
  const statModules = document.getElementById('statModules');
  const statCables = document.getElementById('statCables');
  if (statModules) statModules.textContent = String(engine.module_count());
  if (statCables) statCables.textContent = String(engine.cable_count());
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
      const scale = volume / 5.0 / Math.sqrt(NUM_VOICES);

      let maxL = 0;
      let maxR = 0;

      for (let i = 0; i < 512; i++) {
        const left = samples[i * 2] * scale;
        const right = samples[i * 2 + 1] * scale;
        leftOut[i] = left;
        rightOut[i] = right;

        // Update scope buffers
        scopeBufferL[scopeIndex] = left;
        scopeBufferR[scopeIndex] = right;
        scopeIndex = (scopeIndex + 1) % scopeBufferL.length;

        // Track peaks
        maxL = Math.max(maxL, Math.abs(left));
        maxR = Math.max(maxR, Math.abs(right));
      }

      // Update peak with decay
      peakL = Math.max(maxL, peakL * peakDecay);
      peakR = Math.max(maxR, peakR * peakDecay);

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

    // Create analyser for spectrum
    analyserNode = audioContext.createAnalyser();
    analyserNode.fftSize = FFT_SIZE;
    analyserNode.smoothingTimeConstant = 0.8;
    frequencyData = new Uint8Array(analyserNode.frequencyBinCount);

    engine = new QuiverEngine(audioContext.sampleRate);
    createSynthPatch(engine);

    workletNode = createScriptProcessor(audioContext);
    workletNode.connect(analyserNode);
    analyserNode.connect(audioContext.destination);

    isRunning = true;
    startBtn.textContent = 'Audio Active';
    startBtn.disabled = true;
    statusEl.className = 'running';
    statusEl.textContent = `Active // ${NUM_VOICES} Voice Polyphony`;

    // Setup canvases and start animation
    setupCanvas(scopeCanvas, scopeCtx);
    setupCanvas(spectrumCanvas, spectrumCtx);

    requestAnimationFrame(drawVisualizations);
  } catch (error) {
    statusEl.className = 'error';
    const errorMsg = error instanceof Error ? error.message : String(error);
    statusEl.textContent = `Error: ${errorMsg}`;
    console.error('Failed to start audio:', error);
  }
}

// Create gradient for waveform
function createWaveGradient(ctx: CanvasRenderingContext2D, width: number, height: number) {
  const gradient = ctx.createLinearGradient(0, 0, width, 0);
  gradient.addColorStop(0, '#00f5ff');
  gradient.addColorStop(0.5, '#bf00ff');
  gradient.addColorStop(1, '#ff00ff');
  return gradient;
}

// Draw scope visualization
function drawScope(ctx: CanvasRenderingContext2D, width: number, height: number) {
  ctx.clearRect(0, 0, width, height);

  // Draw grid
  ctx.strokeStyle = 'rgba(0, 245, 255, 0.1)';
  ctx.lineWidth = 1;
  for (let i = 0; i <= 4; i++) {
    const y = (height / 4) * i;
    ctx.beginPath();
    ctx.moveTo(0, y);
    ctx.lineTo(width, y);
    ctx.stroke();
  }
  for (let i = 0; i <= 8; i++) {
    const x = (width / 8) * i;
    ctx.beginPath();
    ctx.moveTo(x, 0);
    ctx.lineTo(x, height);
    ctx.stroke();
  }

  // Draw center line with glow
  ctx.strokeStyle = 'rgba(0, 245, 255, 0.3)';
  ctx.lineWidth = 1;
  ctx.beginPath();
  ctx.moveTo(0, height / 2);
  ctx.lineTo(width, height / 2);
  ctx.stroke();

  // Draw waveform with glow effect
  ctx.strokeStyle = createWaveGradient(ctx, width, height);
  ctx.lineWidth = 2;
  ctx.shadowColor = '#00f5ff';
  ctx.shadowBlur = 15;
  ctx.beginPath();

  const sliceWidth = width / scopeBufferL.length;
  let x = 0;

  for (let i = 0; i < scopeBufferL.length; i++) {
    const v = scopeBufferL[(scopeIndex + i) % scopeBufferL.length];
    const y = (1 - v) * height / 2;

    if (i === 0) {
      ctx.moveTo(x, y);
    } else {
      ctx.lineTo(x, y);
    }
    x += sliceWidth;
  }

  ctx.stroke();
  ctx.shadowBlur = 0;
}

// Draw Lissajous (XY) visualization
function drawLissajous(ctx: CanvasRenderingContext2D, width: number, height: number) {
  ctx.clearRect(0, 0, width, height);

  // Draw crosshair
  ctx.strokeStyle = 'rgba(0, 245, 255, 0.2)';
  ctx.lineWidth = 1;
  ctx.beginPath();
  ctx.moveTo(width / 2, 0);
  ctx.lineTo(width / 2, height);
  ctx.moveTo(0, height / 2);
  ctx.lineTo(width, height / 2);
  ctx.stroke();

  // Draw circular grid
  ctx.beginPath();
  ctx.arc(width / 2, height / 2, Math.min(width, height) / 2 - 10, 0, Math.PI * 2);
  ctx.stroke();

  // Draw XY pattern
  ctx.strokeStyle = createWaveGradient(ctx, width, height);
  ctx.lineWidth = 2;
  ctx.shadowColor = '#ff00ff';
  ctx.shadowBlur = 10;
  ctx.beginPath();

  const centerX = width / 2;
  const centerY = height / 2;
  const scale = Math.min(width, height) / 2 - 20;

  for (let i = 0; i < scopeBufferL.length; i++) {
    const idx = (scopeIndex + i) % scopeBufferL.length;
    const x = centerX + scopeBufferL[idx] * scale;
    const y = centerY - scopeBufferR[idx] * scale;

    if (i === 0) {
      ctx.moveTo(x, y);
    } else {
      ctx.lineTo(x, y);
    }
  }

  ctx.stroke();
  ctx.shadowBlur = 0;
}

// Draw bars visualization
function drawBars(ctx: CanvasRenderingContext2D, width: number, height: number) {
  ctx.clearRect(0, 0, width, height);

  const barCount = 64;
  const barWidth = width / barCount - 2;
  const samplesPerBar = Math.floor(scopeBufferL.length / barCount);

  for (let i = 0; i < barCount; i++) {
    let sum = 0;
    for (let j = 0; j < samplesPerBar; j++) {
      const idx = (scopeIndex + i * samplesPerBar + j) % scopeBufferL.length;
      sum += Math.abs(scopeBufferL[idx]);
    }
    const avg = sum / samplesPerBar;
    const barHeight = avg * height * 2;

    // Create gradient for each bar
    const gradient = ctx.createLinearGradient(0, height, 0, height - barHeight);
    gradient.addColorStop(0, '#00f5ff');
    gradient.addColorStop(0.5, '#bf00ff');
    gradient.addColorStop(1, '#ff00ff');

    ctx.fillStyle = gradient;
    ctx.shadowColor = '#00f5ff';
    ctx.shadowBlur = 8;

    const x = i * (barWidth + 2);
    ctx.fillRect(x, height - barHeight, barWidth, barHeight);

    // Top glow cap
    ctx.fillStyle = '#ffffff';
    ctx.fillRect(x, height - barHeight - 2, barWidth, 2);
  }

  ctx.shadowBlur = 0;
}

// Draw spectrum analyzer
function drawSpectrum(ctx: CanvasRenderingContext2D, width: number, height: number) {
  ctx.clearRect(0, 0, width, height);

  if (analyserNode) {
    analyserNode.getByteFrequencyData(frequencyData);
  }

  const barCount = frequencyData.length;
  const barWidth = width / barCount;

  // Draw frequency bars
  for (let i = 0; i < barCount; i++) {
    const value = frequencyData[i] / 255;
    const barHeight = value * height;

    // Gradient from cyan to magenta based on frequency
    const hue = 180 + (i / barCount) * 120; // cyan to magenta
    ctx.fillStyle = `hsla(${hue}, 100%, 60%, 0.8)`;

    const x = i * barWidth;
    ctx.fillRect(x, height - barHeight, barWidth - 1, barHeight);

    // Top glow
    if (value > 0.1) {
      ctx.fillStyle = `hsla(${hue}, 100%, 80%, ${value})`;
      ctx.fillRect(x, height - barHeight, barWidth - 1, 2);
    }
  }

  // Draw frequency labels
  ctx.fillStyle = 'rgba(0, 245, 255, 0.5)';
  ctx.font = '10px JetBrains Mono';
  ctx.textAlign = 'center';
  const labels = ['100', '1k', '5k', '10k', '20k'];
  const positions = [0.05, 0.15, 0.35, 0.55, 0.9];
  for (let i = 0; i < labels.length; i++) {
    ctx.fillText(labels[i], width * positions[i], height - 5);
  }
}

// Update VU meters
function updateVUMeters() {
  const vuLeft = document.getElementById('vuLeft');
  const vuRight = document.getElementById('vuRight');
  const ledsLeft = document.getElementById('ledsLeft');
  const ledsRight = document.getElementById('ledsRight');

  if (vuLeft) vuLeft.style.width = `${Math.min(peakL * 100, 100)}%`;
  if (vuRight) vuRight.style.width = `${Math.min(peakR * 100, 100)}%`;

  // Update LED indicators
  const updateLeds = (container: HTMLElement | null, peak: number) => {
    if (!container) return;
    const leds = container.querySelectorAll('.vu-led');
    const activeLeds = Math.floor(peak * leds.length);
    leds.forEach((led, i) => {
      led.classList.toggle('active', i < activeLeds);
    });
  };

  updateLeds(ledsLeft, peakL);
  updateLeds(ledsRight, peakR);
}

// Update voice indicators
function updateVoiceIndicators() {
  const container = document.getElementById('voiceIndicators');
  if (!container) return;

  const indicators = container.querySelectorAll('.voice-indicator');
  voices.forEach((voice, i) => {
    if (indicators[i]) {
      indicators[i].classList.toggle('active', voice.active);
    }
  });
}

// Main visualization loop
function drawVisualizations() {
  if (!isRunning) return;

  const scopeRect = scopeCanvas.getBoundingClientRect();
  const spectrumRect = spectrumCanvas.getBoundingClientRect();

  // Draw main visualization based on mode
  switch (visualizationMode) {
    case 'scope':
      drawScope(scopeCtx, scopeRect.width, scopeRect.height);
      break;
    case 'lissajous':
      drawLissajous(scopeCtx, scopeRect.width, scopeRect.height);
      break;
    case 'bars':
      drawBars(scopeCtx, scopeRect.width, scopeRect.height);
      break;
  }

  // Draw spectrum
  drawSpectrum(spectrumCtx, spectrumRect.width, spectrumRect.height);

  // Update meters and indicators
  updateVUMeters();
  updateVoiceIndicators();

  requestAnimationFrame(drawVisualizations);
}

// Note on/off handlers
function noteOn(note: number) {
  if (!engine || noteToVoice.has(note)) return;

  const voiceIndex = allocateVoice(note);
  const voice = voices[voiceIndex];

  if (voice.note !== null) {
    noteToVoice.delete(voice.note);
  }

  voice.note = note;
  voice.active = true;
  noteToVoice.set(note, voiceIndex);

  const vOct = (note - 60) / 12.0;
  engine.set_param(`pitch_${voiceIndex}`, 0, vOct);
  engine.set_param(`gate_${voiceIndex}`, 0, 5.0);

  const key = document.querySelector(`[data-note="${note}"]`);
  if (key) key.classList.add('active');
}

function noteOff(note: number) {
  if (!engine) return;

  const voiceIndex = noteToVoice.get(note);
  if (voiceIndex === undefined) return;

  const voice = voices[voiceIndex];
  engine.set_param(`gate_${voiceIndex}`, 0, 0.0);

  voice.active = false;
  voice.note = null;
  noteToVoice.delete(note);

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

  const keyboard = document.getElementById('keyboard')!;
  keyboard.addEventListener('mousedown', (e) => {
    const key = (e.target as HTMLElement).closest('.key');
    if (key) {
      const note = parseInt(key.getAttribute('data-note')!);
      noteOn(note);
    }
  });

  document.addEventListener('mouseup', () => {
    for (const [note] of noteToVoice) {
      noteOff(note);
    }
  });
}

// Setup visualization mode buttons
function setupVisualizationModes() {
  const buttons = document.querySelectorAll('.viz-mode-btn');
  buttons.forEach(btn => {
    btn.addEventListener('click', () => {
      buttons.forEach(b => b.classList.remove('active'));
      btn.classList.add('active');
      visualizationMode = btn.getAttribute('data-mode') as typeof visualizationMode;
    });
  });
}

// Format value displays
function formatValue(id: string, value: number): string {
  switch (id) {
    case 'pulseWidth':
      return `${Math.round(value * 100)}%`;
    case 'detune':
      return `${value > 0 ? '+' : ''}${Math.round(value)}ct`;
    case 'cutoff':
      return value >= 1000 ? `${(value / 1000).toFixed(1)}k` : `${Math.round(value)}`;
    case 'resonance':
      return `${Math.round(value * 100)}%`;
    case 'filterEnv':
      return value >= 1000 ? `${(value / 1000).toFixed(1)}k` : `${Math.round(value)}`;
    case 'attack':
    case 'decay':
    case 'release':
      return `${Math.round(value)}ms`;
    case 'sustain':
      return `${Math.round(value * 100)}%`;
    case 'volume':
      return `${Math.round(value * 100)}%`;
    default:
      return String(value);
  }
}

// Setup parameter controls
function setupControls() {
  const bindSlider = (id: string, callback: (value: number) => void) => {
    const slider = document.getElementById(id) as HTMLInputElement;
    const valueEl = document.getElementById(id + 'Value');

    const update = () => {
      const value = parseFloat(slider.value);
      if (valueEl) {
        valueEl.textContent = formatValue(id, value);
      }
      callback(value);
    };

    slider.addEventListener('input', update);
    update();
  };

  bindSlider('volume', () => {});

  const waveformSelect = document.getElementById('waveform') as HTMLSelectElement;
  waveformSelect.addEventListener('change', () => {
    switchWaveform(waveformSelect.value);
  });

  const msToCV = (ms: number) => Math.log10(Math.max(1, ms)) / 4;
  const hzToCV = (hz: number) => Math.log10(hz / 20) / 3;

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
  bindSlider('pulseWidth', (v) => {
    if (engine) engine.set_param('pw_knob', 0, v);
  });
  bindSlider('detune', (cents) => {
    const vOct = cents / 1200;
    if (engine) engine.set_param('detune_knob', 0, vOct);
  });
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

// Handle window resize
function handleResize() {
  if (scopeCanvas && scopeCtx) {
    setupCanvas(scopeCanvas, scopeCtx);
  }
  if (spectrumCanvas && spectrumCtx) {
    setupCanvas(spectrumCanvas, spectrumCtx);
  }
}

// Initialize
async function main() {
  console.log('Initializing Quiver Browser Synth...');
  await initWasm();
  setupKeyboard();
  setupVisualizationModes();
  setupControls();
  startBtn.addEventListener('click', startAudio);
  window.addEventListener('resize', handleResize);

  // Initial canvas setup
  handleResize();
}

main();
