/**
 * @quiver/types - TypeScript type definitions for Quiver modular synthesizer
 *
 * This package provides type definitions that match the Rust serialization format,
 * enabling type-safe integration between the Quiver audio engine and frontend UIs.
 *
 * ## Type Sources
 *
 * These types are manually maintained to match the Rust types in:
 * - src/serialize.rs (PatchDef, ModuleDef, CableDef, CatalogResponse, ValidationResult)
 * - src/port.rs (SignalKind, PortDef, PortSpec, PortInfo, Compatibility)
 * - src/introspection.rs (ParamInfo, ValueFormat, ParamCurve, ControlType)
 * - src/observer.rs (ObservableValue, SubscriptionTarget)
 *
 * The Rust types use `#[cfg_attr(feature = "wasm", derive(tsify::Tsify))]` to generate
 * corresponding TypeScript types when building @quiver/wasm. This package provides
 * these types standalone for use without requiring the WASM build.
 *
 * ## Synchronization
 *
 * To verify types are in sync with Rust, run:
 * ```bash
 * pnpm run typecheck
 * ```
 *
 * The CI pipeline validates type compatibility by building both packages.
 */

// =============================================================================
// Core Patch Types
// =============================================================================

/**
 * Complete patch definition for serialization
 * Corresponds to Rust: PatchDef in src/serialize.rs
 */
export interface PatchDef {
  /** Schema version for forward compatibility (currently 1) */
  version: number;

  /** Patch name */
  name: string;

  /** Optional author name */
  author?: string;

  /** Optional patch description */
  description?: string;

  /** Tags for categorization and search */
  tags: string[];

  /** Module instances in the patch */
  modules: ModuleDef[];

  /** Cable connections between modules */
  cables: CableDef[];

  /** Parameter values keyed by 'module_name.param_id' */
  parameters: Record<string, number>;
}

/**
 * Module instance definition
 * Corresponds to Rust: ModuleDef in src/serialize.rs
 */
export interface ModuleDef {
  /** Unique instance name within the patch */
  name: string;

  /** Module type identifier from the registry */
  module_type: ModuleTypeId;

  /** UI position as [x, y] coordinates */
  position?: [number, number];

  /** Module-specific state for serialization */
  state?: Record<string, unknown>;
}

/**
 * Cable connection definition
 * Corresponds to Rust: CableDef in src/serialize.rs
 */
export interface CableDef {
  /** Source port reference as 'module_name.port_name' */
  from: PortReference;

  /** Destination port reference as 'module_name.port_name' */
  to: PortReference;

  /** Optional attenuation/gain (-2.0 to 2.0, unity = 1.0) */
  attenuation?: number;

  /** Optional DC offset in volts (-10.0 to 10.0V) */
  offset?: number;
}

/**
 * Port reference string in format 'module_name.port_name'
 */
export type PortReference = `${string}.${string}`;

// =============================================================================
// Signal Types
// =============================================================================

/**
 * Semantic signal classification following hardware modular conventions
 * Corresponds to Rust: SignalKind in src/port.rs
 */
export type SignalKind =
  | 'audio'
  | 'cv_bipolar'
  | 'cv_unipolar'
  | 'volt_per_octave'
  | 'gate'
  | 'trigger'
  | 'clock';

/**
 * Signal kind metadata with voltage ranges and behaviors
 */
export interface SignalKindInfo {
  kind: SignalKind;
  /** Voltage range as [min, max] */
  voltageRange: [number, number];
  /** Whether multiple signals should be summed when connected */
  isSummable: boolean;
  /** Threshold voltage for high/low detection (for gate-like signals) */
  gateThreshold?: number;
}

/**
 * Complete signal kind information
 */
export const SIGNAL_KINDS: Record<SignalKind, SignalKindInfo> = {
  audio: {
    kind: 'audio',
    voltageRange: [-5.0, 5.0],
    isSummable: true,
  },
  cv_bipolar: {
    kind: 'cv_bipolar',
    voltageRange: [-5.0, 5.0],
    isSummable: true,
  },
  cv_unipolar: {
    kind: 'cv_unipolar',
    voltageRange: [0.0, 10.0],
    isSummable: true,
  },
  volt_per_octave: {
    kind: 'volt_per_octave',
    voltageRange: [-5.0, 5.0],
    isSummable: true,
  },
  gate: {
    kind: 'gate',
    voltageRange: [0.0, 5.0],
    isSummable: false,
    gateThreshold: 2.5,
  },
  trigger: {
    kind: 'trigger',
    voltageRange: [0.0, 5.0],
    isSummable: false,
    gateThreshold: 2.5,
  },
  clock: {
    kind: 'clock',
    voltageRange: [0.0, 5.0],
    isSummable: false,
    gateThreshold: 2.5,
  },
};

// =============================================================================
// Port Types
// =============================================================================

/**
 * Port definition
 * Corresponds to Rust: PortDef in src/port.rs
 */
export interface PortDef {
  /** Unique identifier within the module */
  id: number;

  /** Human-readable name (e.g., 'cutoff', 'voct', 'out') */
  name: string;

  /** Signal type for validation and UI hints */
  kind: SignalKind;

  /** Default value when no cable is connected */
  default: number;

  /** Port ID this input is normalled to when unpatched */
  normalled_to?: number;

  /** Whether this input has an associated attenuverter control */
  has_attenuverter: boolean;
}

/**
 * Enhanced port information for GUI display
 * Corresponds to Rust: PortInfo in src/port.rs
 */
export interface PortInfo {
  /** Unique identifier within the module */
  id: number;

  /** Human-readable name */
  name: string;

  /** Signal type */
  kind: SignalKind;

  /** Port this is normalled to (by name, for UI display) */
  normalled_to?: string;

  /** Optional description for tooltips */
  description?: string;
}

/**
 * Convert a PortDef to PortInfo
 */
export function portDefToInfo(def: PortDef): PortInfo {
  return {
    id: def.id,
    name: def.name,
    kind: def.kind,
    normalled_to: undefined, // PortDef uses ID, PortInfo uses name
    description: undefined,
  };
}

/**
 * Specification of all ports for a module
 * Corresponds to Rust: PortSpec in src/port.rs
 */
export interface PortSpec {
  inputs: PortDef[];
  outputs: PortDef[];
}

// =============================================================================
// Introspection API (Phase 1)
// =============================================================================

/**
 * How to format parameter values for display
 * Corresponds to Rust: ValueFormat in src/introspection.rs
 */
export type ValueFormat =
  | { type: 'decimal'; places: number }
  | { type: 'frequency' }
  | { type: 'time' }
  | { type: 'decibels' }
  | { type: 'percent' }
  | { type: 'note_name' }
  | { type: 'ratio' };

/**
 * How parameter values are scaled between min and max
 * Corresponds to Rust: ParamCurve in src/introspection.rs
 */
export type ParamCurve =
  | { type: 'linear' }
  | { type: 'exponential' }
  | { type: 'logarithmic' }
  | { type: 'stepped'; steps: number };

/**
 * Suggested UI control type for a parameter
 * Corresponds to Rust: ControlType in src/introspection.rs
 */
export type ControlType = 'knob' | 'slider' | 'toggle' | 'select';

/**
 * Complete parameter descriptor for UI generation
 * Corresponds to Rust: ParamInfo in src/introspection.rs
 */
export interface ParamInfo {
  /** Unique identifier within module (e.g., "frequency", "resonance") */
  id: string;
  /** Display name (e.g., "Frequency", "Resonance") */
  name: string;
  /** Current value */
  value: number;
  /** Minimum value */
  min: number;
  /** Maximum value */
  max: number;
  /** Default value */
  default: number;
  /** Value scaling curve */
  curve: ParamCurve;
  /** Suggested control type */
  control: ControlType;
  /** Unit for display (Hz, ms, dB, %, etc.) */
  unit?: string;
  /** Value formatting hint */
  format: ValueFormat;
}

/**
 * Create a frequency parameter preset
 */
export function createFrequencyParam(id: string, name: string): ParamInfo {
  return {
    id,
    name,
    value: 1000,
    min: 20,
    max: 20000,
    default: 1000,
    curve: { type: 'exponential' },
    control: 'knob',
    unit: 'Hz',
    format: { type: 'frequency' },
  };
}

/**
 * Create a time parameter preset
 */
export function createTimeParam(id: string, name: string): ParamInfo {
  return {
    id,
    name,
    value: 0.1,
    min: 0.001,
    max: 10,
    default: 0.1,
    curve: { type: 'exponential' },
    control: 'knob',
    unit: 's',
    format: { type: 'time' },
  };
}

/**
 * Create a percentage parameter preset
 */
export function createPercentParam(id: string, name: string): ParamInfo {
  return {
    id,
    name,
    value: 0.5,
    min: 0,
    max: 1,
    default: 0.5,
    curve: { type: 'linear' },
    control: 'knob',
    format: { type: 'percent' },
  };
}

/**
 * Create a toggle parameter preset
 */
export function createToggleParam(id: string, name: string): ParamInfo {
  return {
    id,
    name,
    value: 0,
    min: 0,
    max: 1,
    default: 0,
    curve: { type: 'stepped', steps: 2 },
    control: 'toggle',
    format: { type: 'decimal', places: 0 },
  };
}

/**
 * Create a selector parameter preset
 */
export function createSelectParam(id: string, name: string, options: number): ParamInfo {
  return {
    id,
    name,
    value: 0,
    min: 0,
    max: options - 1,
    default: 0,
    curve: { type: 'stepped', steps: options },
    control: 'select',
    format: { type: 'decimal', places: 0 },
  };
}

/**
 * Format a value according to a ValueFormat specification
 */
export function formatParamValue(value: number, format: ValueFormat): string {
  switch (format.type) {
    case 'decimal':
      return value.toFixed(format.places);
    case 'frequency':
      return value >= 1000 ? `${(value / 1000).toFixed(2)} kHz` : `${value.toFixed(1)} Hz`;
    case 'time':
      return value >= 1 ? `${value.toFixed(2)} s` : `${(value * 1000).toFixed(1)} ms`;
    case 'decibels':
      return `${value.toFixed(1)} dB`;
    case 'percent':
      return `${(value * 100).toFixed(0)}%`;
    case 'note_name': {
      const midiNote = Math.round(value * 12 + 60);
      const noteNames = ['C', 'C#', 'D', 'D#', 'E', 'F', 'F#', 'G', 'G#', 'A', 'A#', 'B'];
      const note = noteNames[((midiNote % 12) + 12) % 12];
      const octave = Math.floor(midiNote / 12) - 1;
      return `${note}${octave}`;
    }
    case 'ratio':
      return value >= 1 ? `${value.toFixed(1)}:1` : value > 0 ? `1:${(1 / value).toFixed(1)}` : '0:1';
  }
}

/**
 * Apply a parameter curve to convert normalized (0-1) to actual value
 */
export function applyParamCurve(normalized: number, min: number, max: number, curve: ParamCurve): number {
  const n = Math.max(0, Math.min(1, normalized));

  switch (curve.type) {
    case 'linear':
      return min + n * (max - min);
    case 'exponential':
      return min <= 0 ? n * max : min * Math.pow(max / min, n);
    case 'logarithmic': {
      const logMin = min > 0 ? Math.log10(min) : 0;
      const logMax = Math.log10(Math.max(max, 0.001));
      return Math.pow(10, logMin + n * (logMax - logMin));
    }
    case 'stepped': {
      const stepSize = (max - min) / curve.steps;
      const stepIndex = Math.min(Math.floor(n * curve.steps), curve.steps - 1);
      return min + stepIndex * stepSize;
    }
  }
}

/**
 * Normalize an actual value to 0-1 based on a parameter curve
 */
export function normalizeParamValue(value: number, min: number, max: number, curve: ParamCurve): number {
  if (Math.abs(max - min) < 1e-10) return 0;

  switch (curve.type) {
    case 'linear':
      return Math.max(0, Math.min(1, (value - min) / (max - min)));
    case 'exponential':
      if (min <= 0 || value <= 0) {
        return Math.max(0, Math.min(1, (value - min) / (max - min)));
      }
      return Math.max(0, Math.min(1, Math.log(value / min) / Math.log(max / min)));
    case 'logarithmic': {
      const logMin = min > 0 ? Math.log10(min) : 0;
      const logMax = Math.log10(Math.max(max, 0.001));
      const logVal = Math.log10(Math.max(value, 0.001));
      return Math.max(0, Math.min(1, (logVal - logMin) / (logMax - logMin)));
    }
    case 'stepped': {
      const stepSize = (max - min) / curve.steps;
      const stepIndex = Math.round((value - min) / stepSize);
      return Math.max(0, Math.min(1, stepIndex / curve.steps));
    }
  }
}

// =============================================================================
// Module Registry Types
// =============================================================================

/**
 * All available module type IDs
 * Corresponds to modules registered in Rust: ModuleRegistry in src/serialize.rs
 */
export type ModuleTypeId =
  // Oscillators
  | 'vco'
  | 'analog_vco'
  | 'lfo'
  | 'supersaw'
  | 'karplus_strong'
  | 'wavetable'
  | 'formant_osc'
  // Filters
  | 'svf'
  | 'diode_ladder'
  // Envelopes
  | 'adsr'
  // Amplifiers & Utilities
  | 'vca'
  | 'mixer'
  | 'mixer8'
  | 'offset'
  | 'unit_delay'
  | 'multiple'
  | 'attenuverter'
  | 'slew_limiter'
  | 'sample_and_hold'
  | 'precision_adder'
  | 'vc_switch'
  | 'min'
  | 'max'
  | 'envelope_follower'
  | 'scale_quantizer'
  | 'quantizer'
  | 'crossfader'
  | 'chord_memory'
  // Sources
  | 'noise'
  // Sequencing
  | 'step_sequencer'
  | 'clock'
  | 'euclidean'
  | 'arpeggiator'
  // Effects
  | 'saturator'
  | 'wavefolder'
  | 'ring_mod'
  | 'rectifier'
  | 'delay_line'
  | 'chorus'
  | 'flanger'
  | 'phaser'
  | 'tremolo'
  | 'vibrato'
  | 'distortion'
  | 'bitcrusher'
  | 'reverb'
  | 'parametric_eq'
  | 'vocoder'
  | 'pitch_shifter'
  | 'granular'
  // Dynamics
  | 'limiter'
  | 'noise_gate'
  | 'compressor'
  // Analog Modeling
  | 'crosstalk'
  | 'ground_loop'
  // Logic
  | 'logic_and'
  | 'logic_or'
  | 'logic_xor'
  | 'logic_not'
  | 'comparator'
  // Random
  | 'bernoulli_gate'
  // I/O
  | 'stereo_output';

/**
 * Module category for grouping in UI
 * Corresponds to categories in Rust: ModuleRegistry in src/serialize.rs
 */
export type ModuleCategory =
  | 'Oscillators'
  | 'Filters'
  | 'Envelopes'
  | 'Modulation'
  | 'Utilities'
  | 'Sources'
  | 'Sequencers'
  | 'Sequencing' // Alias for compatibility
  | 'Effects'
  | 'Dynamics'
  | 'Logic'
  | 'Random'
  | 'Analog Modeling'
  | 'I/O';

/**
 * Module metadata for the catalog
 * Corresponds to Rust: ModuleMetadata in src/serialize.rs
 */
export interface ModuleMetadata {
  /** Module type identifier */
  type_id: ModuleTypeId;

  /** Human-readable display name */
  name: string;

  /** Category for grouping */
  category: ModuleCategory;

  /** Description of what the module does */
  description: string;

  /** Port specification */
  port_spec: PortSpec;

  /** Keywords for search functionality */
  keywords: string[];

  /** Tags for filtering (e.g., "essential", "advanced", "analog") */
  tags: string[];
}

// =============================================================================
// Module Catalog Types (Phase 3: GUI Framework)
// =============================================================================

/**
 * Summary of a module's port configuration for the catalog UI
 * Corresponds to Rust: PortSummary in src/serialize.rs
 */
export interface PortSummary {
  /** Number of input ports */
  inputs: number;

  /** Number of output ports */
  outputs: number;

  /** Whether the module has audio input(s) */
  has_audio_in: boolean;

  /** Whether the module has audio output(s) */
  has_audio_out: boolean;
}

/**
 * A catalog entry for the "add module" UI
 * Corresponds to Rust: ModuleCatalogEntry in src/serialize.rs
 */
export interface ModuleCatalogEntry {
  /** Module type identifier (e.g., "vco", "svf") */
  type_id: ModuleTypeId;

  /** Human-readable name (e.g., "VCO", "State Variable Filter") */
  name: string;

  /** Category for grouping (e.g., "Oscillators", "Filters") */
  category: ModuleCategory;

  /** Longer description for tooltips/help */
  description: string;

  /** Search keywords (e.g., ["oscillator", "sine", "saw", "pulse"]) */
  keywords: string[];

  /** Port configuration summary */
  ports: PortSummary;

  /** Tags for filtering (e.g., ["essential", "advanced", "analog"]) */
  tags: string[];
}

/**
 * Response from catalog() containing all modules and categories
 * Corresponds to Rust: CatalogResponse in src/serialize.rs
 */
export interface CatalogResponse {
  /** All available modules */
  modules: ModuleCatalogEntry[];

  /** All unique categories (sorted) */
  categories: ModuleCategory[];
}

/**
 * Create a PortSummary from a PortSpec
 */
export function createPortSummary(spec: PortSpec): PortSummary {
  return {
    inputs: spec.inputs.length,
    outputs: spec.outputs.length,
    has_audio_in: spec.inputs.some((p) => p.kind === 'audio'),
    has_audio_out: spec.outputs.some((p) => p.kind === 'audio'),
  };
}

/**
 * Search modules by query string (client-side implementation)
 * Matches against type_id, name, description, and keywords (case-insensitive)
 */
export function searchModules(
  modules: ModuleCatalogEntry[],
  query: string
): ModuleCatalogEntry[] {
  const q = query.toLowerCase();

  // Score and sort results
  const scored = modules
    .map((m) => {
      let score = 0;

      // Exact type_id match
      if (m.type_id.toLowerCase() === q) score = 100;
      // Exact name match
      else if (m.name.toLowerCase() === q) score = 90;
      // type_id contains query
      else if (m.type_id.toLowerCase().includes(q)) score = 70;
      // name contains query
      else if (m.name.toLowerCase().includes(q)) score = 60;
      // keyword exact match
      else if (m.keywords.some((k) => k.toLowerCase() === q)) score = 50;
      // keyword contains query
      else if (m.keywords.some((k) => k.toLowerCase().includes(q))) score = 40;
      // description contains query
      else if (m.description.toLowerCase().includes(q)) score = 20;
      // category contains query
      else if (m.category.toLowerCase().includes(q)) score = 10;

      return { module: m, score };
    })
    .filter((item) => item.score > 0)
    .sort((a, b) => b.score - a.score || a.module.name.localeCompare(b.module.name));

  return scored.map((item) => item.module);
}

/**
 * Filter modules by category
 */
export function filterByCategory(
  modules: ModuleCatalogEntry[],
  category: ModuleCategory
): ModuleCatalogEntry[] {
  return modules
    .filter((m) => m.category === category)
    .sort((a, b) => a.name.localeCompare(b.name));
}

/**
 * Filter modules by tag
 */
export function filterByTag(
  modules: ModuleCatalogEntry[],
  tag: string
): ModuleCatalogEntry[] {
  return modules.filter((m) => m.tags.includes(tag));
}

/**
 * Get all unique categories from modules
 */
export function getCategories(modules: ModuleCatalogEntry[]): ModuleCategory[] {
  const categories = new Set<ModuleCategory>();
  for (const m of modules) {
    categories.add(m.category);
  }
  return Array.from(categories).sort();
}

// =============================================================================
// Signal Colors (for cable coloring)
// =============================================================================

/**
 * CSS hex color values for each signal type
 */
export interface SignalColors {
  audio: string;
  cv_bipolar: string;
  cv_unipolar: string;
  volt_per_octave: string;
  gate: string;
  trigger: string;
  clock: string;
}

/**
 * Default signal colors following modular synth conventions
 */
export const DEFAULT_SIGNAL_COLORS: SignalColors = {
  audio: '#e94560', // Red - audio signals
  cv_bipolar: '#0f3460', // Dark blue - bipolar CV
  cv_unipolar: '#00b4d8', // Cyan - unipolar CV
  volt_per_octave: '#90be6d', // Green - pitch CV
  gate: '#f9c74f', // Yellow - gates
  trigger: '#f8961e', // Orange - triggers
  clock: '#9d4edd', // Purple - clock
};

/**
 * Get the color for a signal kind
 */
export function getSignalColor(
  kind: SignalKind,
  colors: SignalColors = DEFAULT_SIGNAL_COLORS
): string {
  return colors[kind];
}

// =============================================================================
// Port Compatibility
// =============================================================================

/**
 * Compatibility status for port connections
 */
export type Compatibility =
  | { status: 'exact' }
  | { status: 'allowed' }
  | { status: 'warning'; message: string };

/**
 * Check if two signal kinds are compatible for connection
 */
export function checkPortCompatibility(
  from: SignalKind,
  to: SignalKind
): Compatibility {
  // Exact match
  if (from === to) {
    return { status: 'exact' };
  }

  // Audio can go anywhere
  if (from === 'audio') {
    return { status: 'allowed' };
  }

  // CV interoperability
  if (
    (from === 'cv_bipolar' && to === 'cv_unipolar') ||
    (from === 'cv_unipolar' && to === 'cv_bipolar')
  ) {
    return { status: 'allowed' };
  }

  // V/Oct to CV is allowed
  if (
    from === 'volt_per_octave' &&
    (to === 'cv_bipolar' || to === 'cv_unipolar')
  ) {
    return { status: 'allowed' };
  }

  // Gate/Trigger/Clock interoperability
  if (
    (from === 'gate' && to === 'trigger') ||
    (from === 'trigger' && to === 'gate')
  ) {
    return { status: 'allowed' };
  }

  if (
    (from === 'clock' && to === 'gate') ||
    (from === 'clock' && to === 'trigger')
  ) {
    return { status: 'allowed' };
  }

  // Warnings for potentially problematic connections
  if ((from === 'gate' || from === 'trigger') && to === 'audio') {
    return { status: 'warning', message: 'Gate/Trigger to Audio may cause clicks' };
  }

  if (from === 'cv_bipolar' && to === 'volt_per_octave') {
    return { status: 'warning', message: 'CV to V/Oct may cause tuning issues' };
  }

  // Default: allow but could be unusual
  return { status: 'allowed' };
}

// =============================================================================
// Utility Functions
// =============================================================================

/**
 * Parse a port reference string into module name and port name
 */
export function parsePortReference(ref: PortReference): {
  moduleName: string;
  portName: string;
} {
  const parts = ref.split('.');
  if (parts.length !== 2) {
    throw new Error(`Invalid port reference: ${ref}`);
  }
  return {
    moduleName: parts[0],
    portName: parts[1],
  };
}

/**
 * Create a port reference string from module and port names
 */
export function createPortReference(
  moduleName: string,
  portName: string
): PortReference {
  return `${moduleName}.${portName}` as PortReference;
}

/**
 * Create a new empty patch definition
 */
export function createPatchDef(name: string): PatchDef {
  return {
    version: 1,
    name,
    tags: [],
    modules: [],
    cables: [],
    parameters: {},
  };
}

/**
 * Create a new module definition
 */
export function createModuleDef(
  name: string,
  moduleType: ModuleTypeId,
  position?: [number, number]
): ModuleDef {
  return {
    name,
    module_type: moduleType,
    position,
  };
}

/**
 * Create a new cable definition
 */
export function createCableDef(
  from: PortReference,
  to: PortReference,
  options?: { attenuation?: number; offset?: number }
): CableDef {
  return {
    from,
    to,
    ...options,
  };
}

// =============================================================================
// Validation
// =============================================================================

/**
 * Validation error for patch definitions
 */
export interface ValidationError {
  path: string;
  message: string;
}

/**
 * Validation result
 */
export type ValidationResult =
  | { valid: true }
  | { valid: false; errors: ValidationError[] };

/**
 * Validate a patch definition
 */
export function validatePatchDef(patch: unknown): ValidationResult {
  const errors: ValidationError[] = [];

  if (typeof patch !== 'object' || patch === null) {
    return { valid: false, errors: [{ path: '', message: 'Patch must be an object' }] };
  }

  const p = patch as Record<string, unknown>;

  // Required fields
  if (typeof p.version !== 'number' || p.version < 1) {
    errors.push({ path: 'version', message: 'Version must be a positive integer' });
  }

  if (typeof p.name !== 'string' || p.name.length === 0) {
    errors.push({ path: 'name', message: 'Name must be a non-empty string' });
  }

  if (!Array.isArray(p.modules)) {
    errors.push({ path: 'modules', message: 'Modules must be an array' });
  } else {
    const moduleNames = new Set<string>();
    p.modules.forEach((mod, i) => {
      const modErrors = validateModuleDef(mod, `modules[${i}]`);
      errors.push(...modErrors);

      if (typeof mod === 'object' && mod !== null) {
        const m = mod as Record<string, unknown>;
        if (typeof m.name === 'string') {
          if (moduleNames.has(m.name)) {
            errors.push({
              path: `modules[${i}].name`,
              message: `Duplicate module name: ${m.name}`,
            });
          }
          moduleNames.add(m.name);
        }
      }
    });
  }

  if (!Array.isArray(p.cables)) {
    errors.push({ path: 'cables', message: 'Cables must be an array' });
  } else {
    p.cables.forEach((cable, i) => {
      const cableErrors = validateCableDef(cable, `cables[${i}]`);
      errors.push(...cableErrors);
    });
  }

  if (errors.length > 0) {
    return { valid: false, errors };
  }

  return { valid: true };
}

function validateModuleDef(mod: unknown, path: string): ValidationError[] {
  const errors: ValidationError[] = [];

  if (typeof mod !== 'object' || mod === null) {
    return [{ path, message: 'Module must be an object' }];
  }

  const m = mod as Record<string, unknown>;

  if (typeof m.name !== 'string' || m.name.length === 0) {
    errors.push({ path: `${path}.name`, message: 'Module name must be a non-empty string' });
  }

  if (typeof m.module_type !== 'string') {
    errors.push({ path: `${path}.module_type`, message: 'Module type must be a string' });
  }

  if (m.position !== undefined) {
    if (
      !Array.isArray(m.position) ||
      m.position.length !== 2 ||
      typeof m.position[0] !== 'number' ||
      typeof m.position[1] !== 'number'
    ) {
      errors.push({ path: `${path}.position`, message: 'Position must be a [number, number] tuple' });
    }
  }

  return errors;
}

function validateCableDef(cable: unknown, path: string): ValidationError[] {
  const errors: ValidationError[] = [];

  if (typeof cable !== 'object' || cable === null) {
    return [{ path, message: 'Cable must be an object' }];
  }

  const c = cable as Record<string, unknown>;

  const portRefPattern = /^[a-zA-Z0-9_-]+\.[a-zA-Z0-9_-]+$/;

  if (typeof c.from !== 'string' || !portRefPattern.test(c.from)) {
    errors.push({
      path: `${path}.from`,
      message: "From must be a port reference in format 'module_name.port_name'",
    });
  }

  if (typeof c.to !== 'string' || !portRefPattern.test(c.to)) {
    errors.push({
      path: `${path}.to`,
      message: "To must be a port reference in format 'module_name.port_name'",
    });
  }

  if (c.attenuation !== undefined) {
    if (typeof c.attenuation !== 'number' || c.attenuation < -2.0 || c.attenuation > 2.0) {
      errors.push({
        path: `${path}.attenuation`,
        message: 'Attenuation must be a number between -2.0 and 2.0',
      });
    }
  }

  if (c.offset !== undefined) {
    if (typeof c.offset !== 'number' || c.offset < -10.0 || c.offset > 10.0) {
      errors.push({
        path: `${path}.offset`,
        message: 'Offset must be a number between -10.0 and 10.0',
      });
    }
  }

  return errors;
}

// =============================================================================
// Real-Time State Bridge (Phase 4: GUI Framework)
// =============================================================================

/**
 * Values that can be observed and streamed to the UI
 * Corresponds to Rust: ObservableValue in src/observer.rs
 */
export type ObservableValue =
  | { type: 'param'; node_id: string; param_id: string; value: number }
  | { type: 'level'; node_id: string; port_id: number; rms_db: number; peak_db: number }
  | { type: 'gate'; node_id: string; port_id: number; active: boolean }
  | { type: 'scope'; node_id: string; port_id: number; samples: number[] }
  | { type: 'spectrum'; node_id: string; port_id: number; bins: number[]; freq_range: [number, number] };

/**
 * Subscription target specifying what to observe
 * Corresponds to Rust: SubscriptionTarget in src/observer.rs
 */
export type SubscriptionTarget =
  | { type: 'param'; node_id: string; param_id: string }
  | { type: 'level'; node_id: string; port_id: number }
  | { type: 'gate'; node_id: string; port_id: number }
  | { type: 'scope'; node_id: string; port_id: number; buffer_size: number }
  | { type: 'spectrum'; node_id: string; port_id: number; fft_size: number };

/**
 * Get a unique key for an observable value (for deduplication in UI state)
 */
export function getObservableValueKey(value: ObservableValue): string {
  switch (value.type) {
    case 'param':
      return `param:${value.node_id}:${value.param_id}`;
    case 'level':
      return `level:${value.node_id}:${value.port_id}`;
    case 'gate':
      return `gate:${value.node_id}:${value.port_id}`;
    case 'scope':
      return `scope:${value.node_id}:${value.port_id}`;
    case 'spectrum':
      return `spectrum:${value.node_id}:${value.port_id}`;
  }
}

/**
 * Get a unique key for a subscription target
 */
export function getSubscriptionTargetKey(target: SubscriptionTarget): string {
  switch (target.type) {
    case 'param':
      return `param:${target.node_id}:${target.param_id}`;
    case 'level':
      return `level:${target.node_id}:${target.port_id}`;
    case 'gate':
      return `gate:${target.node_id}:${target.port_id}`;
    case 'scope':
      return `scope:${target.node_id}:${target.port_id}`;
    case 'spectrum':
      return `spectrum:${target.node_id}:${target.port_id}`;
  }
}

/**
 * Create a param subscription target
 */
export function subscribeParam(nodeId: string, paramId: string): SubscriptionTarget {
  return { type: 'param', node_id: nodeId, param_id: paramId };
}

/**
 * Create a level meter subscription target
 */
export function subscribeLevel(nodeId: string, portId: number): SubscriptionTarget {
  return { type: 'level', node_id: nodeId, port_id: portId };
}

/**
 * Create a gate subscription target
 */
export function subscribeGate(nodeId: string, portId: number): SubscriptionTarget {
  return { type: 'gate', node_id: nodeId, port_id: portId };
}

/**
 * Create a scope subscription target
 */
export function subscribeScope(
  nodeId: string,
  portId: number,
  bufferSize: number = 512
): SubscriptionTarget {
  return { type: 'scope', node_id: nodeId, port_id: portId, buffer_size: bufferSize };
}

/**
 * Create a spectrum analyzer subscription target
 */
export function subscribeSpectrum(
  nodeId: string,
  portId: number,
  fftSize: number = 1024
): SubscriptionTarget {
  return { type: 'spectrum', node_id: nodeId, port_id: portId, fft_size: fftSize };
}

/**
 * Configuration for the state observer
 */
export interface ObserverConfig {
  /** Maximum updates per second (default: 60) */
  maxUpdateRate: number;
  /** Maximum pending updates before oldest are dropped (default: 1000) */
  maxPendingUpdates: number;
  /** Default scope buffer size (default: 512) */
  defaultScopeBufferSize: number;
  /** Default FFT size for spectrum analysis (default: 1024) */
  defaultFftSize: number;
}

/**
 * Default observer configuration
 */
export const DEFAULT_OBSERVER_CONFIG: ObserverConfig = {
  maxUpdateRate: 60,
  maxPendingUpdates: 1000,
  defaultScopeBufferSize: 512,
  defaultFftSize: 1024,
};

/**
 * Bridge interface for both WASM and HTTP backends
 */
export interface QuiverBridge {
  /** Subscribe to real-time values */
  subscribe(targets: SubscriptionTarget[]): void;

  /** Unsubscribe from specific targets */
  unsubscribe(targetKeys: string[]): void;

  /** Poll for pending updates (WASM) or register callback (HTTP) */
  onUpdate(callback: (updates: ObservableValue[]) => void): () => void;
}

/**
 * Calculate RMS level in decibels from samples
 */
export function calculateRmsDb(samples: number[]): number {
  if (samples.length === 0) return -Infinity;

  const sumSq = samples.reduce((sum, s) => sum + s * s, 0);
  const rms = Math.sqrt(sumSq / samples.length);

  return rms > 0 ? 20 * Math.log10(rms) : -Infinity;
}

/**
 * Calculate peak level in decibels from samples
 */
export function calculatePeakDb(samples: number[]): number {
  if (samples.length === 0) return -Infinity;

  const peak = samples.reduce((max, s) => Math.max(max, Math.abs(s)), 0);

  return peak > 0 ? 20 * Math.log10(peak) : -Infinity;
}

/**
 * Format decibels for display
 */
export function formatDb(db: number): string {
  if (!isFinite(db)) return '-∞ dB';
  return `${db.toFixed(1)} dB`;
}
