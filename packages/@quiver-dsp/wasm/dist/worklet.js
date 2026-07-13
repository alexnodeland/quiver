// quiver.js
var wasm;
function addToExternrefTable0(obj) {
  const idx = wasm.__externref_table_alloc();
  wasm.__wbindgen_externrefs.set(idx, obj);
  return idx;
}
function debugString(val) {
  const type = typeof val;
  if (type == "number" || type == "boolean" || val == null) {
    return `${val}`;
  }
  if (type == "string") {
    return `"${val}"`;
  }
  if (type == "symbol") {
    const description = val.description;
    if (description == null) {
      return "Symbol";
    } else {
      return `Symbol(${description})`;
    }
  }
  if (type == "function") {
    const name = val.name;
    if (typeof name == "string" && name.length > 0) {
      return `Function(${name})`;
    } else {
      return "Function";
    }
  }
  if (Array.isArray(val)) {
    const length = val.length;
    let debug = "[";
    if (length > 0) {
      debug += debugString(val[0]);
    }
    for (let i = 1; i < length; i++) {
      debug += ", " + debugString(val[i]);
    }
    debug += "]";
    return debug;
  }
  const builtInMatches = /\[object ([^\]]+)\]/.exec(toString.call(val));
  let className;
  if (builtInMatches && builtInMatches.length > 1) {
    className = builtInMatches[1];
  } else {
    return toString.call(val);
  }
  if (className == "Object") {
    try {
      return "Object(" + JSON.stringify(val) + ")";
    } catch (_) {
      return "Object";
    }
  }
  if (val instanceof Error) {
    return `${val.name}: ${val.message}
${val.stack}`;
  }
  return className;
}
function getArrayF32FromWasm0(ptr, len) {
  ptr = ptr >>> 0;
  return getFloat32ArrayMemory0().subarray(ptr / 4, ptr / 4 + len);
}
function getArrayF64FromWasm0(ptr, len) {
  ptr = ptr >>> 0;
  return getFloat64ArrayMemory0().subarray(ptr / 8, ptr / 8 + len);
}
function getArrayU8FromWasm0(ptr, len) {
  ptr = ptr >>> 0;
  return getUint8ArrayMemory0().subarray(ptr / 1, ptr / 1 + len);
}
var cachedDataViewMemory0 = null;
function getDataViewMemory0() {
  if (cachedDataViewMemory0 === null || cachedDataViewMemory0.buffer.detached === true || cachedDataViewMemory0.buffer.detached === void 0 && cachedDataViewMemory0.buffer !== wasm.memory.buffer) {
    cachedDataViewMemory0 = new DataView(wasm.memory.buffer);
  }
  return cachedDataViewMemory0;
}
var cachedFloat32ArrayMemory0 = null;
function getFloat32ArrayMemory0() {
  if (cachedFloat32ArrayMemory0 === null || cachedFloat32ArrayMemory0.byteLength === 0) {
    cachedFloat32ArrayMemory0 = new Float32Array(wasm.memory.buffer);
  }
  return cachedFloat32ArrayMemory0;
}
var cachedFloat64ArrayMemory0 = null;
function getFloat64ArrayMemory0() {
  if (cachedFloat64ArrayMemory0 === null || cachedFloat64ArrayMemory0.byteLength === 0) {
    cachedFloat64ArrayMemory0 = new Float64Array(wasm.memory.buffer);
  }
  return cachedFloat64ArrayMemory0;
}
function getStringFromWasm0(ptr, len) {
  ptr = ptr >>> 0;
  return decodeText(ptr, len);
}
var cachedUint8ArrayMemory0 = null;
function getUint8ArrayMemory0() {
  if (cachedUint8ArrayMemory0 === null || cachedUint8ArrayMemory0.byteLength === 0) {
    cachedUint8ArrayMemory0 = new Uint8Array(wasm.memory.buffer);
  }
  return cachedUint8ArrayMemory0;
}
function handleError(f, args) {
  try {
    return f.apply(this, args);
  } catch (e) {
    const idx = addToExternrefTable0(e);
    wasm.__wbindgen_exn_store(idx);
  }
}
function isLikeNone(x) {
  return x === void 0 || x === null;
}
function passStringToWasm0(arg, malloc, realloc) {
  if (realloc === void 0) {
    const buf = cachedTextEncoder.encode(arg);
    const ptr2 = malloc(buf.length, 1) >>> 0;
    getUint8ArrayMemory0().subarray(ptr2, ptr2 + buf.length).set(buf);
    WASM_VECTOR_LEN = buf.length;
    return ptr2;
  }
  let len = arg.length;
  let ptr = malloc(len, 1) >>> 0;
  const mem = getUint8ArrayMemory0();
  let offset = 0;
  for (; offset < len; offset++) {
    const code = arg.charCodeAt(offset);
    if (code > 127) break;
    mem[ptr + offset] = code;
  }
  if (offset !== len) {
    if (offset !== 0) {
      arg = arg.slice(offset);
    }
    ptr = realloc(ptr, len, len = offset + arg.length * 3, 1) >>> 0;
    const view = getUint8ArrayMemory0().subarray(ptr + offset, ptr + len);
    const ret = cachedTextEncoder.encodeInto(arg, view);
    offset += ret.written;
    ptr = realloc(ptr, len, offset, 1) >>> 0;
  }
  WASM_VECTOR_LEN = offset;
  return ptr;
}
function takeFromExternrefTable0(idx) {
  const value = wasm.__wbindgen_externrefs.get(idx);
  wasm.__externref_table_dealloc(idx);
  return value;
}
var cachedTextDecoder = new TextDecoder("utf-8", { ignoreBOM: true, fatal: true });
cachedTextDecoder.decode();
var MAX_SAFARI_DECODE_BYTES = 2146435072;
var numBytesDecoded = 0;
function decodeText(ptr, len) {
  numBytesDecoded += len;
  if (numBytesDecoded >= MAX_SAFARI_DECODE_BYTES) {
    cachedTextDecoder = new TextDecoder("utf-8", { ignoreBOM: true, fatal: true });
    cachedTextDecoder.decode();
    numBytesDecoded = len;
  }
  return cachedTextDecoder.decode(getUint8ArrayMemory0().subarray(ptr, ptr + len));
}
var cachedTextEncoder = new TextEncoder();
if (!("encodeInto" in cachedTextEncoder)) {
  cachedTextEncoder.encodeInto = function(arg, view) {
    const buf = cachedTextEncoder.encode(arg);
    view.set(buf);
    return {
      read: arg.length,
      written: buf.length
    };
  };
}
var WASM_VECTOR_LEN = 0;
var QuiverEngineFinalization = typeof FinalizationRegistry === "undefined" ? { register: () => {
}, unregister: () => {
} } : new FinalizationRegistry((ptr) => wasm.__wbg_quiverengine_free(ptr >>> 0, 1));
var QuiverErrorFinalization = typeof FinalizationRegistry === "undefined" ? { register: () => {
}, unregister: () => {
} } : new FinalizationRegistry((ptr) => wasm.__wbg_quivererror_free(ptr >>> 0, 1));
var QuiverEngine = class {
  __destroy_into_raw() {
    const ptr = this.__wbg_ptr;
    this.__wbg_ptr = 0;
    QuiverEngineFinalization.unregister(this);
    return ptr;
  }
  free() {
    const ptr = this.__destroy_into_raw();
    wasm.__wbg_quiverengine_free(ptr, 0);
  }
  /**
   * Add a module to the patch
   * @param {string} type_id
   * @param {string} name
   */
  add_module(type_id, name) {
    const ptr0 = passStringToWasm0(type_id, wasm.__wbindgen_malloc, wasm.__wbindgen_realloc);
    const len0 = WASM_VECTOR_LEN;
    const ptr1 = passStringToWasm0(name, wasm.__wbindgen_malloc, wasm.__wbindgen_realloc);
    const len1 = WASM_VECTOR_LEN;
    const ret = wasm.quiverengine_add_module(this.__wbg_ptr, ptr0, len0, ptr1, len1);
    if (ret[1]) {
      throw takeFromExternrefTable0(ret[0]);
    }
  }
  /**
   * Disconnect two ports (format: "module.port")
   * @param {string} from
   * @param {string} to
   */
  disconnect(from, to) {
    const ptr0 = passStringToWasm0(from, wasm.__wbindgen_malloc, wasm.__wbindgen_realloc);
    const len0 = WASM_VECTOR_LEN;
    const ptr1 = passStringToWasm0(to, wasm.__wbindgen_malloc, wasm.__wbindgen_realloc);
    const len1 = WASM_VECTOR_LEN;
    const ret = wasm.quiverengine_disconnect(this.__wbg_ptr, ptr0, len0, ptr1, len1);
    if (ret[1]) {
      throw takeFromExternrefTable0(ret[0]);
    }
  }
  /**
   * Get parameters for a module
   *
   * Note: This returns metadata about the module's type from the registry,
   * not the current parameter values. Use get_param for values.
   * @param {string} node_name
   * @returns {any}
   */
  get_params(node_name) {
    const ptr0 = passStringToWasm0(node_name, wasm.__wbindgen_malloc, wasm.__wbindgen_realloc);
    const len0 = WASM_VECTOR_LEN;
    const ret = wasm.quiverengine_get_params(this.__wbg_ptr, ptr0, len0);
    if (ret[2]) {
      throw takeFromExternrefTable0(ret[1]);
    }
    return takeFromExternrefTable0(ret[0]);
  }
  /**
   * Load a patch from JSON
   * @param {any} patch_json
   */
  load_patch(patch_json) {
    const ret = wasm.quiverengine_load_patch(this.__wbg_ptr, patch_json);
    if (ret[1]) {
      throw takeFromExternrefTable0(ret[0]);
    }
  }
  /**
   * Get the current pitch bend value (-1 to 1)
   * @returns {number}
   */
  get pitch_bend() {
    const ret = wasm.quiverengine_pitch_bend(this.__wbg_ptr);
    return ret;
  }
  /**
   * Save the current patch to JSON
   * @param {string} name
   * @returns {any}
   */
  save_patch(name) {
    const ptr0 = passStringToWasm0(name, wasm.__wbindgen_malloc, wasm.__wbindgen_realloc);
    const len0 = WASM_VECTOR_LEN;
    const ret = wasm.quiverengine_save_patch(this.__wbg_ptr, ptr0, len0);
    if (ret[2]) {
      throw takeFromExternrefTable0(ret[1]);
    }
    return takeFromExternrefTable0(ret[0]);
  }
  /**
   * Set the output module (required for audio output)
   *
   * The specified module's outputs will be read as the patch's stereo output.
   * Port 0 is left channel, port 1 is right channel.
   * @param {string} name
   */
  set_output(name) {
    const ptr0 = passStringToWasm0(name, wasm.__wbindgen_malloc, wasm.__wbindgen_realloc);
    const len0 = WASM_VECTOR_LEN;
    const ret = wasm.quiverengine_set_output(this.__wbg_ptr, ptr0, len0);
    if (ret[1]) {
      throw takeFromExternrefTable0(ret[0]);
    }
  }
  /**
   * Get the number of cables in the patch
   * @returns {number}
   */
  cable_count() {
    const ret = wasm.quiverengine_cable_count(this.__wbg_ptr);
    return ret >>> 0;
  }
  /**
   * Clear the current patch
   */
  clear_patch() {
    wasm.quiverengine_clear_patch(this.__wbg_ptr);
  }
  /**
   * Get the full module catalog
   * @returns {any}
   */
  get_catalog() {
    const ret = wasm.quiverengine_get_catalog(this.__wbg_ptr);
    if (ret[2]) {
      throw takeFromExternrefTable0(ret[1]);
    }
    return takeFromExternrefTable0(ret[0]);
  }
  /**
   * Get a MIDI CC value (0-1 normalized)
   * @param {number} cc
   * @returns {number}
   */
  get_midi_cc(cc) {
    const ret = wasm.quiverengine_get_midi_cc(this.__wbg_ptr, cc);
    return ret;
  }
  /**
   * Get the sample rate
   * @returns {number}
   */
  get sample_rate() {
    const ret = wasm.quiverengine_sample_rate(this.__wbg_ptr);
    return ret;
  }
  /**
   * Unsubscribe from real-time value updates
   * @param {any} target_ids
   */
  unsubscribe(target_ids) {
    const ret = wasm.quiverengine_unsubscribe(this.__wbg_ptr, target_ids);
    if (ret[1]) {
      throw takeFromExternrefTable0(ret[0]);
    }
  }
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
   * @param {number} note
   * @param {number} velocity
   */
  midi_note_on(note, velocity) {
    const ret = wasm.quiverengine_midi_note_on(this.__wbg_ptr, note, velocity);
    if (ret[1]) {
      throw takeFromExternrefTable0(ret[0]);
    }
  }
  /**
   * Get the number of modules in the patch
   * @returns {number}
   */
  module_count() {
    const ret = wasm.quiverengine_module_count(this.__wbg_ptr);
    return ret >>> 0;
  }
  /**
   * Poll for pending updates (called from requestAnimationFrame)
   * @returns {any}
   */
  poll_updates() {
    const ret = wasm.quiverengine_poll_updates(this.__wbg_ptr);
    if (ret[2]) {
      throw takeFromExternrefTable0(ret[1]);
    }
    return takeFromExternrefTable0(ret[0]);
  }
  /**
   * Get port specification for a module type
   * @param {string} type_id
   * @returns {any}
   */
  get_port_spec(type_id) {
    const ptr0 = passStringToWasm0(type_id, wasm.__wbindgen_malloc, wasm.__wbindgen_realloc);
    const len0 = WASM_VECTOR_LEN;
    const ret = wasm.quiverengine_get_port_spec(this.__wbg_ptr, ptr0, len0);
    if (ret[2]) {
      throw takeFromExternrefTable0(ret[1]);
    }
    return takeFromExternrefTable0(ret[0]);
  }
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
   * @param {number} note
   * @param {number} _velocity
   */
  midi_note_off(note, _velocity) {
    const ret = wasm.quiverengine_midi_note_off(this.__wbg_ptr, note, _velocity);
    if (ret[1]) {
      throw takeFromExternrefTable0(ret[0]);
    }
  }
  /**
   * Get the current MIDI velocity (0-1)
   * @returns {number}
   */
  get midi_velocity() {
    const ret = wasm.quiverengine_midi_velocity(this.__wbg_ptr);
    return ret;
  }
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
   * @param {number} num_samples
   * @returns {Float32Array}
   */
  process_block(num_samples) {
    const ret = wasm.quiverengine_process_block(this.__wbg_ptr, num_samples);
    return ret;
  }
  /**
   * Remove a module from the patch
   * @param {string} name
   */
  remove_module(name) {
    const ptr0 = passStringToWasm0(name, wasm.__wbindgen_malloc, wasm.__wbindgen_realloc);
    const len0 = WASM_VECTOR_LEN;
    const ret = wasm.quiverengine_remove_module(this.__wbg_ptr, ptr0, len0);
    if (ret[1]) {
      throw takeFromExternrefTable0(ret[0]);
    }
  }
  /**
   * Get all categories
   * @returns {any}
   */
  get_categories() {
    const ret = wasm.quiverengine_get_categories(this.__wbg_ptr);
    if (ret[2]) {
      throw takeFromExternrefTable0(ret[1]);
    }
    return takeFromExternrefTable0(ret[0]);
  }
  /**
   * Search modules by query string
   * @param {string} query
   * @returns {any}
   */
  search_modules(query) {
    const ptr0 = passStringToWasm0(query, wasm.__wbindgen_malloc, wasm.__wbindgen_realloc);
    const len0 = WASM_VECTOR_LEN;
    const ret = wasm.quiverengine_search_modules(this.__wbg_ptr, ptr0, len0);
    if (ret[2]) {
      throw takeFromExternrefTable0(ret[1]);
    }
    return takeFromExternrefTable0(ret[0]);
  }
  /**
   * Validate a patch definition
   * @param {any} patch_json
   * @returns {any}
   */
  validate_patch(patch_json) {
    const ret = wasm.quiverengine_validate_patch(this.__wbg_ptr, patch_json);
    if (ret[2]) {
      throw takeFromExternrefTable0(ret[1]);
    }
    return takeFromExternrefTable0(ret[0]);
  }
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
  add_midi_inputs() {
    wasm.quiverengine_add_midi_inputs(this.__wbg_ptr);
  }
  /**
   * Handle a MIDI Pitch Bend message (`value` in -1..1).
   *
   * Drives the shared `midi_bend` CV source as a V/Oct offset of ±2 semitones at
   * full deflection. The [`pitch_bend`](Self::pitch_bend) getter still returns the
   * raw -1..1 value.
   * @param {number} value
   */
  midi_pitch_bend(value) {
    const ret = wasm.quiverengine_midi_pitch_bend(this.__wbg_ptr, value);
    if (ret[1]) {
      throw takeFromExternrefTable0(ret[0]);
    }
  }
  /**
   * Disconnect a cable by its stable [`CableId`](crate::graph::CableId).
   *
   * This is the id returned by [`connect`](Self::connect) and friends. It stays
   * valid regardless of how many other cables have been removed since.
   * @param {number} cable_id
   */
  disconnect_cable(cable_id) {
    const ret = wasm.quiverengine_disconnect_cable(this.__wbg_ptr, cable_id);
    if (ret[1]) {
      throw takeFromExternrefTable0(ret[0]);
    }
  }
  /**
   * Get all module names in the patch
   * @returns {any}
   */
  get_module_names() {
    const ret = wasm.quiverengine_get_module_names(this.__wbg_ptr);
    if (ret[2]) {
      throw takeFromExternrefTable0(ret[1]);
    }
    return takeFromExternrefTable0(ret[0]);
  }
  /**
   * Connect with full modulation (attenuation and offset).
   * Returns the new cable's stable `CableId`.
   * @param {string} from
   * @param {string} to
   * @param {number} attenuation
   * @param {number} offset
   * @returns {number}
   */
  connect_modulated(from, to, attenuation, offset) {
    const ptr0 = passStringToWasm0(from, wasm.__wbindgen_malloc, wasm.__wbindgen_realloc);
    const len0 = WASM_VECTOR_LEN;
    const ptr1 = passStringToWasm0(to, wasm.__wbindgen_malloc, wasm.__wbindgen_realloc);
    const len1 = WASM_VECTOR_LEN;
    const ret = wasm.quiverengine_connect_modulated(this.__wbg_ptr, ptr0, len0, ptr1, len1, attenuation, offset);
    if (ret[2]) {
      throw takeFromExternrefTable0(ret[1]);
    }
    return ret[0] >>> 0;
  }
  /**
   * Get default signal colors
   * @returns {any}
   */
  get_signal_colors() {
    const ret = wasm.quiverengine_get_signal_colors(this.__wbg_ptr);
    if (ret[2]) {
      throw takeFromExternrefTable0(ret[1]);
    }
    return takeFromExternrefTable0(ret[0]);
  }
  /**
   * Set a parameter value by name
   *
   * This is a convenience method that looks up the parameter index by name.
   * @param {string} node_name
   * @param {string} param_name
   * @param {number} value
   */
  set_param_by_name(node_name, param_name, value) {
    const ptr0 = passStringToWasm0(node_name, wasm.__wbindgen_malloc, wasm.__wbindgen_realloc);
    const len0 = WASM_VECTOR_LEN;
    const ptr1 = passStringToWasm0(param_name, wasm.__wbindgen_malloc, wasm.__wbindgen_realloc);
    const len1 = WASM_VECTOR_LEN;
    const ret = wasm.quiverengine_set_param_by_name(this.__wbg_ptr, ptr0, len0, ptr1, len1, value);
    if (ret[1]) {
      throw takeFromExternrefTable0(ret[0]);
    }
  }
  /**
   * Connect with attenuation. Returns the new cable's stable `CableId`.
   * @param {string} from
   * @param {string} to
   * @param {number} attenuation
   * @returns {number}
   */
  connect_attenuated(from, to, attenuation) {
    const ptr0 = passStringToWasm0(from, wasm.__wbindgen_malloc, wasm.__wbindgen_realloc);
    const len0 = WASM_VECTOR_LEN;
    const ptr1 = passStringToWasm0(to, wasm.__wbindgen_malloc, wasm.__wbindgen_realloc);
    const len1 = WASM_VECTOR_LEN;
    const ret = wasm.quiverengine_connect_attenuated(this.__wbg_ptr, ptr0, len0, ptr1, len1, attenuation);
    if (ret[2]) {
      throw takeFromExternrefTable0(ret[1]);
    }
    return ret[0] >>> 0;
  }
  /**
   * Check port compatibility between two signal kinds
   * @param {string} from
   * @param {string} to
   * @returns {any}
   */
  check_compatibility(from, to) {
    const ptr0 = passStringToWasm0(from, wasm.__wbindgen_malloc, wasm.__wbindgen_realloc);
    const len0 = WASM_VECTOR_LEN;
    const ptr1 = passStringToWasm0(to, wasm.__wbindgen_malloc, wasm.__wbindgen_realloc);
    const len1 = WASM_VECTOR_LEN;
    const ret = wasm.quiverengine_check_compatibility(this.__wbg_ptr, ptr0, len0, ptr1, len1);
    if (ret[2]) {
      throw takeFromExternrefTable0(ret[1]);
    }
    return takeFromExternrefTable0(ret[0]);
  }
  /**
   * Clear all subscriptions
   */
  clear_subscriptions() {
    wasm.quiverengine_clear_subscriptions(this.__wbg_ptr);
  }
  /**
   * Disconnect the cable at the given position in the cable list.
   *
   * Convenience for callers that track cables positionally. Resolves the position
   * to the cable's stable [`CableId`](crate::graph::CableId) and removes it, so the
   * underlying removal is id-based (never off-by-one after prior removals).
   * @param {number} cable_index
   */
  disconnect_by_index(cable_index) {
    const ret = wasm.quiverengine_disconnect_by_index(this.__wbg_ptr, cable_index);
    if (ret[1]) {
      throw takeFromExternrefTable0(ret[0]);
    }
  }
  /**
   * Get module position
   * @param {string} name
   * @returns {any}
   */
  get_module_position(name) {
    const ptr0 = passStringToWasm0(name, wasm.__wbindgen_malloc, wasm.__wbindgen_realloc);
    const len0 = WASM_VECTOR_LEN;
    const ret = wasm.quiverengine_get_module_position(this.__wbg_ptr, ptr0, len0);
    if (ret[2]) {
      throw takeFromExternrefTable0(ret[1]);
    }
    return takeFromExternrefTable0(ret[0]);
  }
  /**
   * Set module position for UI layout
   * @param {string} name
   * @param {number} x
   * @param {number} y
   */
  set_module_position(name, x, y) {
    const ptr0 = passStringToWasm0(name, wasm.__wbindgen_malloc, wasm.__wbindgen_realloc);
    const len0 = WASM_VECTOR_LEN;
    const ret = wasm.quiverengine_set_module_position(this.__wbg_ptr, ptr0, len0, x, y);
    if (ret[1]) {
      throw takeFromExternrefTable0(ret[0]);
    }
  }
  /**
   * Get the number of pending updates
   * @returns {number}
   */
  pending_update_count() {
    const ret = wasm.quiverengine_pending_update_count(this.__wbg_ptr);
    return ret >>> 0;
  }
  /**
   * Set how often the state observer collects values, in blocks.
   *
   * `1` collects on every [`process_block`](Self::process_block); higher values
   * decimate collection (default `8`). Clamped to a minimum of 1.
   * @param {number} blocks
   */
  set_observer_interval(blocks) {
    wasm.quiverengine_set_observer_interval(this.__wbg_ptr, blocks);
  }
  /**
   * Get modules by category
   * @param {string} category
   * @returns {any}
   */
  get_modules_by_category(category) {
    const ptr0 = passStringToWasm0(category, wasm.__wbindgen_malloc, wasm.__wbindgen_realloc);
    const len0 = WASM_VECTOR_LEN;
    const ret = wasm.quiverengine_get_modules_by_category(this.__wbg_ptr, ptr0, len0);
    if (ret[2]) {
      throw takeFromExternrefTable0(ret[1]);
    }
    return takeFromExternrefTable0(ret[0]);
  }
  /**
   * Create a new Quiver engine
   * @param {number} sample_rate
   */
  constructor(sample_rate) {
    const ret = wasm.quiverengine_new(sample_rate);
    this.__wbg_ptr = ret >>> 0;
    QuiverEngineFinalization.register(this, this.__wbg_ptr, this);
    return this;
  }
  /**
   * Process a single sample and return stereo output as a `Float64Array`
   * `[left, right]`.
   * @returns {Float64Array}
   */
  tick() {
    const ret = wasm.quiverengine_tick(this.__wbg_ptr);
    var v1 = getArrayF64FromWasm0(ret[0], ret[1]).slice();
    wasm.__wbindgen_free(ret[0], ret[1] * 8, 8);
    return v1;
  }
  /**
   * Reset all module state
   */
  reset() {
    wasm.quiverengine_reset(this.__wbg_ptr);
  }
  /**
   * Compile the patch (required after adding/removing modules or cables)
   */
  compile() {
    const ret = wasm.quiverengine_compile(this.__wbg_ptr);
    if (ret[1]) {
      throw takeFromExternrefTable0(ret[0]);
    }
  }
  /**
   * Connect two ports (format: "module.port").
   *
   * Returns the new cable's stable [`CableId`](crate::graph::CableId) as a number.
   * Hold onto it and pass it to [`disconnect_cable`](Self::disconnect_cable) to
   * remove exactly this connection later, even after other cables change.
   * @param {string} from
   * @param {string} to
   * @returns {number}
   */
  connect(from, to) {
    const ptr0 = passStringToWasm0(from, wasm.__wbindgen_malloc, wasm.__wbindgen_realloc);
    const len0 = WASM_VECTOR_LEN;
    const ptr1 = passStringToWasm0(to, wasm.__wbindgen_malloc, wasm.__wbindgen_realloc);
    const len1 = WASM_VECTOR_LEN;
    const ret = wasm.quiverengine_connect(this.__wbg_ptr, ptr0, len0, ptr1, len1);
    if (ret[2]) {
      throw takeFromExternrefTable0(ret[1]);
    }
    return ret[0] >>> 0;
  }
  /**
   * Handle a MIDI Control Change message.
   *
   * All CCs are stored for retrieval via [`get_midi_cc`](Self::get_midi_cc). CC1
   * (mod wheel) additionally drives the shared `midi_mod` CV source.
   * @param {number} cc
   * @param {number} value
   */
  midi_cc(cc, value) {
    const ret = wasm.quiverengine_midi_cc(this.__wbg_ptr, cc, value);
    if (ret[1]) {
      throw takeFromExternrefTable0(ret[0]);
    }
  }
  /**
   * Get a parameter value
   * @param {string} node_name
   * @param {number} param_index
   * @returns {number}
   */
  get_param(node_name, param_index) {
    const ptr0 = passStringToWasm0(node_name, wasm.__wbindgen_malloc, wasm.__wbindgen_realloc);
    const len0 = WASM_VECTOR_LEN;
    const ret = wasm.quiverengine_get_param(this.__wbg_ptr, ptr0, len0, param_index);
    if (ret[2]) {
      throw takeFromExternrefTable0(ret[1]);
    }
    return ret[0];
  }
  /**
   * Get the current MIDI gate state
   * @returns {boolean}
   */
  get midi_gate() {
    const ret = wasm.quiverengine_midi_gate(this.__wbg_ptr);
    return ret !== 0;
  }
  /**
   * Get the current MIDI note as V/Oct (for connecting to VCO)
   * @returns {number}
   */
  get midi_note() {
    const ret = wasm.quiverengine_midi_note(this.__wbg_ptr);
    return ret;
  }
  /**
   * Set a parameter value by numeric index
   * @param {string} node_name
   * @param {number} param_index
   * @param {number} value
   */
  set_param(node_name, param_index, value) {
    const ptr0 = passStringToWasm0(node_name, wasm.__wbindgen_malloc, wasm.__wbindgen_realloc);
    const len0 = WASM_VECTOR_LEN;
    const ret = wasm.quiverengine_set_param(this.__wbg_ptr, ptr0, len0, param_index, value);
    if (ret[1]) {
      throw takeFromExternrefTable0(ret[0]);
    }
  }
  /**
   * Subscribe to real-time value updates
   * @param {any} targets
   */
  subscribe(targets) {
    const ret = wasm.quiverengine_subscribe(this.__wbg_ptr, targets);
    if (ret[1]) {
      throw takeFromExternrefTable0(ret[0]);
    }
  }
};
if (Symbol.dispose) QuiverEngine.prototype[Symbol.dispose] = QuiverEngine.prototype.free;
var QuiverError = class {
  __destroy_into_raw() {
    const ptr = this.__wbg_ptr;
    this.__wbg_ptr = 0;
    QuiverErrorFinalization.unregister(this);
    return ptr;
  }
  free() {
    const ptr = this.__destroy_into_raw();
    wasm.__wbg_quivererror_free(ptr, 0);
  }
  /**
   * Get the error message
   * @returns {string}
   */
  get message() {
    let deferred1_0;
    let deferred1_1;
    try {
      const ret = wasm.quivererror_message(this.__wbg_ptr);
      deferred1_0 = ret[0];
      deferred1_1 = ret[1];
      return getStringFromWasm0(ret[0], ret[1]);
    } finally {
      wasm.__wbindgen_free(deferred1_0, deferred1_1, 1);
    }
  }
};
if (Symbol.dispose) QuiverError.prototype[Symbol.dispose] = QuiverError.prototype.free;
var EXPECTED_RESPONSE_TYPES = /* @__PURE__ */ new Set(["basic", "cors", "default"]);
async function __wbg_load(module, imports) {
  if (typeof Response === "function" && module instanceof Response) {
    if (typeof WebAssembly.instantiateStreaming === "function") {
      try {
        return await WebAssembly.instantiateStreaming(module, imports);
      } catch (e) {
        const validResponse = module.ok && EXPECTED_RESPONSE_TYPES.has(module.type);
        if (validResponse && module.headers.get("Content-Type") !== "application/wasm") {
          console.warn("`WebAssembly.instantiateStreaming` failed because your server does not serve Wasm with `application/wasm` MIME type. Falling back to `WebAssembly.instantiate` which is slower. Original error:\n", e);
        } else {
          throw e;
        }
      }
    }
    const bytes = await module.arrayBuffer();
    return await WebAssembly.instantiate(bytes, imports);
  } else {
    const instance = await WebAssembly.instantiate(module, imports);
    if (instance instanceof WebAssembly.Instance) {
      return { instance, module };
    } else {
      return instance;
    }
  }
}
function __wbg_get_imports() {
  const imports = {};
  imports.wbg = {};
  imports.wbg.__wbg_Error_52673b7de5a0ca89 = function(arg0, arg1) {
    const ret = Error(getStringFromWasm0(arg0, arg1));
    return ret;
  };
  imports.wbg.__wbg_Number_2d1dcfcf4ec51736 = function(arg0) {
    const ret = Number(arg0);
    return ret;
  };
  imports.wbg.__wbg_String_8f0eb39a4a4c2f66 = function(arg0, arg1) {
    const ret = String(arg1);
    const ptr1 = passStringToWasm0(ret, wasm.__wbindgen_malloc, wasm.__wbindgen_realloc);
    const len1 = WASM_VECTOR_LEN;
    getDataViewMemory0().setInt32(arg0 + 4 * 1, len1, true);
    getDataViewMemory0().setInt32(arg0 + 4 * 0, ptr1, true);
  };
  imports.wbg.__wbg___wbindgen_bigint_get_as_i64_6e32f5e6aff02e1d = function(arg0, arg1) {
    const v = arg1;
    const ret = typeof v === "bigint" ? v : void 0;
    getDataViewMemory0().setBigInt64(arg0 + 8 * 1, isLikeNone(ret) ? BigInt(0) : ret, true);
    getDataViewMemory0().setInt32(arg0 + 4 * 0, !isLikeNone(ret), true);
  };
  imports.wbg.__wbg___wbindgen_boolean_get_dea25b33882b895b = function(arg0) {
    const v = arg0;
    const ret = typeof v === "boolean" ? v : void 0;
    return isLikeNone(ret) ? 16777215 : ret ? 1 : 0;
  };
  imports.wbg.__wbg___wbindgen_debug_string_adfb662ae34724b6 = function(arg0, arg1) {
    const ret = debugString(arg1);
    const ptr1 = passStringToWasm0(ret, wasm.__wbindgen_malloc, wasm.__wbindgen_realloc);
    const len1 = WASM_VECTOR_LEN;
    getDataViewMemory0().setInt32(arg0 + 4 * 1, len1, true);
    getDataViewMemory0().setInt32(arg0 + 4 * 0, ptr1, true);
  };
  imports.wbg.__wbg___wbindgen_in_0d3e1e8f0c669317 = function(arg0, arg1) {
    const ret = arg0 in arg1;
    return ret;
  };
  imports.wbg.__wbg___wbindgen_is_bigint_0e1a2e3f55cfae27 = function(arg0) {
    const ret = typeof arg0 === "bigint";
    return ret;
  };
  imports.wbg.__wbg___wbindgen_is_function_8d400b8b1af978cd = function(arg0) {
    const ret = typeof arg0 === "function";
    return ret;
  };
  imports.wbg.__wbg___wbindgen_is_object_ce774f3490692386 = function(arg0) {
    const val = arg0;
    const ret = typeof val === "object" && val !== null;
    return ret;
  };
  imports.wbg.__wbg___wbindgen_is_string_704ef9c8fc131030 = function(arg0) {
    const ret = typeof arg0 === "string";
    return ret;
  };
  imports.wbg.__wbg___wbindgen_is_undefined_f6b95eab589e0269 = function(arg0) {
    const ret = arg0 === void 0;
    return ret;
  };
  imports.wbg.__wbg___wbindgen_jsval_eq_b6101cc9cef1fe36 = function(arg0, arg1) {
    const ret = arg0 === arg1;
    return ret;
  };
  imports.wbg.__wbg___wbindgen_jsval_loose_eq_766057600fdd1b0d = function(arg0, arg1) {
    const ret = arg0 == arg1;
    return ret;
  };
  imports.wbg.__wbg___wbindgen_number_get_9619185a74197f95 = function(arg0, arg1) {
    const obj = arg1;
    const ret = typeof obj === "number" ? obj : void 0;
    getDataViewMemory0().setFloat64(arg0 + 8 * 1, isLikeNone(ret) ? 0 : ret, true);
    getDataViewMemory0().setInt32(arg0 + 4 * 0, !isLikeNone(ret), true);
  };
  imports.wbg.__wbg___wbindgen_string_get_a2a31e16edf96e42 = function(arg0, arg1) {
    const obj = arg1;
    const ret = typeof obj === "string" ? obj : void 0;
    var ptr1 = isLikeNone(ret) ? 0 : passStringToWasm0(ret, wasm.__wbindgen_malloc, wasm.__wbindgen_realloc);
    var len1 = WASM_VECTOR_LEN;
    getDataViewMemory0().setInt32(arg0 + 4 * 1, len1, true);
    getDataViewMemory0().setInt32(arg0 + 4 * 0, ptr1, true);
  };
  imports.wbg.__wbg___wbindgen_throw_dd24417ed36fc46e = function(arg0, arg1) {
    throw new Error(getStringFromWasm0(arg0, arg1));
  };
  imports.wbg.__wbg_call_abb4ff46ce38be40 = function() {
    return handleError(function(arg0, arg1) {
      const ret = arg0.call(arg1);
      return ret;
    }, arguments);
  };
  imports.wbg.__wbg_done_62ea16af4ce34b24 = function(arg0) {
    const ret = arg0.done;
    return ret;
  };
  imports.wbg.__wbg_entries_83c79938054e065f = function(arg0) {
    const ret = Object.entries(arg0);
    return ret;
  };
  imports.wbg.__wbg_error_7534b8e9a36f1ab4 = function(arg0, arg1) {
    let deferred0_0;
    let deferred0_1;
    try {
      deferred0_0 = arg0;
      deferred0_1 = arg1;
      console.error(getStringFromWasm0(arg0, arg1));
    } finally {
      wasm.__wbindgen_free(deferred0_0, deferred0_1, 1);
    }
  };
  imports.wbg.__wbg_get_6b7bd52aca3f9671 = function(arg0, arg1) {
    const ret = arg0[arg1 >>> 0];
    return ret;
  };
  imports.wbg.__wbg_get_af9dab7e9603ea93 = function() {
    return handleError(function(arg0, arg1) {
      const ret = Reflect.get(arg0, arg1);
      return ret;
    }, arguments);
  };
  imports.wbg.__wbg_get_with_ref_key_1dc361bd10053bfe = function(arg0, arg1) {
    const ret = arg0[arg1];
    return ret;
  };
  imports.wbg.__wbg_instanceof_ArrayBuffer_f3320d2419cd0355 = function(arg0) {
    let result;
    try {
      result = arg0 instanceof ArrayBuffer;
    } catch (_) {
      result = false;
    }
    const ret = result;
    return ret;
  };
  imports.wbg.__wbg_instanceof_Map_084be8da74364158 = function(arg0) {
    let result;
    try {
      result = arg0 instanceof Map;
    } catch (_) {
      result = false;
    }
    const ret = result;
    return ret;
  };
  imports.wbg.__wbg_instanceof_Uint8Array_da54ccc9d3e09434 = function(arg0) {
    let result;
    try {
      result = arg0 instanceof Uint8Array;
    } catch (_) {
      result = false;
    }
    const ret = result;
    return ret;
  };
  imports.wbg.__wbg_isArray_51fd9e6422c0a395 = function(arg0) {
    const ret = Array.isArray(arg0);
    return ret;
  };
  imports.wbg.__wbg_isSafeInteger_ae7d3f054d55fa16 = function(arg0) {
    const ret = Number.isSafeInteger(arg0);
    return ret;
  };
  imports.wbg.__wbg_iterator_27b7c8b35ab3e86b = function() {
    const ret = Symbol.iterator;
    return ret;
  };
  imports.wbg.__wbg_length_22ac23eaec9d8053 = function(arg0) {
    const ret = arg0.length;
    return ret;
  };
  imports.wbg.__wbg_length_d45040a40c570362 = function(arg0) {
    const ret = arg0.length;
    return ret;
  };
  imports.wbg.__wbg_new_1ba21ce319a06297 = function() {
    const ret = new Object();
    return ret;
  };
  imports.wbg.__wbg_new_25f239778d6112b9 = function() {
    const ret = new Array();
    return ret;
  };
  imports.wbg.__wbg_new_6421f6084cc5bc5a = function(arg0) {
    const ret = new Uint8Array(arg0);
    return ret;
  };
  imports.wbg.__wbg_new_8a6f238a6ece86ea = function() {
    const ret = new Error();
    return ret;
  };
  imports.wbg.__wbg_new_b546ae120718850e = function() {
    const ret = /* @__PURE__ */ new Map();
    return ret;
  };
  imports.wbg.__wbg_next_138a17bbf04e926c = function(arg0) {
    const ret = arg0.next;
    return ret;
  };
  imports.wbg.__wbg_next_3cfe5c0fe2a4cc53 = function() {
    return handleError(function(arg0) {
      const ret = arg0.next();
      return ret;
    }, arguments);
  };
  imports.wbg.__wbg_prototypesetcall_dfe9b766cdc1f1fd = function(arg0, arg1, arg2) {
    Uint8Array.prototype.set.call(getArrayU8FromWasm0(arg0, arg1), arg2);
  };
  imports.wbg.__wbg_set_3f1d0b984ed272ed = function(arg0, arg1, arg2) {
    arg0[arg1] = arg2;
  };
  imports.wbg.__wbg_set_7df433eea03a5c14 = function(arg0, arg1, arg2) {
    arg0[arg1 >>> 0] = arg2;
  };
  imports.wbg.__wbg_set_efaaf145b9377369 = function(arg0, arg1, arg2) {
    const ret = arg0.set(arg1, arg2);
    return ret;
  };
  imports.wbg.__wbg_stack_0ed75d68575b0f3c = function(arg0, arg1) {
    const ret = arg1.stack;
    const ptr1 = passStringToWasm0(ret, wasm.__wbindgen_malloc, wasm.__wbindgen_realloc);
    const len1 = WASM_VECTOR_LEN;
    getDataViewMemory0().setInt32(arg0 + 4 * 1, len1, true);
    getDataViewMemory0().setInt32(arg0 + 4 * 0, ptr1, true);
  };
  imports.wbg.__wbg_value_57b7b035e117f7ee = function(arg0) {
    const ret = arg0.value;
    return ret;
  };
  imports.wbg.__wbindgen_cast_2241b6af4c4b2941 = function(arg0, arg1) {
    const ret = getStringFromWasm0(arg0, arg1);
    return ret;
  };
  imports.wbg.__wbindgen_cast_4625c577ab2ec9ee = function(arg0) {
    const ret = BigInt.asUintN(64, arg0);
    return ret;
  };
  imports.wbg.__wbindgen_cast_9ae0607507abb057 = function(arg0) {
    const ret = arg0;
    return ret;
  };
  imports.wbg.__wbindgen_cast_cd07b1914aa3d62c = function(arg0, arg1) {
    const ret = getArrayF32FromWasm0(arg0, arg1);
    return ret;
  };
  imports.wbg.__wbindgen_cast_d6cd19b81560fd6e = function(arg0) {
    const ret = arg0;
    return ret;
  };
  imports.wbg.__wbindgen_init_externref_table = function() {
    const table = wasm.__wbindgen_externrefs;
    const offset = table.grow(4);
    table.set(0, void 0);
    table.set(offset + 0, void 0);
    table.set(offset + 1, null);
    table.set(offset + 2, true);
    table.set(offset + 3, false);
  };
  return imports;
}
function __wbg_finalize_init(instance, module) {
  wasm = instance.exports;
  __wbg_init.__wbindgen_wasm_module = module;
  cachedDataViewMemory0 = null;
  cachedFloat32ArrayMemory0 = null;
  cachedFloat64ArrayMemory0 = null;
  cachedUint8ArrayMemory0 = null;
  wasm.__wbindgen_start();
  return wasm;
}
function initSync(module) {
  if (wasm !== void 0) return wasm;
  if (typeof module !== "undefined") {
    if (Object.getPrototypeOf(module) === Object.prototype) {
      ({ module } = module);
    } else {
      console.warn("using deprecated parameters for `initSync()`; pass a single object instead");
    }
  }
  const imports = __wbg_get_imports();
  if (!(module instanceof WebAssembly.Module)) {
    module = new WebAssembly.Module(module);
  }
  const instance = new WebAssembly.Instance(module, imports);
  return __wbg_finalize_init(instance, module);
}
async function __wbg_init(module_or_path) {
  if (wasm !== void 0) return wasm;
  if (typeof module_or_path !== "undefined") {
    if (Object.getPrototypeOf(module_or_path) === Object.prototype) {
      ({ module_or_path } = module_or_path);
    } else {
      console.warn("using deprecated parameters for the initialization function; pass a single object instead");
    }
  }
  if (typeof module_or_path === "undefined") {
    module_or_path = new URL("quiver_bg.wasm", import.meta.url);
  }
  const imports = __wbg_get_imports();
  if (typeof module_or_path === "string" || typeof Request === "function" && module_or_path instanceof Request || typeof URL === "function" && module_or_path instanceof URL) {
    module_or_path = fetch(module_or_path);
  }
  const { instance, module } = await __wbg_load(await module_or_path, imports);
  return __wbg_finalize_init(instance, module);
}

// src/worklet.ts
var QuiverProcessor = class extends AudioWorkletProcessor {
  constructor() {
    super();
    this.engine = null;
    this.ready = false;
    this.destroyed = false;
    this.pending = [];
    this.port.onmessage = (event) => {
      const message = event.data;
      if (message.type === "init") {
        this.handleInit(message);
      } else if (message.type === "destroy") {
        this.handleDestroy();
      } else if (this.ready && this.engine) {
        this.handleMessage(message);
      } else {
        this.pending.push(message);
      }
    };
  }
  handleInit(message) {
    try {
      initSync({ module: message.wasmBytes });
      this.engine = new QuiverEngine(message.sampleRate ?? sampleRate);
      this.ready = true;
      for (const msg of this.pending) this.handleMessage(msg);
      this.pending = [];
      this.port.postMessage({ type: "ready" });
    } catch (e) {
      this.port.postMessage({ type: "error", error: String(e) });
    }
  }
  handleDestroy() {
    if (this.engine) {
      const engine = this.engine;
      this.engine = null;
      this.ready = false;
      try {
        engine.free();
      } catch {
      }
    }
    this.destroyed = true;
  }
  handleMessage(message) {
    const engine = this.engine;
    if (!engine) return;
    const requestId = message.requestId;
    try {
      switch (message.type) {
        case "load_patch":
          engine.load_patch(message.patch);
          engine.compile();
          this.port.postMessage({ type: "patch_loaded", requestId });
          break;
        case "save_patch":
          this.port.postMessage({
            type: "patch_saved",
            patch: engine.save_patch(message.name),
            requestId
          });
          break;
        case "set_param":
          engine.set_param(message.nodeId, message.paramIndex, message.value);
          break;
        case "add_module":
          engine.add_module(message.typeId, message.name);
          break;
        case "remove_module":
          engine.remove_module(message.name);
          break;
        case "connect":
          if (message.attenuation !== void 0 && message.offset !== void 0) {
            engine.connect_modulated(
              message.from,
              message.to,
              message.attenuation,
              message.offset
            );
          } else if (message.attenuation !== void 0) {
            engine.connect_attenuated(message.from, message.to, message.attenuation);
          } else {
            engine.connect(message.from, message.to);
          }
          break;
        case "disconnect":
          engine.disconnect(message.from, message.to);
          break;
        case "set_output":
          engine.set_output(message.name);
          break;
        case "add_midi_inputs":
          engine.add_midi_inputs();
          break;
        case "midi_note_on":
          engine.midi_note_on(message.note, message.velocity);
          break;
        case "midi_note_off":
          engine.midi_note_off(message.note, message.velocity);
          break;
        case "midi_cc":
          engine.midi_cc(message.cc, message.value);
          break;
        case "midi_pitch_bend":
          engine.midi_pitch_bend(message.value);
          break;
        case "compile":
          engine.compile();
          this.port.postMessage({ type: "compiled", requestId });
          break;
        case "reset":
          engine.reset();
          break;
      }
    } catch (e) {
      this.port.postMessage({ type: "error", error: String(e), requestId });
    }
  }
  process(_inputs, outputs, _parameters) {
    if (this.destroyed) return false;
    const output = outputs[0];
    if (!this.ready || !this.engine || !output || output.length < 2) {
      return true;
    }
    const left = output[0];
    const right = output[1];
    const numSamples = left.length;
    try {
      const stereo = this.engine.process_block(numSamples);
      for (let i = 0; i < numSamples; i++) {
        left[i] = stereo[i * 2];
        right[i] = stereo[i * 2 + 1];
      }
    } catch (e) {
      left.fill(0);
      right.fill(0);
      console.error("Quiver processing error:", e);
    }
    return true;
  }
};
registerProcessor("quiver-processor", QuiverProcessor);
//# sourceMappingURL=worklet.js.map