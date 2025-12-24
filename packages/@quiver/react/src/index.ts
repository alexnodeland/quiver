/**
 * @quiver/react - React utilities for Quiver modular synthesizer
 *
 * This package provides React Flow mapping utilities and hooks for building
 * modular synthesizer UIs with Quiver.
 */

import type { Node, Edge, XYPosition } from '@xyflow/react';
import type {
  PatchDef,
  ModuleDef,
  CableDef,
  PortReference,
  SignalKind,
  ModuleTypeId,
} from '@quiver/types';
import {
  parsePortReference,
  createPortReference,
  getSignalColor,
  DEFAULT_SIGNAL_COLORS,
} from '@quiver/types';

// =============================================================================
// React Flow Types
// =============================================================================

/**
 * Data payload for Quiver module nodes
 */
export interface QuiverNodeData extends Record<string, unknown> {
  /** Module type identifier */
  moduleType: ModuleTypeId;
  /** Module-specific state */
  state?: Record<string, unknown>;
  /** Display label (defaults to node id) */
  label?: string;
}

/**
 * Data payload for Quiver cable edges
 */
export interface QuiverEdgeData extends Record<string, unknown> {
  /** Source port name */
  sourcePort: string;
  /** Target port name */
  targetPort: string;
  /** Signal type for coloring */
  signalKind?: SignalKind;
  /** Attenuation value (-2.0 to 2.0) */
  attenuation?: number;
  /** DC offset value (-10.0 to 10.0V) */
  offset?: number;
}

/**
 * Quiver-typed React Flow Node
 */
export type QuiverNode = Node<QuiverNodeData, 'quiverModule'>;

/**
 * Quiver-typed React Flow Edge
 */
export type QuiverEdge = Edge<QuiverEdgeData>;

// =============================================================================
// Patch to React Flow Conversion
// =============================================================================

/**
 * Options for converting a patch to React Flow format
 */
export interface PatchToReactFlowOptions {
  /** Default position for modules without position data */
  defaultPosition?: XYPosition;
  /** Spacing between modules when auto-positioning */
  moduleSpacing?: number;
  /** Signal colors for edge styling */
  signalColors?: typeof DEFAULT_SIGNAL_COLORS;
  /** Callback to determine signal kind for a port */
  getPortSignalKind?: (
    moduleType: ModuleTypeId,
    portName: string,
    isOutput: boolean
  ) => SignalKind | undefined;
}

/**
 * Result of converting a patch to React Flow format
 */
export interface ReactFlowPatch {
  nodes: QuiverNode[];
  edges: QuiverEdge[];
}

/**
 * Convert a Quiver patch definition to React Flow nodes and edges
 */
export function patchToReactFlow(
  patch: PatchDef,
  options: PatchToReactFlowOptions = {}
): ReactFlowPatch {
  const {
    defaultPosition = { x: 0, y: 0 },
    moduleSpacing = 250,
    signalColors = DEFAULT_SIGNAL_COLORS,
    getPortSignalKind,
  } = options;

  // Convert modules to nodes
  const nodes: QuiverNode[] = patch.modules.map((module, index) => {
    // Auto-position if no position specified
    const position: XYPosition = module.position
      ? { x: module.position[0], y: module.position[1] }
      : {
          x: defaultPosition.x + (index % 4) * moduleSpacing,
          y: defaultPosition.y + Math.floor(index / 4) * moduleSpacing,
        };

    return {
      id: module.name,
      type: 'quiverModule',
      position,
      data: {
        moduleType: module.module_type,
        state: module.state,
        label: module.name,
      },
    };
  });

  // Convert cables to edges
  const edges: QuiverEdge[] = patch.cables.map((cable, index) => {
    const { moduleName: sourceModule, portName: sourcePort } = parsePortReference(
      cable.from
    );
    const { moduleName: targetModule, portName: targetPort } = parsePortReference(
      cable.to
    );

    // Try to determine signal kind for coloring
    let signalKind: SignalKind | undefined;
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
      type: 'default',
      style: signalKind
        ? { stroke: getSignalColor(signalKind, signalColors), strokeWidth: 2 }
        : undefined,
      data: {
        sourcePort,
        targetPort,
        signalKind,
        attenuation: cable.attenuation,
        offset: cable.offset,
      },
    };
  });

  return { nodes, edges };
}

// =============================================================================
// React Flow to Patch Conversion
// =============================================================================

/**
 * Metadata for the patch when converting from React Flow
 */
export interface PatchMetadata {
  name: string;
  author?: string;
  description?: string;
  tags?: string[];
}

/**
 * Convert React Flow nodes and edges back to a Quiver patch definition
 */
export function reactFlowToPatch(
  nodes: QuiverNode[],
  edges: QuiverEdge[],
  metadata: PatchMetadata
): PatchDef {
  // Convert nodes to modules
  const modules: ModuleDef[] = nodes.map((node) => ({
    name: node.id,
    module_type: node.data.moduleType,
    position: [node.position.x, node.position.y] as [number, number],
    state: node.data.state,
  }));

  // Convert edges to cables
  const cables: CableDef[] = edges.map((edge) => {
    const from = createPortReference(
      edge.source,
      edge.sourceHandle || edge.data?.sourcePort || 'out'
    );
    const to = createPortReference(
      edge.target,
      edge.targetHandle || edge.data?.targetPort || 'in'
    );

    const cable: CableDef = { from, to };

    if (edge.data?.attenuation !== undefined) {
      cable.attenuation = edge.data.attenuation;
    }
    if (edge.data?.offset !== undefined) {
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
    parameters: {},
  };
}

// =============================================================================
// Utility Functions
// =============================================================================

/**
 * Generate a unique module name
 */
export function generateModuleName(
  moduleType: ModuleTypeId,
  existingNames: Set<string>
): string {
  let counter = 1;
  let name: string = moduleType;

  while (existingNames.has(name)) {
    name = `${moduleType}_${counter}`;
    counter++;
  }

  return name;
}

/**
 * Create a new Quiver node for adding to the graph
 */
export function createQuiverNode(
  moduleType: ModuleTypeId,
  position: XYPosition,
  existingNames: Set<string>
): QuiverNode {
  const name = generateModuleName(moduleType, existingNames);

  return {
    id: name,
    type: 'quiverModule',
    position,
    data: {
      moduleType,
      label: name,
    },
  };
}

/**
 * Create a new Quiver edge for adding to the graph
 */
export function createQuiverEdge(
  sourceNode: string,
  sourcePort: string,
  targetNode: string,
  targetPort: string,
  options?: {
    signalKind?: SignalKind;
    attenuation?: number;
    offset?: number;
  }
): QuiverEdge {
  const id = `cable-${sourceNode}-${sourcePort}-${targetNode}-${targetPort}`;

  return {
    id,
    source: sourceNode,
    sourceHandle: sourcePort,
    target: targetNode,
    targetHandle: targetPort,
    style: options?.signalKind
      ? { stroke: getSignalColor(options.signalKind), strokeWidth: 2 }
      : undefined,
    data: {
      sourcePort,
      targetPort,
      signalKind: options?.signalKind,
      attenuation: options?.attenuation,
      offset: options?.offset,
    },
  };
}

/**
 * Update node positions in a patch definition
 */
export function updatePatchPositions(
  patch: PatchDef,
  positions: Map<string, XYPosition>
): PatchDef {
  return {
    ...patch,
    modules: patch.modules.map((module) => {
      const position = positions.get(module.name);
      if (position) {
        return {
          ...module,
          position: [position.x, position.y] as [number, number],
        };
      }
      return module;
    }),
  };
}

/**
 * Find all cables connected to a module
 */
export function getCablesForModule(
  cables: CableDef[],
  moduleName: string
): { incoming: CableDef[]; outgoing: CableDef[] } {
  const incoming: CableDef[] = [];
  const outgoing: CableDef[] = [];

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

/**
 * Remove a module and all its cables from a patch
 */
export function removeModuleFromPatch(
  patch: PatchDef,
  moduleName: string
): PatchDef {
  return {
    ...patch,
    modules: patch.modules.filter((m) => m.name !== moduleName),
    cables: patch.cables.filter((c) => {
      const from = parsePortReference(c.from);
      const to = parsePortReference(c.to);
      return from.moduleName !== moduleName && to.moduleName !== moduleName;
    }),
  };
}

// =============================================================================
// WASM Bridge Hooks
// =============================================================================

export {
  // Types
  type QuiverEngine,
  // Hooks
  useQuiverUpdates,
  useQuiverParam,
  useQuiverLevel,
  useQuiverGate,
  useQuiverCatalog,
  useQuiverSearch,
  useQuiverPatch,
  useQuiverEngine,
} from './hooks';

// =============================================================================
// Re-exports from @quiver/types
// =============================================================================

export {
  // Types
  type PatchDef,
  type ModuleDef,
  type CableDef,
  type PortReference,
  type SignalKind,
  type ModuleTypeId,
  type ModuleCategory,
  type ModuleMetadata,
  type PortDef,
  type PortSpec,
  type SignalColors,
  type Compatibility,
  type ValidationResult,
  type ValidationError,
  // Constants
  SIGNAL_KINDS,
  DEFAULT_SIGNAL_COLORS,
  // Functions
  parsePortReference,
  createPortReference,
  createPatchDef,
  createModuleDef,
  createCableDef,
  getSignalColor,
  checkPortCompatibility,
  validatePatchDef,
} from '@quiver/types';
