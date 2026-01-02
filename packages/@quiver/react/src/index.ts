/**
 * @quiver/react - React utilities for Quiver modular synthesizer
 *
 * This package provides React Flow mapping utilities and hooks for building
 * modular synthesizer UIs with Quiver.
 */

import type { Node, Edge, XYPosition } from '@xyflow/react';

// =============================================================================
// Core Type Definitions (inlined to avoid monorepo resolution issues)
// =============================================================================

/** Signal type classification */
export type SignalKind =
  | 'Audio'
  | 'CvBipolar'
  | 'CvUnipolar'
  | 'VoltPerOctave'
  | 'Gate'
  | 'Trigger'
  | 'Clock';

/** CSS hex color values for each signal type */
export interface SignalColors {
  audio: string;
  cv_bipolar: string;
  cv_unipolar: string;
  volt_per_octave: string;
  gate: string;
  trigger: string;
  clock: string;
}

/** Port definition */
export interface PortDef {
  id: number;
  name: string;
  kind: SignalKind;
  default: number;
  normalled_to?: number;
  has_attenuverter: boolean;
}

/** Port specification for a module */
export interface PortSpec {
  inputs: PortDef[];
  outputs: PortDef[];
}

/** Compatibility status for port connections */
export type Compatibility =
  | { status: 'exact' }
  | { status: 'allowed' }
  | { status: 'warning'; message: string };

/** Module definition for serialization */
export interface ModuleDef {
  name: string;
  module_type: string;
  position?: [number, number];
  state?: unknown;
}

/** Cable definition for serialization */
export interface CableDef {
  from: string;
  to: string;
  attenuation?: number;
  offset?: number;
}

/** Patch definition for serialization */
export interface PatchDef {
  version: number;
  name: string;
  author?: string;
  description?: string;
  tags: string[];
  modules: ModuleDef[];
  cables: CableDef[];
  parameters: Record<string, number>;
}

/** Validation error */
export interface ValidationError {
  path: string;
  message: string;
}

/** Validation result */
export interface ValidationResult {
  valid: boolean;
  errors: ValidationError[];
}

/** Port reference string in format 'module_name.port_name' */
export type PortReference = `${string}.${string}`;

/** Module type identifier */
export type ModuleTypeId = string;

/** Module category */
export type ModuleCategory = string;

// =============================================================================
// Signal Colors
// =============================================================================

/** Default signal colors following modular synth conventions */
export const DEFAULT_SIGNAL_COLORS: SignalColors = {
  audio: '#e94560',
  cv_bipolar: '#0f3460',
  cv_unipolar: '#00b4d8',
  volt_per_octave: '#90be6d',
  gate: '#f9c74f',
  trigger: '#f8961e',
  clock: '#9d4edd',
};

/** Get the color for a signal kind */
export function getSignalColor(
  kind: SignalKind,
  colors: SignalColors = DEFAULT_SIGNAL_COLORS
): string {
  const keyMap: Record<SignalKind, keyof SignalColors> = {
    Audio: 'audio',
    CvBipolar: 'cv_bipolar',
    CvUnipolar: 'cv_unipolar',
    VoltPerOctave: 'volt_per_octave',
    Gate: 'gate',
    Trigger: 'trigger',
    Clock: 'clock',
  };
  return colors[keyMap[kind]];
}

// =============================================================================
// Port Reference Utilities
// =============================================================================

/** Parse a port reference string into module name and port name */
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

/** Create a port reference string from module and port names */
export function createPortReference(
  moduleName: string,
  portName: string
): PortReference {
  return `${moduleName}.${portName}` as PortReference;
}

// =============================================================================
// Patch Utilities
// =============================================================================

/** Create a new empty patch definition */
export function createPatchDef(name: string): PatchDef {
  return {
    version: 1,
    name,
    author: undefined,
    description: undefined,
    tags: [],
    modules: [],
    cables: [],
    parameters: {},
  };
}

/** Create a new module definition */
export function createModuleDef(
  name: string,
  moduleType: ModuleTypeId,
  position?: [number, number]
): ModuleDef {
  return {
    name,
    module_type: moduleType,
    position,
    state: undefined,
  };
}

/** Create a new cable definition */
export function createCableDef(
  from: PortReference,
  to: PortReference,
  options?: { attenuation?: number; offset?: number }
): CableDef {
  return {
    from,
    to,
    attenuation: options?.attenuation,
    offset: options?.offset,
  };
}

// =============================================================================
// Compatibility Checking
// =============================================================================

/** Check if two signal kinds are compatible for connection */
export function checkPortCompatibility(
  from: SignalKind,
  to: SignalKind
): Compatibility {
  if (from === to) {
    return { status: 'exact' };
  }
  if (from === 'Audio') {
    return { status: 'allowed' };
  }
  if (
    (from === 'CvBipolar' && to === 'CvUnipolar') ||
    (from === 'CvUnipolar' && to === 'CvBipolar')
  ) {
    return { status: 'allowed' };
  }
  if (from === 'VoltPerOctave' && (to === 'CvBipolar' || to === 'CvUnipolar')) {
    return { status: 'allowed' };
  }
  if ((from === 'Gate' && to === 'Trigger') || (from === 'Trigger' && to === 'Gate')) {
    return { status: 'allowed' };
  }
  if ((from === 'Clock' && to === 'Gate') || (from === 'Clock' && to === 'Trigger')) {
    return { status: 'allowed' };
  }
  if ((from === 'Gate' || from === 'Trigger') && to === 'Audio') {
    return { status: 'warning', message: 'Gate/Trigger to Audio may cause clicks' };
  }
  if (from === 'CvBipolar' && to === 'VoltPerOctave') {
    return { status: 'warning', message: 'CV to V/Oct may cause tuning issues' };
  }
  return { status: 'allowed' };
}

// =============================================================================
// React Flow Types
// =============================================================================

/** Data payload for Quiver module nodes */
export interface QuiverNodeData extends Record<string, unknown> {
  typeId: ModuleTypeId;
  name: string;
  category?: ModuleCategory;
  inputs?: string[];
  outputs?: string[];
}

/** Data payload for Quiver cable edges */
export interface QuiverEdgeData extends Record<string, unknown> {
  fromPort: string;
  toPort: string;
  signalKind?: SignalKind;
  attenuation?: number;
  offset?: number;
}

/** Quiver module node type for React Flow */
export type QuiverNode = Node<QuiverNodeData, 'quiver-module'>;

/** Quiver cable edge type for React Flow */
export type QuiverEdge = Edge<QuiverEdgeData>;

// =============================================================================
// Patch <-> React Flow Conversion
// =============================================================================

export interface PatchToFlowOptions {
  defaultPosition?: XYPosition;
  moduleSpacing?: number;
}

/** Convert a Quiver PatchDef to React Flow nodes and edges */
export function patchToReactFlow(
  patch: PatchDef,
  options: PatchToFlowOptions = {}
): { nodes: QuiverNode[]; edges: QuiverEdge[] } {
  const { defaultPosition = { x: 100, y: 100 }, moduleSpacing = 250 } = options;

  const nodes: QuiverNode[] = patch.modules.map((module: ModuleDef, index: number) => ({
    id: module.name,
    type: 'quiver-module',
    position: module.position
      ? { x: module.position[0], y: module.position[1] }
      : { x: defaultPosition.x + index * moduleSpacing, y: defaultPosition.y },
    data: {
      typeId: module.module_type,
      name: module.name,
    },
  }));

  const edges: QuiverEdge[] = patch.cables.map((cable: CableDef, index: number) => {
    const fromParsed = parsePortReference(cable.from as PortReference);
    const toParsed = parsePortReference(cable.to as PortReference);

    return {
      id: `cable-${index}`,
      source: fromParsed.moduleName,
      target: toParsed.moduleName,
      sourceHandle: fromParsed.portName,
      targetHandle: toParsed.portName,
      data: {
        fromPort: cable.from,
        toPort: cable.to,
        attenuation: cable.attenuation,
        offset: cable.offset,
      },
    };
  });

  return { nodes, edges };
}

export interface FlowToPatchOptions {
  name: string;
  author?: string;
  description?: string;
  tags?: string[];
}

/** Convert React Flow nodes and edges back to a PatchDef */
export function reactFlowToPatch(
  nodes: QuiverNode[],
  edges: QuiverEdge[],
  options: FlowToPatchOptions
): PatchDef {
  const modules: ModuleDef[] = nodes.map((node) => ({
    name: node.id,
    module_type: node.data.typeId,
    position: [node.position.x, node.position.y] as [number, number],
    state: undefined,
  }));

  const cables: CableDef[] = edges.map((edge) => ({
    from: edge.data?.fromPort ?? createPortReference(edge.source, edge.sourceHandle ?? ''),
    to: edge.data?.toPort ?? createPortReference(edge.target, edge.targetHandle ?? ''),
    attenuation: edge.data?.attenuation,
    offset: edge.data?.offset,
  }));

  return {
    version: 1,
    name: options.name,
    author: options.author,
    description: options.description,
    tags: options.tags ?? [],
    modules,
    cables,
    parameters: {},
  };
}

// =============================================================================
// React Flow Helpers
// =============================================================================

/** Generate a unique module name */
export function generateModuleName(
  typeId: ModuleTypeId,
  existingNames: Set<string>
): string {
  let counter = 1;
  let name = typeId.toLowerCase().replace(/[^a-z0-9]/g, '_');

  while (existingNames.has(name)) {
    name = `${typeId.toLowerCase().replace(/[^a-z0-9]/g, '_')}_${counter}`;
    counter++;
  }

  return name;
}

/** Create a new Quiver module node */
export function createQuiverNode(
  typeId: ModuleTypeId,
  position: XYPosition,
  existingNames: Set<string>
): QuiverNode {
  const name = generateModuleName(typeId, existingNames);

  return {
    id: name,
    type: 'quiver-module',
    position,
    data: {
      typeId,
      name,
    },
  };
}

/** Create a new Quiver cable edge */
export function createQuiverEdge(
  sourceModule: string,
  sourcePort: string,
  targetModule: string,
  targetPort: string,
  signalKind?: SignalKind
): QuiverEdge {
  return {
    id: `${sourceModule}.${sourcePort}-${targetModule}.${targetPort}`,
    source: sourceModule,
    target: targetModule,
    sourceHandle: sourcePort,
    targetHandle: targetPort,
    data: {
      fromPort: createPortReference(sourceModule, sourcePort),
      toPort: createPortReference(targetModule, targetPort),
      signalKind,
    },
  };
}

/** Update module positions in a patch from React Flow nodes */
export function updatePatchPositions(patch: PatchDef, nodes: QuiverNode[]): PatchDef {
  const positionMap = new Map(
    nodes.map((n) => [n.id, [n.position.x, n.position.y] as [number, number]])
  );

  return {
    ...patch,
    modules: patch.modules.map((m: ModuleDef) => ({
      ...m,
      position: positionMap.get(m.name) ?? m.position,
    })),
  };
}

/** Get all cables connected to a module */
export function getCablesForModule(
  patch: PatchDef,
  moduleName: string
): CableDef[] {
  return patch.cables.filter((c: CableDef) => {
    const from = parsePortReference(c.from as PortReference);
    const to = parsePortReference(c.to as PortReference);
    return from.moduleName === moduleName || to.moduleName === moduleName;
  });
}

/** Remove a module and its cables from a patch */
export function removeModuleFromPatch(
  patch: PatchDef,
  moduleName: string
): PatchDef {
  return {
    ...patch,
    modules: patch.modules.filter((m: ModuleDef) => m.name !== moduleName),
    cables: patch.cables.filter((c: CableDef) => {
      const from = parsePortReference(c.from as PortReference);
      const to = parsePortReference(c.to as PortReference);
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
  type ObservableValue,
  type SubscriptionTarget,
  type CatalogResponse,
  type ModuleCatalogEntry,
  type PortSummary,
  // Functions
  getObservableValueKey,
  getSubscriptionTargetKey,
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
