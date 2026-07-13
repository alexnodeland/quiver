"use strict";
var __defProp = Object.defineProperty;
var __getOwnPropDesc = Object.getOwnPropertyDescriptor;
var __getOwnPropNames = Object.getOwnPropertyNames;
var __hasOwnProp = Object.prototype.hasOwnProperty;
var __export = (target, all) => {
  for (var name in all)
    __defProp(target, name, { get: all[name], enumerable: true });
};
var __copyProps = (to, from, except, desc) => {
  if (from && typeof from === "object" || typeof from === "function") {
    for (let key of __getOwnPropNames(from))
      if (!__hasOwnProp.call(to, key) && key !== except)
        __defProp(to, key, { get: () => from[key], enumerable: !(desc = __getOwnPropDesc(from, key)) || desc.enumerable });
  }
  return to;
};
var __toCommonJS = (mod) => __copyProps(__defProp({}, "__esModule", { value: true }), mod);

// src/index.ts
var index_exports = {};
__export(index_exports, {
  DEFAULT_OBSERVER_CONFIG: () => DEFAULT_OBSERVER_CONFIG,
  DEFAULT_SIGNAL_COLORS: () => DEFAULT_SIGNAL_COLORS,
  SIGNAL_KINDS: () => SIGNAL_KINDS,
  applyParamCurve: () => applyParamCurve,
  calculatePeakDb: () => calculatePeakDb,
  calculateRmsDb: () => calculateRmsDb,
  checkPortCompatibility: () => checkPortCompatibility,
  createCableDef: () => createCableDef,
  createFrequencyParam: () => createFrequencyParam,
  createModuleDef: () => createModuleDef,
  createPatchDef: () => createPatchDef,
  createPercentParam: () => createPercentParam,
  createPortReference: () => createPortReference,
  createPortSummary: () => createPortSummary,
  createSelectParam: () => createSelectParam,
  createTimeParam: () => createTimeParam,
  createToggleParam: () => createToggleParam,
  filterByCategory: () => filterByCategory,
  filterByTag: () => filterByTag,
  formatDb: () => formatDb,
  formatParamValue: () => formatParamValue,
  getCategories: () => getCategories,
  getObservableValueKey: () => getObservableValueKey,
  getSignalColor: () => getSignalColor,
  getSubscriptionTargetKey: () => getSubscriptionTargetKey,
  normalizeParamValue: () => normalizeParamValue,
  parsePortReference: () => parsePortReference,
  portDefToInfo: () => portDefToInfo,
  searchModules: () => searchModules,
  subscribeGate: () => subscribeGate,
  subscribeLevel: () => subscribeLevel,
  subscribeParam: () => subscribeParam,
  subscribeScope: () => subscribeScope,
  subscribeSpectrum: () => subscribeSpectrum,
  validatePatchDef: () => validatePatchDef
});
module.exports = __toCommonJS(index_exports);
var SIGNAL_KINDS = {
  audio: {
    kind: "audio",
    voltageRange: [-5, 5],
    isSummable: true
  },
  cv_bipolar: {
    kind: "cv_bipolar",
    voltageRange: [-5, 5],
    isSummable: true
  },
  cv_unipolar: {
    kind: "cv_unipolar",
    voltageRange: [0, 10],
    isSummable: true
  },
  volt_per_octave: {
    kind: "volt_per_octave",
    voltageRange: [-5, 5],
    isSummable: true
  },
  gate: {
    kind: "gate",
    voltageRange: [0, 5],
    isSummable: false,
    gateThreshold: 2.5
  },
  trigger: {
    kind: "trigger",
    voltageRange: [0, 5],
    isSummable: false,
    gateThreshold: 2.5
  },
  clock: {
    kind: "clock",
    voltageRange: [0, 5],
    isSummable: false,
    gateThreshold: 2.5
  }
};
function portDefToInfo(def) {
  return {
    id: def.id,
    name: def.name,
    kind: def.kind,
    normalled_to: void 0,
    // PortDef uses ID, PortInfo uses name
    description: void 0
  };
}
function createFrequencyParam(id, name) {
  return {
    id,
    name,
    value: 1e3,
    min: 20,
    max: 2e4,
    default: 1e3,
    curve: { type: "exponential" },
    control: "knob",
    unit: "Hz",
    format: { type: "frequency" }
  };
}
function createTimeParam(id, name) {
  return {
    id,
    name,
    value: 0.1,
    min: 1e-3,
    max: 10,
    default: 0.1,
    curve: { type: "exponential" },
    control: "knob",
    unit: "s",
    format: { type: "time" }
  };
}
function createPercentParam(id, name) {
  return {
    id,
    name,
    value: 0.5,
    min: 0,
    max: 1,
    default: 0.5,
    curve: { type: "linear" },
    control: "knob",
    format: { type: "percent" }
  };
}
function createToggleParam(id, name) {
  return {
    id,
    name,
    value: 0,
    min: 0,
    max: 1,
    default: 0,
    curve: { type: "stepped", steps: 2 },
    control: "toggle",
    format: { type: "decimal", places: 0 }
  };
}
function createSelectParam(id, name, options) {
  return {
    id,
    name,
    value: 0,
    min: 0,
    max: options - 1,
    default: 0,
    curve: { type: "stepped", steps: options },
    control: "select",
    format: { type: "decimal", places: 0 }
  };
}
function formatParamValue(value, format) {
  switch (format.type) {
    case "decimal":
      return value.toFixed(format.places);
    case "frequency":
      return value >= 1e3 ? `${(value / 1e3).toFixed(2)} kHz` : `${value.toFixed(1)} Hz`;
    case "time":
      return value >= 1 ? `${value.toFixed(2)} s` : `${(value * 1e3).toFixed(1)} ms`;
    case "decibels":
      return `${value.toFixed(1)} dB`;
    case "percent":
      return `${(value * 100).toFixed(0)}%`;
    case "note_name": {
      const midiNote = Math.round(value * 12 + 60);
      const noteNames = ["C", "C#", "D", "D#", "E", "F", "F#", "G", "G#", "A", "A#", "B"];
      const note = noteNames[(midiNote % 12 + 12) % 12];
      const octave = Math.floor(midiNote / 12) - 1;
      return `${note}${octave}`;
    }
    case "ratio":
      return value >= 1 ? `${value.toFixed(1)}:1` : value > 0 ? `1:${(1 / value).toFixed(1)}` : "0:1";
  }
}
function applyParamCurve(normalized, min, max, curve) {
  const n = Math.max(0, Math.min(1, normalized));
  switch (curve.type) {
    case "linear":
      return min + n * (max - min);
    case "exponential":
      return min <= 0 ? n * max : min * Math.pow(max / min, n);
    case "logarithmic": {
      const logMin = min > 0 ? Math.log10(min) : 0;
      const logMax = Math.log10(Math.max(max, 1e-3));
      return Math.pow(10, logMin + n * (logMax - logMin));
    }
    case "stepped": {
      const stepSize = (max - min) / curve.steps;
      const stepIndex = Math.min(Math.floor(n * curve.steps), curve.steps - 1);
      return min + stepIndex * stepSize;
    }
  }
}
function normalizeParamValue(value, min, max, curve) {
  if (Math.abs(max - min) < 1e-10) return 0;
  switch (curve.type) {
    case "linear":
      return Math.max(0, Math.min(1, (value - min) / (max - min)));
    case "exponential":
      if (min <= 0 || value <= 0) {
        return Math.max(0, Math.min(1, (value - min) / (max - min)));
      }
      return Math.max(0, Math.min(1, Math.log(value / min) / Math.log(max / min)));
    case "logarithmic": {
      const logMin = min > 0 ? Math.log10(min) : 0;
      const logMax = Math.log10(Math.max(max, 1e-3));
      const logVal = Math.log10(Math.max(value, 1e-3));
      return Math.max(0, Math.min(1, (logVal - logMin) / (logMax - logMin)));
    }
    case "stepped": {
      const stepSize = (max - min) / curve.steps;
      const stepIndex = Math.round((value - min) / stepSize);
      return Math.max(0, Math.min(1, stepIndex / curve.steps));
    }
  }
}
function createPortSummary(spec) {
  return {
    inputs: spec.inputs.length,
    outputs: spec.outputs.length,
    has_audio_in: spec.inputs.some((p) => p.kind === "audio"),
    has_audio_out: spec.outputs.some((p) => p.kind === "audio")
  };
}
function searchModules(modules, query) {
  const q = query.toLowerCase();
  const scored = modules.map((m) => {
    let score = 0;
    if (m.type_id.toLowerCase() === q) score = 100;
    else if (m.name.toLowerCase() === q) score = 90;
    else if (m.type_id.toLowerCase().includes(q)) score = 70;
    else if (m.name.toLowerCase().includes(q)) score = 60;
    else if (m.keywords.some((k) => k.toLowerCase() === q)) score = 50;
    else if (m.keywords.some((k) => k.toLowerCase().includes(q))) score = 40;
    else if (m.description.toLowerCase().includes(q)) score = 20;
    else if (m.category.toLowerCase().includes(q)) score = 10;
    return { module: m, score };
  }).filter((item) => item.score > 0).sort((a, b) => b.score - a.score || a.module.name.localeCompare(b.module.name));
  return scored.map((item) => item.module);
}
function filterByCategory(modules, category) {
  return modules.filter((m) => m.category === category).sort((a, b) => a.name.localeCompare(b.name));
}
function filterByTag(modules, tag) {
  return modules.filter((m) => m.tags.includes(tag));
}
function getCategories(modules) {
  const categories = /* @__PURE__ */ new Set();
  for (const m of modules) {
    categories.add(m.category);
  }
  return Array.from(categories).sort();
}
var DEFAULT_SIGNAL_COLORS = {
  audio: "#e94560",
  // Red - audio signals
  cv_bipolar: "#0f3460",
  // Dark blue - bipolar CV
  cv_unipolar: "#00b4d8",
  // Cyan - unipolar CV
  volt_per_octave: "#90be6d",
  // Green - pitch CV
  gate: "#f9c74f",
  // Yellow - gates
  trigger: "#f8961e",
  // Orange - triggers
  clock: "#9d4edd"
  // Purple - clock
};
function getSignalColor(kind, colors = DEFAULT_SIGNAL_COLORS) {
  return colors[kind];
}
function checkPortCompatibility(from, to) {
  if (from === to) {
    return { status: "exact" };
  }
  if (from === "audio") {
    return { status: "allowed" };
  }
  if (from === "cv_bipolar" && to === "cv_unipolar" || from === "cv_unipolar" && to === "cv_bipolar") {
    return { status: "allowed" };
  }
  if (from === "volt_per_octave" && (to === "cv_bipolar" || to === "cv_unipolar")) {
    return { status: "allowed" };
  }
  if (from === "gate" && to === "trigger" || from === "trigger" && to === "gate") {
    return { status: "allowed" };
  }
  if (from === "clock" && to === "gate" || from === "clock" && to === "trigger") {
    return { status: "allowed" };
  }
  if ((from === "gate" || from === "trigger") && to === "audio") {
    return { status: "warning", message: "Gate/Trigger to Audio may cause clicks" };
  }
  if (from === "cv_bipolar" && to === "volt_per_octave") {
    return { status: "warning", message: "CV to V/Oct may cause tuning issues" };
  }
  return { status: "allowed" };
}
function parsePortReference(ref) {
  const parts = ref.split(".");
  if (parts.length !== 2) {
    throw new Error(`Invalid port reference: ${ref}`);
  }
  return {
    moduleName: parts[0],
    portName: parts[1]
  };
}
function createPortReference(moduleName, portName) {
  return `${moduleName}.${portName}`;
}
function createPatchDef(name) {
  return {
    version: 1,
    name,
    tags: [],
    modules: [],
    cables: [],
    parameters: {}
  };
}
function createModuleDef(name, moduleType, position) {
  return {
    name,
    module_type: moduleType,
    position
  };
}
function createCableDef(from, to, options) {
  return {
    from,
    to,
    ...options
  };
}
function validatePatchDef(patch) {
  const errors = [];
  if (typeof patch !== "object" || patch === null) {
    return { valid: false, errors: [{ path: "", message: "Patch must be an object" }] };
  }
  const p = patch;
  if (typeof p.version !== "number" || p.version < 1) {
    errors.push({ path: "version", message: "Version must be a positive integer" });
  }
  if (typeof p.name !== "string" || p.name.length === 0) {
    errors.push({ path: "name", message: "Name must be a non-empty string" });
  }
  if (!Array.isArray(p.modules)) {
    errors.push({ path: "modules", message: "Modules must be an array" });
  } else {
    const moduleNames = /* @__PURE__ */ new Set();
    p.modules.forEach((mod, i) => {
      const modErrors = validateModuleDef(mod, `modules[${i}]`);
      errors.push(...modErrors);
      if (typeof mod === "object" && mod !== null) {
        const m = mod;
        if (typeof m.name === "string") {
          if (moduleNames.has(m.name)) {
            errors.push({
              path: `modules[${i}].name`,
              message: `Duplicate module name: ${m.name}`
            });
          }
          moduleNames.add(m.name);
        }
      }
    });
  }
  if (!Array.isArray(p.cables)) {
    errors.push({ path: "cables", message: "Cables must be an array" });
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
function validateModuleDef(mod, path) {
  const errors = [];
  if (typeof mod !== "object" || mod === null) {
    return [{ path, message: "Module must be an object" }];
  }
  const m = mod;
  if (typeof m.name !== "string" || m.name.length === 0) {
    errors.push({ path: `${path}.name`, message: "Module name must be a non-empty string" });
  }
  if (typeof m.module_type !== "string") {
    errors.push({ path: `${path}.module_type`, message: "Module type must be a string" });
  }
  if (m.position !== void 0) {
    if (!Array.isArray(m.position) || m.position.length !== 2 || typeof m.position[0] !== "number" || typeof m.position[1] !== "number") {
      errors.push({ path: `${path}.position`, message: "Position must be a [number, number] tuple" });
    }
  }
  return errors;
}
function validateCableDef(cable, path) {
  const errors = [];
  if (typeof cable !== "object" || cable === null) {
    return [{ path, message: "Cable must be an object" }];
  }
  const c = cable;
  const portRefPattern = /^[a-zA-Z0-9_-]+\.[a-zA-Z0-9_-]+$/;
  if (typeof c.from !== "string" || !portRefPattern.test(c.from)) {
    errors.push({
      path: `${path}.from`,
      message: "From must be a port reference in format 'module_name.port_name'"
    });
  }
  if (typeof c.to !== "string" || !portRefPattern.test(c.to)) {
    errors.push({
      path: `${path}.to`,
      message: "To must be a port reference in format 'module_name.port_name'"
    });
  }
  if (c.attenuation !== void 0) {
    if (typeof c.attenuation !== "number" || c.attenuation < -2 || c.attenuation > 2) {
      errors.push({
        path: `${path}.attenuation`,
        message: "Attenuation must be a number between -2.0 and 2.0"
      });
    }
  }
  if (c.offset !== void 0) {
    if (typeof c.offset !== "number" || c.offset < -10 || c.offset > 10) {
      errors.push({
        path: `${path}.offset`,
        message: "Offset must be a number between -10.0 and 10.0"
      });
    }
  }
  return errors;
}
function getObservableValueKey(value) {
  switch (value.type) {
    case "param":
      return `param:${value.node_id}:${value.param_id}`;
    case "level":
      return `level:${value.node_id}:${value.port_id}`;
    case "gate":
      return `gate:${value.node_id}:${value.port_id}`;
    case "scope":
      return `scope:${value.node_id}:${value.port_id}`;
    case "spectrum":
      return `spectrum:${value.node_id}:${value.port_id}`;
  }
}
function getSubscriptionTargetKey(target) {
  switch (target.type) {
    case "param":
      return `param:${target.node_id}:${target.param_id}`;
    case "level":
      return `level:${target.node_id}:${target.port_id}`;
    case "gate":
      return `gate:${target.node_id}:${target.port_id}`;
    case "scope":
      return `scope:${target.node_id}:${target.port_id}`;
    case "spectrum":
      return `spectrum:${target.node_id}:${target.port_id}`;
  }
}
function subscribeParam(nodeId, paramId) {
  return { type: "param", node_id: nodeId, param_id: paramId };
}
function subscribeLevel(nodeId, portId) {
  return { type: "level", node_id: nodeId, port_id: portId };
}
function subscribeGate(nodeId, portId) {
  return { type: "gate", node_id: nodeId, port_id: portId };
}
function subscribeScope(nodeId, portId, bufferSize = 512) {
  return { type: "scope", node_id: nodeId, port_id: portId, buffer_size: bufferSize };
}
function subscribeSpectrum(nodeId, portId, fftSize = 1024) {
  return { type: "spectrum", node_id: nodeId, port_id: portId, fft_size: fftSize };
}
var DEFAULT_OBSERVER_CONFIG = {
  maxUpdateRate: 60,
  maxPendingUpdates: 1e3,
  defaultScopeBufferSize: 512,
  defaultFftSize: 1024
};
function calculateRmsDb(samples) {
  if (samples.length === 0) return -Infinity;
  const sumSq = samples.reduce((sum, s) => sum + s * s, 0);
  const rms = Math.sqrt(sumSq / samples.length);
  return rms > 0 ? 20 * Math.log10(rms) : -Infinity;
}
function calculatePeakDb(samples) {
  if (samples.length === 0) return -Infinity;
  const peak = samples.reduce((max, s) => Math.max(max, Math.abs(s)), 0);
  return peak > 0 ? 20 * Math.log10(peak) : -Infinity;
}
function formatDb(db) {
  if (!isFinite(db)) return "-\u221E dB";
  return `${db.toFixed(1)} dB`;
}
// Annotate the CommonJS export names for ESM import in node:
0 && (module.exports = {
  DEFAULT_OBSERVER_CONFIG,
  DEFAULT_SIGNAL_COLORS,
  SIGNAL_KINDS,
  applyParamCurve,
  calculatePeakDb,
  calculateRmsDb,
  checkPortCompatibility,
  createCableDef,
  createFrequencyParam,
  createModuleDef,
  createPatchDef,
  createPercentParam,
  createPortReference,
  createPortSummary,
  createSelectParam,
  createTimeParam,
  createToggleParam,
  filterByCategory,
  filterByTag,
  formatDb,
  formatParamValue,
  getCategories,
  getObservableValueKey,
  getSignalColor,
  getSubscriptionTargetKey,
  normalizeParamValue,
  parsePortReference,
  portDefToInfo,
  searchModules,
  subscribeGate,
  subscribeLevel,
  subscribeParam,
  subscribeScope,
  subscribeSpectrum,
  validatePatchDef
});
