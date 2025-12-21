/**
 * @quiver/types - TypeScript type definitions for Quiver modular synthesizer
 *
 * This package provides type definitions that match the Rust serialization format,
 * enabling type-safe integration between the Quiver audio engine and frontend UIs.
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
 * Specification of all ports for a module
 * Corresponds to Rust: PortSpec in src/port.rs
 */
export interface PortSpec {
  inputs: PortDef[];
  outputs: PortDef[];
}

// =============================================================================
// Module Registry Types
// =============================================================================

/**
 * All available module type IDs
 */
export type ModuleTypeId =
  // Oscillators
  | 'vco'
  | 'analog_vco'
  | 'lfo'
  // Filters
  | 'svf'
  | 'diode_ladder'
  // Envelopes
  | 'adsr'
  // Amplifiers & Utilities
  | 'vca'
  | 'mixer'
  | 'offset'
  | 'unit_delay'
  | 'multiple'
  | 'attenuverter'
  | 'slew_limiter'
  | 'sample_and_hold'
  | 'precision_adder'
  | 'vc_switch'
  // Sources
  | 'noise'
  // Sequencing
  | 'step_sequencer'
  | 'clock'
  // Effects
  | 'saturator'
  | 'wavefolder'
  | 'ring_mod'
  | 'crossfader'
  | 'rectifier'
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
  // Utilities
  | 'min'
  | 'max'
  // I/O
  | 'stereo_output'
  | 'quantizer';

/**
 * Module category for grouping in UI
 */
export type ModuleCategory =
  | 'Oscillators'
  | 'Filters'
  | 'Envelopes'
  | 'Modulation'
  | 'Utilities'
  | 'Sources'
  | 'Sequencing'
  | 'Effects'
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
