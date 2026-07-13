'use client';
"use strict";
var __create = Object.create;
var __defProp = Object.defineProperty;
var __getOwnPropDesc = Object.getOwnPropertyDescriptor;
var __getOwnPropNames = Object.getOwnPropertyNames;
var __getProtoOf = Object.getPrototypeOf;
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
var __toESM = (mod, isNodeMode, target) => (target = mod != null ? __create(__getProtoOf(mod)) : {}, __copyProps(
  // If the importer is in node compatibility mode or this is not an ESM
  // file that has been converted to a CommonJS file using a Babel-
  // compatible transform (i.e. "__esModule" has not been set), then set
  // "default" to the CommonJS "module.exports" for node compatibility.
  isNodeMode || !mod || !mod.__esModule ? __defProp(target, "default", { value: mod, enumerable: true }) : target,
  mod
));
var __toCommonJS = (mod) => __copyProps(__defProp({}, "__esModule", { value: true }), mod);

// src/index.ts
var index_exports = {};
__export(index_exports, {
  DEFAULT_SIGNAL_COLORS: () => import_types3.DEFAULT_SIGNAL_COLORS,
  SIGNAL_KINDS: () => import_types3.SIGNAL_KINDS,
  checkPortCompatibility: () => import_types3.checkPortCompatibility,
  createCableDef: () => import_types3.createCableDef,
  createModuleDef: () => import_types3.createModuleDef,
  createPatchDef: () => import_types3.createPatchDef,
  createPortReference: () => import_types3.createPortReference,
  createQuiverEdge: () => createQuiverEdge,
  createQuiverNode: () => createQuiverNode,
  generateModuleName: () => generateModuleName,
  getCablesForModule: () => getCablesForModule,
  getSignalColor: () => import_types3.getSignalColor,
  parsePortReference: () => import_types3.parsePortReference,
  patchToReactFlow: () => patchToReactFlow,
  reactFlowToPatch: () => reactFlowToPatch,
  removeModuleFromPatch: () => removeModuleFromPatch,
  updatePatchPositions: () => updatePatchPositions,
  useQuiverCatalog: () => useQuiverCatalog,
  useQuiverEngine: () => useQuiverEngine,
  useQuiverGate: () => useQuiverGate,
  useQuiverLevel: () => useQuiverLevel,
  useQuiverParam: () => useQuiverParam,
  useQuiverPatch: () => useQuiverPatch,
  useQuiverSearch: () => useQuiverSearch,
  useQuiverUpdates: () => useQuiverUpdates,
  validatePatchDef: () => import_types3.validatePatchDef
});
module.exports = __toCommonJS(index_exports);
var import_types2 = require("@quiver-dsp/types");

// src/hooks.ts
var import_react = require("react");
var import_types = require("@quiver-dsp/types");
function freeEngine(engine) {
  if (!engine) return;
  try {
    engine.free();
  } catch {
  }
}
function useQuiverUpdates(engine, targets) {
  const [values, setValues] = (0, import_react.useState)(/* @__PURE__ */ new Map());
  const targetsRef = (0, import_react.useRef)(targets);
  const frameRef = (0, import_react.useRef)();
  const targetsKey = (0, import_react.useMemo)(
    () => JSON.stringify(targets.map(import_types.getSubscriptionTargetKey).sort()),
    [targets]
  );
  (0, import_react.useEffect)(() => {
    if (!engine) return;
    engine.subscribe(targets);
    targetsRef.current = targets;
    const poll = () => {
      try {
        const updates = engine.poll_updates();
        if (updates && updates.length > 0) {
          setValues((prev) => {
            const next = new Map(prev);
            for (const update of updates) {
              next.set((0, import_types.getObservableValueKey)(update), update);
            }
            return next;
          });
        }
      } catch (e) {
        console.error("Error polling Quiver updates:", e);
      }
      frameRef.current = requestAnimationFrame(poll);
    };
    frameRef.current = requestAnimationFrame(poll);
    return () => {
      if (frameRef.current) {
        cancelAnimationFrame(frameRef.current);
      }
      engine.unsubscribe(targets.map(import_types.getSubscriptionTargetKey));
    };
  }, [engine, targetsKey]);
  return values;
}
function useQuiverParam(engine, nodeId, paramIndex) {
  const [value, setValue] = (0, import_react.useState)(0);
  (0, import_react.useEffect)(() => {
    if (!engine || !nodeId) return;
    try {
      const v = engine.get_param(nodeId, paramIndex);
      setValue(v);
    } catch (e) {
      console.error("Error getting param:", e);
    }
  }, [engine, nodeId, paramIndex]);
  const setParam = (0, import_react.useCallback)(
    (newValue) => {
      if (!engine) return;
      try {
        engine.set_param(nodeId, paramIndex, newValue);
        setValue(newValue);
      } catch (e) {
        console.error("Error setting param:", e);
      }
    },
    [engine, nodeId, paramIndex]
  );
  return [value, setParam];
}
function useQuiverLevel(engine, nodeId, portId) {
  const targets = (0, import_react.useMemo)(
    () => [{ type: "level", node_id: nodeId, port_id: portId }],
    [nodeId, portId]
  );
  const updates = useQuiverUpdates(engine, targets);
  const key = `level:${nodeId}:${portId}`;
  const update = updates.get(key);
  if (update?.type === "level") {
    return { rmsDb: update.rms_db, peakDb: update.peak_db };
  }
  return { rmsDb: -Infinity, peakDb: -Infinity };
}
function useQuiverGate(engine, nodeId, portId) {
  const targets = (0, import_react.useMemo)(
    () => [{ type: "gate", node_id: nodeId, port_id: portId }],
    [nodeId, portId]
  );
  const updates = useQuiverUpdates(engine, targets);
  const key = `gate:${nodeId}:${portId}`;
  const update = updates.get(key);
  if (update?.type === "gate") {
    return update.active;
  }
  return false;
}
function useQuiverCatalog(engine) {
  const [catalog, setCatalog] = (0, import_react.useState)(null);
  (0, import_react.useEffect)(() => {
    if (!engine) return;
    try {
      const cat = engine.get_catalog();
      setCatalog(cat);
    } catch (e) {
      console.error("Error getting catalog:", e);
    }
  }, [engine]);
  return catalog;
}
function useQuiverSearch(engine, query) {
  const [results, setResults] = (0, import_react.useState)([]);
  (0, import_react.useEffect)(() => {
    if (!engine) {
      setResults([]);
      return;
    }
    try {
      if (query.trim()) {
        const r = engine.search_modules(query);
        setResults(r);
      } else {
        setResults([]);
      }
    } catch (e) {
      console.error("Error searching modules:", e);
      setResults([]);
    }
  }, [engine, query]);
  return results;
}
function useQuiverPatch(engine) {
  const [isLoaded, setIsLoaded] = (0, import_react.useState)(false);
  const [error, setError] = (0, import_react.useState)(null);
  const loadPatch = (0, import_react.useCallback)(
    async (patch) => {
      if (!engine) return;
      try {
        setError(null);
        engine.load_patch(patch);
        engine.compile();
        setIsLoaded(true);
      } catch (e) {
        setError(e);
        setIsLoaded(false);
      }
    },
    [engine]
  );
  const savePatch = (0, import_react.useCallback)(
    (name) => {
      if (!engine) return null;
      try {
        return engine.save_patch(name);
      } catch (e) {
        setError(e);
        return null;
      }
    },
    [engine]
  );
  const clearPatch = (0, import_react.useCallback)(() => {
    if (!engine) return;
    try {
      engine.clear_patch();
      setIsLoaded(false);
      setError(null);
    } catch (e) {
      setError(e);
    }
  }, [engine]);
  return {
    isLoaded,
    error,
    loadPatch,
    savePatch,
    clearPatch
  };
}
function useQuiverEngine(sampleRate = 44100) {
  const [engine, setEngine] = (0, import_react.useState)(null);
  const [isReady, setIsReady] = (0, import_react.useState)(false);
  const [error, setError] = (0, import_react.useState)(null);
  (0, import_react.useEffect)(() => {
    let mounted = true;
    let created = null;
    async function init() {
      try {
        const { createEngine } = await import("@quiver-dsp/wasm");
        const eng = await createEngine(sampleRate);
        if (mounted) {
          created = eng;
          setEngine(eng);
          setIsReady(true);
        } else {
          freeEngine(eng);
        }
      } catch (e) {
        if (mounted) {
          setError(e);
        }
      }
    }
    init();
    return () => {
      mounted = false;
      setEngine(null);
      setIsReady(false);
      freeEngine(created);
      created = null;
    };
  }, [sampleRate]);
  return { engine, isReady, error };
}

// src/index.ts
var import_types3 = require("@quiver-dsp/types");
function patchToReactFlow(patch, options = {}) {
  const {
    defaultPosition = { x: 0, y: 0 },
    moduleSpacing = 250,
    signalColors = import_types2.DEFAULT_SIGNAL_COLORS,
    getPortSignalKind
  } = options;
  const nodes = patch.modules.map((module2, index) => {
    const position = module2.position ? { x: module2.position[0], y: module2.position[1] } : {
      x: defaultPosition.x + index % 4 * moduleSpacing,
      y: defaultPosition.y + Math.floor(index / 4) * moduleSpacing
    };
    return {
      id: module2.name,
      type: "quiverModule",
      position,
      data: {
        moduleType: module2.module_type,
        state: module2.state,
        label: module2.name
      }
    };
  });
  const edges = patch.cables.map((cable, index) => {
    const { moduleName: sourceModule, portName: sourcePort } = (0, import_types2.parsePortReference)(
      cable.from
    );
    const { moduleName: targetModule, portName: targetPort } = (0, import_types2.parsePortReference)(
      cable.to
    );
    let signalKind;
    if (getPortSignalKind) {
      const sourceModuleDef = patch.modules.find((m) => m.name === sourceModule);
      if (sourceModuleDef) {
        signalKind = getPortSignalKind(sourceModuleDef.module_type, sourcePort, true);
      }
    }
    return {
      id: `cable-${index}`,
      source: sourceModule,
      sourceHandle: sourcePort,
      target: targetModule,
      targetHandle: targetPort,
      type: "default",
      style: signalKind ? { stroke: (0, import_types2.getSignalColor)(signalKind, signalColors), strokeWidth: 2 } : void 0,
      data: {
        sourcePort,
        targetPort,
        signalKind,
        attenuation: cable.attenuation,
        offset: cable.offset
      }
    };
  });
  return { nodes, edges };
}
function reactFlowToPatch(nodes, edges, metadata) {
  const modules = nodes.map((node) => ({
    name: node.id,
    module_type: node.data.moduleType,
    position: [node.position.x, node.position.y],
    state: node.data.state
  }));
  const cables = edges.map((edge) => {
    const from = (0, import_types2.createPortReference)(
      edge.source,
      edge.sourceHandle || edge.data?.sourcePort || "out"
    );
    const to = (0, import_types2.createPortReference)(
      edge.target,
      edge.targetHandle || edge.data?.targetPort || "in"
    );
    const cable = { from, to };
    if (edge.data?.attenuation !== void 0) {
      cable.attenuation = edge.data.attenuation;
    }
    if (edge.data?.offset !== void 0) {
      cable.offset = edge.data.offset;
    }
    return cable;
  });
  return {
    version: 1,
    name: metadata.name,
    author: metadata.author,
    description: metadata.description,
    tags: metadata.tags || [],
    modules,
    cables,
    parameters: {}
  };
}
function generateModuleName(moduleType, existingNames) {
  let counter = 1;
  let name = moduleType;
  while (existingNames.has(name)) {
    name = `${moduleType}_${counter}`;
    counter++;
  }
  return name;
}
function createQuiverNode(moduleType, position, existingNames) {
  const name = generateModuleName(moduleType, existingNames);
  return {
    id: name,
    type: "quiverModule",
    position,
    data: {
      moduleType,
      label: name
    }
  };
}
function createQuiverEdge(sourceNode, sourcePort, targetNode, targetPort, options) {
  const id = `cable-${sourceNode}-${sourcePort}-${targetNode}-${targetPort}`;
  return {
    id,
    source: sourceNode,
    sourceHandle: sourcePort,
    target: targetNode,
    targetHandle: targetPort,
    style: options?.signalKind ? { stroke: (0, import_types2.getSignalColor)(options.signalKind), strokeWidth: 2 } : void 0,
    data: {
      sourcePort,
      targetPort,
      signalKind: options?.signalKind,
      attenuation: options?.attenuation,
      offset: options?.offset
    }
  };
}
function updatePatchPositions(patch, positions) {
  return {
    ...patch,
    modules: patch.modules.map((module2) => {
      const position = positions.get(module2.name);
      if (position) {
        return {
          ...module2,
          position: [position.x, position.y]
        };
      }
      return module2;
    })
  };
}
function getCablesForModule(cables, moduleName) {
  const incoming = [];
  const outgoing = [];
  for (const cable of cables) {
    const from = (0, import_types2.parsePortReference)(cable.from);
    const to = (0, import_types2.parsePortReference)(cable.to);
    if (from.moduleName === moduleName) {
      outgoing.push(cable);
    }
    if (to.moduleName === moduleName) {
      incoming.push(cable);
    }
  }
  return { incoming, outgoing };
}
function removeModuleFromPatch(patch, moduleName) {
  return {
    ...patch,
    modules: patch.modules.filter((m) => m.name !== moduleName),
    cables: patch.cables.filter((c) => {
      const from = (0, import_types2.parsePortReference)(c.from);
      const to = (0, import_types2.parsePortReference)(c.to);
      return from.moduleName !== moduleName && to.moduleName !== moduleName;
    })
  };
}
// Annotate the CommonJS export names for ESM import in node:
0 && (module.exports = {
  DEFAULT_SIGNAL_COLORS,
  SIGNAL_KINDS,
  checkPortCompatibility,
  createCableDef,
  createModuleDef,
  createPatchDef,
  createPortReference,
  createQuiverEdge,
  createQuiverNode,
  generateModuleName,
  getCablesForModule,
  getSignalColor,
  parsePortReference,
  patchToReactFlow,
  reactFlowToPatch,
  removeModuleFromPatch,
  updatePatchPositions,
  useQuiverCatalog,
  useQuiverEngine,
  useQuiverGate,
  useQuiverLevel,
  useQuiverParam,
  useQuiverPatch,
  useQuiverSearch,
  useQuiverUpdates,
  validatePatchDef
});
//# sourceMappingURL=index.js.map