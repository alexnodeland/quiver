'use client';

// src/index.ts
import {
  parsePortReference,
  createPortReference,
  getSignalColor,
  DEFAULT_SIGNAL_COLORS
} from "@quiver-dsp/types";

// src/hooks.ts
import { useEffect, useState, useRef, useCallback, useMemo } from "react";
import {
  getObservableValueKey,
  getSubscriptionTargetKey
} from "@quiver-dsp/types";
function freeEngine(engine) {
  if (!engine) return;
  try {
    engine.free();
  } catch {
  }
}
function useQuiverUpdates(engine, targets) {
  const [values, setValues] = useState(/* @__PURE__ */ new Map());
  const targetsRef = useRef(targets);
  const frameRef = useRef();
  const targetsKey = useMemo(
    () => JSON.stringify(targets.map(getSubscriptionTargetKey).sort()),
    [targets]
  );
  useEffect(() => {
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
              next.set(getObservableValueKey(update), update);
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
      engine.unsubscribe(targets.map(getSubscriptionTargetKey));
    };
  }, [engine, targetsKey]);
  return values;
}
function useQuiverParam(engine, nodeId, paramIndex) {
  const [value, setValue] = useState(0);
  useEffect(() => {
    if (!engine || !nodeId) return;
    try {
      const v = engine.get_param(nodeId, paramIndex);
      setValue(v);
    } catch (e) {
      console.error("Error getting param:", e);
    }
  }, [engine, nodeId, paramIndex]);
  const setParam = useCallback(
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
  const targets = useMemo(
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
  const targets = useMemo(
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
  const [catalog, setCatalog] = useState(null);
  useEffect(() => {
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
  const [results, setResults] = useState([]);
  useEffect(() => {
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
  const [isLoaded, setIsLoaded] = useState(false);
  const [error, setError] = useState(null);
  const loadPatch = useCallback(
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
  const savePatch = useCallback(
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
  const clearPatch = useCallback(() => {
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
  const [engine, setEngine] = useState(null);
  const [isReady, setIsReady] = useState(false);
  const [error, setError] = useState(null);
  useEffect(() => {
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
import {
  SIGNAL_KINDS,
  DEFAULT_SIGNAL_COLORS as DEFAULT_SIGNAL_COLORS2,
  parsePortReference as parsePortReference2,
  createPortReference as createPortReference2,
  createPatchDef,
  createModuleDef,
  createCableDef,
  getSignalColor as getSignalColor2,
  checkPortCompatibility,
  validatePatchDef
} from "@quiver-dsp/types";
function patchToReactFlow(patch, options = {}) {
  const {
    defaultPosition = { x: 0, y: 0 },
    moduleSpacing = 250,
    signalColors = DEFAULT_SIGNAL_COLORS,
    getPortSignalKind
  } = options;
  const nodes = patch.modules.map((module, index) => {
    const position = module.position ? { x: module.position[0], y: module.position[1] } : {
      x: defaultPosition.x + index % 4 * moduleSpacing,
      y: defaultPosition.y + Math.floor(index / 4) * moduleSpacing
    };
    return {
      id: module.name,
      type: "quiverModule",
      position,
      data: {
        moduleType: module.module_type,
        state: module.state,
        label: module.name
      }
    };
  });
  const edges = patch.cables.map((cable, index) => {
    const { moduleName: sourceModule, portName: sourcePort } = parsePortReference(
      cable.from
    );
    const { moduleName: targetModule, portName: targetPort } = parsePortReference(
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
      style: signalKind ? { stroke: getSignalColor(signalKind, signalColors), strokeWidth: 2 } : void 0,
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
    const from = createPortReference(
      edge.source,
      edge.sourceHandle || edge.data?.sourcePort || "out"
    );
    const to = createPortReference(
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
    style: options?.signalKind ? { stroke: getSignalColor(options.signalKind), strokeWidth: 2 } : void 0,
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
    modules: patch.modules.map((module) => {
      const position = positions.get(module.name);
      if (position) {
        return {
          ...module,
          position: [position.x, position.y]
        };
      }
      return module;
    })
  };
}
function getCablesForModule(cables, moduleName) {
  const incoming = [];
  const outgoing = [];
  for (const cable of cables) {
    const from = parsePortReference(cable.from);
    const to = parsePortReference(cable.to);
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
      const from = parsePortReference(c.from);
      const to = parsePortReference(c.to);
      return from.moduleName !== moduleName && to.moduleName !== moduleName;
    })
  };
}
export {
  DEFAULT_SIGNAL_COLORS2 as DEFAULT_SIGNAL_COLORS,
  SIGNAL_KINDS,
  checkPortCompatibility,
  createCableDef,
  createModuleDef,
  createPatchDef,
  createPortReference2 as createPortReference,
  createQuiverEdge,
  createQuiverNode,
  generateModuleName,
  getCablesForModule,
  getSignalColor2 as getSignalColor,
  parsePortReference2 as parsePortReference,
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
};
//# sourceMappingURL=index.mjs.map