/**
 * @quiver-dsp/types - TypeScript type definitions for Quiver modular synthesizer
 *
 * This package provides type definitions that match the Rust serialization format,
 * enabling type-safe integration between the Quiver audio engine and frontend UIs.
 */
/**
 * Complete patch definition for serialization
 * Corresponds to Rust: PatchDef in src/serialize.rs
 */
interface PatchDef {
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
interface ModuleDef {
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
interface CableDef {
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
type PortReference = `${string}.${string}`;
/**
 * Semantic signal classification following hardware modular conventions
 * Corresponds to Rust: SignalKind in src/port.rs
 */
type SignalKind = 'audio' | 'cv_bipolar' | 'cv_unipolar' | 'volt_per_octave' | 'gate' | 'trigger' | 'clock';
/**
 * Signal kind metadata with voltage ranges and behaviors
 */
interface SignalKindInfo {
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
declare const SIGNAL_KINDS: Record<SignalKind, SignalKindInfo>;
/**
 * Port definition
 * Corresponds to Rust: PortDef in src/port.rs
 */
interface PortDef {
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
interface PortInfo {
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
declare function portDefToInfo(def: PortDef): PortInfo;
/**
 * Specification of all ports for a module
 * Corresponds to Rust: PortSpec in src/port.rs
 */
interface PortSpec {
    inputs: PortDef[];
    outputs: PortDef[];
}
/**
 * How to format parameter values for display
 * Corresponds to Rust: ValueFormat in src/introspection.rs
 */
type ValueFormat = {
    type: 'decimal';
    places: number;
} | {
    type: 'frequency';
} | {
    type: 'time';
} | {
    type: 'decibels';
} | {
    type: 'percent';
} | {
    type: 'note_name';
} | {
    type: 'ratio';
};
/**
 * How parameter values are scaled between min and max
 * Corresponds to Rust: ParamCurve in src/introspection.rs
 */
type ParamCurve = {
    type: 'linear';
} | {
    type: 'exponential';
} | {
    type: 'logarithmic';
} | {
    type: 'stepped';
    steps: number;
};
/**
 * Suggested UI control type for a parameter
 * Corresponds to Rust: ControlType in src/introspection.rs
 */
type ControlType = 'knob' | 'slider' | 'toggle' | 'select';
/**
 * Complete parameter descriptor for UI generation
 * Corresponds to Rust: ParamInfo in src/introspection.rs
 */
interface ParamInfo {
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
declare function createFrequencyParam(id: string, name: string): ParamInfo;
/**
 * Create a time parameter preset
 */
declare function createTimeParam(id: string, name: string): ParamInfo;
/**
 * Create a percentage parameter preset
 */
declare function createPercentParam(id: string, name: string): ParamInfo;
/**
 * Create a toggle parameter preset
 */
declare function createToggleParam(id: string, name: string): ParamInfo;
/**
 * Create a selector parameter preset
 */
declare function createSelectParam(id: string, name: string, options: number): ParamInfo;
/**
 * Format a value according to a ValueFormat specification
 */
declare function formatParamValue(value: number, format: ValueFormat): string;
/**
 * Apply a parameter curve to convert normalized (0-1) to actual value
 */
declare function applyParamCurve(normalized: number, min: number, max: number, curve: ParamCurve): number;
/**
 * Normalize an actual value to 0-1 based on a parameter curve
 */
declare function normalizeParamValue(value: number, min: number, max: number, curve: ParamCurve): number;
/**
 * All available module type IDs
 */
type ModuleTypeId = 'vco' | 'analog_vco' | 'lfo' | 'svf' | 'diode_ladder' | 'adsr' | 'vca' | 'mixer' | 'offset' | 'unit_delay' | 'multiple' | 'attenuverter' | 'slew_limiter' | 'sample_and_hold' | 'precision_adder' | 'vc_switch' | 'noise' | 'step_sequencer' | 'clock' | 'saturator' | 'wavefolder' | 'ring_mod' | 'crossfader' | 'rectifier' | 'crosstalk' | 'ground_loop' | 'logic_and' | 'logic_or' | 'logic_xor' | 'logic_not' | 'comparator' | 'bernoulli_gate' | 'min' | 'max' | 'stereo_output' | 'quantizer';
/**
 * Module category for grouping in UI
 */
type ModuleCategory = 'Oscillators' | 'Filters' | 'Envelopes' | 'Modulation' | 'Utilities' | 'Sources' | 'Sequencing' | 'Effects' | 'Logic' | 'Random' | 'Analog Modeling' | 'I/O';
/**
 * Module metadata for the catalog
 * Corresponds to Rust: ModuleMetadata in src/serialize.rs
 */
interface ModuleMetadata {
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
/**
 * Summary of a module's port configuration for the catalog UI
 * Corresponds to Rust: PortSummary in src/serialize.rs
 */
interface PortSummary {
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
interface ModuleCatalogEntry {
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
interface CatalogResponse {
    /** All available modules */
    modules: ModuleCatalogEntry[];
    /** All unique categories (sorted) */
    categories: ModuleCategory[];
}
/**
 * Create a PortSummary from a PortSpec
 */
declare function createPortSummary(spec: PortSpec): PortSummary;
/**
 * Search modules by query string (client-side implementation)
 * Matches against type_id, name, description, and keywords (case-insensitive)
 */
declare function searchModules(modules: ModuleCatalogEntry[], query: string): ModuleCatalogEntry[];
/**
 * Filter modules by category
 */
declare function filterByCategory(modules: ModuleCatalogEntry[], category: ModuleCategory): ModuleCatalogEntry[];
/**
 * Filter modules by tag
 */
declare function filterByTag(modules: ModuleCatalogEntry[], tag: string): ModuleCatalogEntry[];
/**
 * Get all unique categories from modules
 */
declare function getCategories(modules: ModuleCatalogEntry[]): ModuleCategory[];
/**
 * CSS hex color values for each signal type
 */
interface SignalColors {
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
declare const DEFAULT_SIGNAL_COLORS: SignalColors;
/**
 * Get the color for a signal kind
 */
declare function getSignalColor(kind: SignalKind, colors?: SignalColors): string;
/**
 * Compatibility status for port connections
 */
type Compatibility = {
    status: 'exact';
} | {
    status: 'allowed';
} | {
    status: 'warning';
    message: string;
};
/**
 * Check if two signal kinds are compatible for connection
 */
declare function checkPortCompatibility(from: SignalKind, to: SignalKind): Compatibility;
/**
 * Parse a port reference string into module name and port name
 */
declare function parsePortReference(ref: PortReference): {
    moduleName: string;
    portName: string;
};
/**
 * Create a port reference string from module and port names
 */
declare function createPortReference(moduleName: string, portName: string): PortReference;
/**
 * Create a new empty patch definition
 */
declare function createPatchDef(name: string): PatchDef;
/**
 * Create a new module definition
 */
declare function createModuleDef(name: string, moduleType: ModuleTypeId, position?: [number, number]): ModuleDef;
/**
 * Create a new cable definition
 */
declare function createCableDef(from: PortReference, to: PortReference, options?: {
    attenuation?: number;
    offset?: number;
}): CableDef;
/**
 * Validation error for patch definitions
 */
interface ValidationError {
    path: string;
    message: string;
}
/**
 * Validation result
 */
type ValidationResult = {
    valid: true;
} | {
    valid: false;
    errors: ValidationError[];
};
/**
 * Validate a patch definition
 */
declare function validatePatchDef(patch: unknown): ValidationResult;
/**
 * Values that can be observed and streamed to the UI
 * Corresponds to Rust: ObservableValue in src/observer.rs
 */
type ObservableValue = {
    type: 'param';
    node_id: string;
    param_id: string;
    value: number;
} | {
    type: 'level';
    node_id: string;
    port_id: number;
    rms_db: number;
    peak_db: number;
} | {
    type: 'gate';
    node_id: string;
    port_id: number;
    active: boolean;
} | {
    type: 'scope';
    node_id: string;
    port_id: number;
    samples: number[];
} | {
    type: 'spectrum';
    node_id: string;
    port_id: number;
    bins: number[];
    freq_range: [number, number];
};
/**
 * Subscription target specifying what to observe
 * Corresponds to Rust: SubscriptionTarget in src/observer.rs
 */
type SubscriptionTarget = {
    type: 'param';
    node_id: string;
    param_id: string;
} | {
    type: 'level';
    node_id: string;
    port_id: number;
} | {
    type: 'gate';
    node_id: string;
    port_id: number;
} | {
    type: 'scope';
    node_id: string;
    port_id: number;
    buffer_size: number;
} | {
    type: 'spectrum';
    node_id: string;
    port_id: number;
    fft_size: number;
};
/**
 * Get a unique key for an observable value (for deduplication in UI state)
 */
declare function getObservableValueKey(value: ObservableValue): string;
/**
 * Get a unique key for a subscription target
 */
declare function getSubscriptionTargetKey(target: SubscriptionTarget): string;
/**
 * Create a param subscription target
 */
declare function subscribeParam(nodeId: string, paramId: string): SubscriptionTarget;
/**
 * Create a level meter subscription target
 */
declare function subscribeLevel(nodeId: string, portId: number): SubscriptionTarget;
/**
 * Create a gate subscription target
 */
declare function subscribeGate(nodeId: string, portId: number): SubscriptionTarget;
/**
 * Create a scope subscription target
 */
declare function subscribeScope(nodeId: string, portId: number, bufferSize?: number): SubscriptionTarget;
/**
 * Create a spectrum analyzer subscription target
 */
declare function subscribeSpectrum(nodeId: string, portId: number, fftSize?: number): SubscriptionTarget;
/**
 * Configuration for the state observer
 */
interface ObserverConfig {
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
declare const DEFAULT_OBSERVER_CONFIG: ObserverConfig;
/**
 * Bridge interface for both WASM and HTTP backends
 */
interface QuiverBridge {
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
declare function calculateRmsDb(samples: number[]): number;
/**
 * Calculate peak level in decibels from samples
 */
declare function calculatePeakDb(samples: number[]): number;
/**
 * Format decibels for display
 */
declare function formatDb(db: number): string;

export { type CableDef, type CatalogResponse, type Compatibility, type ControlType, DEFAULT_OBSERVER_CONFIG, DEFAULT_SIGNAL_COLORS, type ModuleCatalogEntry, type ModuleCategory, type ModuleDef, type ModuleMetadata, type ModuleTypeId, type ObservableValue, type ObserverConfig, type ParamCurve, type ParamInfo, type PatchDef, type PortDef, type PortInfo, type PortReference, type PortSpec, type PortSummary, type QuiverBridge, SIGNAL_KINDS, type SignalColors, type SignalKind, type SignalKindInfo, type SubscriptionTarget, type ValidationError, type ValidationResult, type ValueFormat, applyParamCurve, calculatePeakDb, calculateRmsDb, checkPortCompatibility, createCableDef, createFrequencyParam, createModuleDef, createPatchDef, createPercentParam, createPortReference, createPortSummary, createSelectParam, createTimeParam, createToggleParam, filterByCategory, filterByTag, formatDb, formatParamValue, getCategories, getObservableValueKey, getSignalColor, getSubscriptionTargetKey, normalizeParamValue, parsePortReference, portDefToInfo, searchModules, subscribeGate, subscribeLevel, subscribeParam, subscribeScope, subscribeSpectrum, validatePatchDef };
