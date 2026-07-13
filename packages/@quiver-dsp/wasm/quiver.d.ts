/* tslint:disable */
/* eslint-disable */
export interface PortSummary {
    inputs: number;
    outputs: number;
    has_audio_in: boolean;
    has_audio_out: boolean;
}

export interface ValidationError {
    path: string;
    message: string;
}

export interface ValidationResult {
    valid: boolean;
    errors: ValidationError[];
}

export interface CableDef {
    from: string;
    to: string;
    attenuation: number | undefined;
    offset: number | undefined;
}

export interface ModuleDef {
    name: string;
    module_type: string;
    position: [number, number] | undefined;
    state: Value | undefined;
}

export interface CatalogResponse {
    modules: ModuleCatalogEntry[];
    categories: string[];
}

export interface PatchDef {
    version: number;
    name: string;
    author: string | undefined;
    description: string | undefined;
    tags?: string[];
    output?: string | undefined;
    modules: ModuleDef[];
    cables: CableDef[];
    parameters?: StdMap<string, number>;
}

export interface ModuleCatalogEntry {
    type_id: string;
    name: string;
    category: string;
    description: string;
    keywords: string[];
    ports: PortSummary;
    tags: string[];
}

export interface ParamInfo {
    id: string;
    name: string;
    value: number;
    min: number;
    max: number;
    default: number;
    curve: ParamCurve;
    control: ControlType;
    unit: string | undefined;
    format: ValueFormat;
}

export type ValueFormat = { type: "decimal"; places: number } | { type: "frequency" } | { type: "time" } | { type: "decibels" } | { type: "percent" } | { type: "note_name" } | { type: "ratio" };

export type ControlType = "knob" | "slider" | "toggle" | "select";

export type ParamCurve = { type: "linear" } | { type: "exponential" } | { type: "logarithmic" } | { type: "stepped"; steps: number };

export type Compatibility = { status: "exact" } | { status: "allowed" } | { status: "warning"; message: string };

export interface PortSpec {
    inputs: PortDef[];
    outputs: PortDef[];
}

export interface SignalColors {
    audio: string;
    cv_bipolar: string;
    cv_unipolar: string;
    volt_per_octave: string;
    gate: string;
    trigger: string;
    clock: string;
}

export type SignalKind = "audio" | "cv_bipolar" | "cv_unipolar" | "volt_per_octave" | "gate" | "trigger" | "clock";

export interface PortInfo {
    id: number;
    name: string;
    kind: SignalKind;
    normalled_to: string | undefined;
    description: string | undefined;
}

export interface PortDef {
    id: PortId;
    name: string;
    kind: SignalKind;
    default: number;
    normalled_to: PortId | undefined;
    has_attenuverter: boolean;
}

export type ObservableValue = { type: "param"; node_id: string; param_id: string; value: number } | { type: "level"; node_id: string; port_id: number; rms_db: number; peak_db: number } | { type: "gate"; node_id: string; port_id: number; active: boolean } | { type: "scope"; node_id: string; port_id: number; samples: number[] } | { type: "spectrum"; node_id: string; port_id: number; bins: number[]; freq_range: [number, number] };

export type SubscriptionTarget = { type: "param"; node_id: string; param_id: string } | { type: "level"; node_id: string; port_id: number } | { type: "gate"; node_id: string; port_id: number } | { type: "scope"; node_id: string; port_id: number; buffer_size: number } | { type: "spectrum"; node_id: string; port_id: number; fft_size: number };


export class QuiverEngine {
  free(): void;
  [Symbol.dispose](): void;
  /**
   * Add a module to the patch
   */
  add_module(type_id: string, name: string): void;
  /**
   * Disconnect two ports (format: "module.port")
   */
  disconnect(from: string, to: string): void;
  /**
   * Get parameters for a module
   *
   * Note: This returns metadata about the module's type from the registry,
   * not the current parameter values. Use get_param for values.
   */
  get_params(node_name: string): any;
  /**
   * Load a patch from JSON
   */
  load_patch(patch_json: any): void;
  /**
   * Save the current patch to JSON
   */
  save_patch(name: string): any;
  /**
   * Set the output module (required for audio output)
   *
   * The specified module's outputs will be read as the patch's stereo output.
   * Port 0 is left channel, port 1 is right channel.
   */
  set_output(name: string): void;
  /**
   * Get the number of cables in the patch
   */
  cable_count(): number;
  /**
   * Clear the current patch
   */
  clear_patch(): void;
  /**
   * Get the full module catalog
   */
  get_catalog(): any;
  /**
   * Get a MIDI CC value (0-1 normalized)
   */
  get_midi_cc(cc: number): number;
  /**
   * Unsubscribe from real-time value updates
   */
  unsubscribe(target_ids: any): void;
  /**
   * Handle a MIDI Note On message.
   *
   * Updates both the scalar getters and the shared `midi_voct` / `midi_gate` /
   * `midi_velocity` CV sources (see [`add_midi_inputs`](Self::add_midi_inputs)),
   * so a cabled patch responds on the next processed sample.
   *
   * The shared CV sources are monophonic, so overlapping notes follow **last-note
   * priority**: the newly pressed note becomes the sounding note and is pushed onto
   * the held-note stack (see [`midi_note_off`](Self::midi_note_off)).
   */
  midi_note_on(note: number, velocity: number): void;
  /**
   * Get the number of modules in the patch
   */
  module_count(): number;
  /**
   * Poll for pending updates (called from requestAnimationFrame)
   */
  poll_updates(): any;
  /**
   * Get port specification for a module type
   */
  get_port_spec(type_id: string): any;
  /**
   * Handle a MIDI Note Off message.
   *
   * The `midi_*` CV sources are monophonic and shared, so releasing a note only
   * closes the gate when it is the **last** held note. With overlapping notes (a
   * chord, or legato where the next note-on precedes the previous note-off),
   * releasing an inner note keeps the gate open and re-points pitch/velocity to the
   * most recently pressed note still held (**last-note priority**). This preserves
   * the documented "Gate: 5.0 while a note is held" contract instead of dropping the
   * gate — and prematurely releasing every cabled envelope — on the first release.
   */
  midi_note_off(note: number, _velocity: number): void;
  /**
   * Process a block of `num_samples` frames and return the interleaved stereo
   * result as a `Float32Array` of length `num_samples * 2` (`[l0, r0, l1, r1, ...]`).
   *
   * # Zero-allocation
   *
   * The engine keeps preallocated, reused L/R and interleaved buffers (grown on
   * demand). Rendering uses the allocation-free [`Patch::tick_block`], so a
   * steady-state render quantum performs no per-sample or per-block heap
   * allocation. Output is safety-clamped to ±10V to prevent speaker/hearing
   * damage from runaway signals.
   *
   * # Ownership rule (important)
   *
   * The returned `Float32Array` is a **view into WASM linear memory**, valid only
   * until the next call into this engine (which reuses/grows the buffer) or
   * `free`. Read it immediately — e.g. copy into your own array with
   * `Array.from(...)` or `myBuffer.set(...)` — before calling any other engine
   * method. Do not retain the returned object.
   */
  process_block(num_samples: number): Float32Array;
  /**
   * Remove a module from the patch
   */
  remove_module(name: string): void;
  /**
   * Get all categories
   */
  get_categories(): any;
  /**
   * Search modules by query string
   */
  search_modules(query: string): any;
  /**
   * Validate a patch definition
   */
  validate_patch(patch_json: any): any;
  /**
   * Inject the engine-owned MIDI CV source modules into the current patch.
   *
   * Adds five [`ExternalInput`](crate::io::ExternalInput) modules the user can
   * cable from to make MIDI actually drive audio:
   *
   * | Module name      | Signal          | Fed by                         |
   * |------------------|-----------------|--------------------------------|
   * | `midi_voct`      | V/Oct           | `midi_note_on` (pitch)         |
   * | `midi_gate`      | Gate (0/5V)     | `midi_note_on` / `midi_note_off` |
   * | `midi_velocity`  | CV unipolar 0–1 | `midi_note_on` (velocity)      |
   * | `midi_mod`       | CV unipolar 0–1 | `midi_cc(1, ...)` (mod wheel)  |
   * | `midi_bend`      | CV bipolar V/Oct| `midi_pitch_bend`              |
   *
   * Each exposes a single `out` port (e.g. cable `midi_voct.out` -> `vco.voct`).
   * Idempotent: modules already present (by name) are left untouched, so it is
   * safe to call after building or loading a patch. Marks the patch dirty.
   *
   * Note: these modules are engine-managed and are not in the module registry, so
   * a patch saved while they are present cannot be re-instantiated by
   * `load_patch` on a fresh engine — call `add_midi_inputs()` again after loading.
   */
  add_midi_inputs(): void;
  /**
   * Handle a MIDI Pitch Bend message (`value` in -1..1).
   *
   * Drives the shared `midi_bend` CV source as a V/Oct offset of ±2 semitones at
   * full deflection. The [`pitch_bend`](Self::pitch_bend) getter still returns the
   * raw -1..1 value.
   */
  midi_pitch_bend(value: number): void;
  /**
   * Disconnect a cable by its stable [`CableId`](crate::graph::CableId).
   *
   * This is the id returned by [`connect`](Self::connect) and friends. It stays
   * valid regardless of how many other cables have been removed since.
   */
  disconnect_cable(cable_id: number): void;
  /**
   * Get all module names in the patch
   */
  get_module_names(): any;
  /**
   * Connect with full modulation (attenuation and offset).
   * Returns the new cable's stable `CableId`.
   */
  connect_modulated(from: string, to: string, attenuation: number, offset: number): number;
  /**
   * Get default signal colors
   */
  get_signal_colors(): any;
  /**
   * Set a parameter value by name
   *
   * This is a convenience method that looks up the parameter index by name.
   */
  set_param_by_name(node_name: string, param_name: string, value: number): void;
  /**
   * Connect with attenuation. Returns the new cable's stable `CableId`.
   */
  connect_attenuated(from: string, to: string, attenuation: number): number;
  /**
   * Check port compatibility between two signal kinds
   */
  check_compatibility(from: string, to: string): any;
  /**
   * Clear all subscriptions
   */
  clear_subscriptions(): void;
  /**
   * Disconnect the cable at the given position in the cable list.
   *
   * Convenience for callers that track cables positionally. Resolves the position
   * to the cable's stable [`CableId`](crate::graph::CableId) and removes it, so the
   * underlying removal is id-based (never off-by-one after prior removals).
   */
  disconnect_by_index(cable_index: number): void;
  /**
   * Get module position
   */
  get_module_position(name: string): any;
  /**
   * Set module position for UI layout
   */
  set_module_position(name: string, x: number, y: number): void;
  /**
   * Get the number of pending updates
   */
  pending_update_count(): number;
  /**
   * Set how often the state observer collects values, in blocks.
   *
   * `1` collects on every [`process_block`](Self::process_block); higher values
   * decimate collection (default `8`). Clamped to a minimum of 1.
   */
  set_observer_interval(blocks: number): void;
  /**
   * Get modules by category
   */
  get_modules_by_category(category: string): any;
  /**
   * Create a new Quiver engine
   */
  constructor(sample_rate: number);
  /**
   * Process a single sample and return stereo output as a `Float64Array`
   * `[left, right]`.
   */
  tick(): Float64Array;
  /**
   * Reset all module state
   */
  reset(): void;
  /**
   * Compile the patch (required after adding/removing modules or cables)
   */
  compile(): void;
  /**
   * Connect two ports (format: "module.port").
   *
   * Returns the new cable's stable [`CableId`](crate::graph::CableId) as a number.
   * Hold onto it and pass it to [`disconnect_cable`](Self::disconnect_cable) to
   * remove exactly this connection later, even after other cables change.
   */
  connect(from: string, to: string): number;
  /**
   * Handle a MIDI Control Change message.
   *
   * All CCs are stored for retrieval via [`get_midi_cc`](Self::get_midi_cc). CC1
   * (mod wheel) additionally drives the shared `midi_mod` CV source.
   */
  midi_cc(cc: number, value: number): void;
  /**
   * Get a parameter value
   */
  get_param(node_name: string, param_index: number): number;
  /**
   * Set a parameter value by numeric index
   */
  set_param(node_name: string, param_index: number, value: number): void;
  /**
   * Subscribe to real-time value updates
   */
  subscribe(targets: any): void;
  /**
   * Get the current pitch bend value (-1 to 1)
   */
  readonly pitch_bend: number;
  /**
   * Get the sample rate
   */
  readonly sample_rate: number;
  /**
   * Get the current MIDI velocity (0-1)
   */
  readonly midi_velocity: number;
  /**
   * Get the current MIDI gate state
   */
  readonly midi_gate: boolean;
  /**
   * Get the current MIDI note as V/Oct (for connecting to VCO)
   */
  readonly midi_note: number;
}

export class QuiverError {
  private constructor();
  free(): void;
  [Symbol.dispose](): void;
  /**
   * Get the error message
   */
  readonly message: string;
}

export type InitInput = RequestInfo | URL | Response | BufferSource | WebAssembly.Module;

export interface InitOutput {
  readonly memory: WebAssembly.Memory;
  readonly __wbg_quiverengine_free: (a: number, b: number) => void;
  readonly __wbg_quivererror_free: (a: number, b: number) => void;
  readonly quiverengine_add_midi_inputs: (a: number) => void;
  readonly quiverengine_add_module: (a: number, b: number, c: number, d: number, e: number) => [number, number];
  readonly quiverengine_cable_count: (a: number) => number;
  readonly quiverengine_check_compatibility: (a: number, b: number, c: number, d: number, e: number) => [number, number, number];
  readonly quiverengine_clear_patch: (a: number) => void;
  readonly quiverengine_clear_subscriptions: (a: number) => void;
  readonly quiverengine_compile: (a: number) => [number, number];
  readonly quiverengine_connect: (a: number, b: number, c: number, d: number, e: number) => [number, number, number];
  readonly quiverengine_connect_attenuated: (a: number, b: number, c: number, d: number, e: number, f: number) => [number, number, number];
  readonly quiverengine_connect_modulated: (a: number, b: number, c: number, d: number, e: number, f: number, g: number) => [number, number, number];
  readonly quiverengine_disconnect: (a: number, b: number, c: number, d: number, e: number) => [number, number];
  readonly quiverengine_disconnect_by_index: (a: number, b: number) => [number, number];
  readonly quiverengine_disconnect_cable: (a: number, b: number) => [number, number];
  readonly quiverengine_get_catalog: (a: number) => [number, number, number];
  readonly quiverengine_get_categories: (a: number) => [number, number, number];
  readonly quiverengine_get_midi_cc: (a: number, b: number) => number;
  readonly quiverengine_get_module_names: (a: number) => [number, number, number];
  readonly quiverengine_get_module_position: (a: number, b: number, c: number) => [number, number, number];
  readonly quiverengine_get_modules_by_category: (a: number, b: number, c: number) => [number, number, number];
  readonly quiverengine_get_param: (a: number, b: number, c: number, d: number) => [number, number, number];
  readonly quiverengine_get_params: (a: number, b: number, c: number) => [number, number, number];
  readonly quiverengine_get_port_spec: (a: number, b: number, c: number) => [number, number, number];
  readonly quiverengine_get_signal_colors: (a: number) => [number, number, number];
  readonly quiverengine_load_patch: (a: number, b: any) => [number, number];
  readonly quiverengine_midi_cc: (a: number, b: number, c: number) => [number, number];
  readonly quiverengine_midi_gate: (a: number) => number;
  readonly quiverengine_midi_note: (a: number) => number;
  readonly quiverengine_midi_note_off: (a: number, b: number, c: number) => [number, number];
  readonly quiverengine_midi_note_on: (a: number, b: number, c: number) => [number, number];
  readonly quiverengine_midi_pitch_bend: (a: number, b: number) => [number, number];
  readonly quiverengine_midi_velocity: (a: number) => number;
  readonly quiverengine_module_count: (a: number) => number;
  readonly quiverengine_new: (a: number) => number;
  readonly quiverengine_pending_update_count: (a: number) => number;
  readonly quiverengine_pitch_bend: (a: number) => number;
  readonly quiverengine_poll_updates: (a: number) => [number, number, number];
  readonly quiverengine_process_block: (a: number, b: number) => any;
  readonly quiverengine_remove_module: (a: number, b: number, c: number) => [number, number];
  readonly quiverengine_reset: (a: number) => void;
  readonly quiverengine_sample_rate: (a: number) => number;
  readonly quiverengine_save_patch: (a: number, b: number, c: number) => [number, number, number];
  readonly quiverengine_search_modules: (a: number, b: number, c: number) => [number, number, number];
  readonly quiverengine_set_module_position: (a: number, b: number, c: number, d: number, e: number) => [number, number];
  readonly quiverengine_set_observer_interval: (a: number, b: number) => void;
  readonly quiverengine_set_output: (a: number, b: number, c: number) => [number, number];
  readonly quiverengine_set_param: (a: number, b: number, c: number, d: number, e: number) => [number, number];
  readonly quiverengine_set_param_by_name: (a: number, b: number, c: number, d: number, e: number, f: number) => [number, number];
  readonly quiverengine_subscribe: (a: number, b: any) => [number, number];
  readonly quiverengine_tick: (a: number) => [number, number];
  readonly quiverengine_unsubscribe: (a: number, b: any) => [number, number];
  readonly quiverengine_validate_patch: (a: number, b: any) => [number, number, number];
  readonly quivererror_message: (a: number) => [number, number];
  readonly __wbindgen_malloc: (a: number, b: number) => number;
  readonly __wbindgen_realloc: (a: number, b: number, c: number, d: number) => number;
  readonly __wbindgen_exn_store: (a: number) => void;
  readonly __externref_table_alloc: () => number;
  readonly __wbindgen_externrefs: WebAssembly.Table;
  readonly __wbindgen_free: (a: number, b: number, c: number) => void;
  readonly __externref_table_dealloc: (a: number) => void;
  readonly __wbindgen_start: () => void;
}

export type SyncInitInput = BufferSource | WebAssembly.Module;

/**
* Instantiates the given `module`, which can either be bytes or
* a precompiled `WebAssembly.Module`.
*
* @param {{ module: SyncInitInput }} module - Passing `SyncInitInput` directly is deprecated.
*
* @returns {InitOutput}
*/
export function initSync(module: { module: SyncInitInput } | SyncInitInput): InitOutput;

/**
* If `module_or_path` is {RequestInfo} or {URL}, makes a request and
* for everything else, calls `WebAssembly.instantiate` directly.
*
* @param {{ module_or_path: InitInput | Promise<InitInput> }} module_or_path - Passing `InitInput` directly is deprecated.
*
* @returns {Promise<InitOutput>}
*/
export default function __wbg_init (module_or_path?: { module_or_path: InitInput | Promise<InitInput> } | InitInput | Promise<InitInput>): Promise<InitOutput>;
