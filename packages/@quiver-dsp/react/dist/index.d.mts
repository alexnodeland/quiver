import { XYPosition, Edge, Node } from '@xyflow/react';
import { CatalogResponse, ModuleCatalogEntry, SubscriptionTarget, ObservableValue, DEFAULT_SIGNAL_COLORS, ModuleTypeId, SignalKind, CableDef, PatchDef } from '@quiver-dsp/types';
export { CableDef, Compatibility, DEFAULT_SIGNAL_COLORS, ModuleCategory, ModuleDef, ModuleMetadata, ModuleTypeId, PatchDef, PortDef, PortReference, PortSpec, SIGNAL_KINDS, SignalColors, SignalKind, ValidationError, ValidationResult, checkPortCompatibility, createCableDef, createModuleDef, createPatchDef, createPortReference, getSignalColor, parsePortReference, validatePatchDef } from '@quiver-dsp/types';
import { QuiverEngine } from '@quiver-dsp/wasm';
export { QuiverEngine } from '@quiver-dsp/wasm';

/**
 * React hooks for WASM bridge integration
 *
 * These hooks provide reactive bindings for the Quiver WASM engine,
 * enabling real-time updates of parameters, levels, and other values.
 *
 * The `'use client'` directive is injected at the top of the built bundle by tsup
 * (see tsup.config.ts banner) so these hooks work in React Server Component setups.
 */

/**
 * Hook for subscribing to real-time Quiver value updates
 */
declare function useQuiverUpdates(engine: QuiverEngine | null, targets: SubscriptionTarget[]): Map<string, ObservableValue>;
/**
 * Hook for a single parameter value with setter
 */
declare function useQuiverParam(engine: QuiverEngine | null, nodeId: string, paramIndex: number): [number, (value: number) => void];
/**
 * Hook for level meter values
 */
declare function useQuiverLevel(engine: QuiverEngine | null, nodeId: string, portId: number): {
    rmsDb: number;
    peakDb: number;
};
/**
 * Hook for gate state
 */
declare function useQuiverGate(engine: QuiverEngine | null, nodeId: string, portId: number): boolean;
/**
 * Hook for module catalog
 */
declare function useQuiverCatalog(engine: QuiverEngine | null): CatalogResponse | null;
/**
 * Hook for searching modules
 */
declare function useQuiverSearch(engine: QuiverEngine | null, query: string): ModuleCatalogEntry[];
/**
 * Hook for loading and managing a patch
 */
declare function useQuiverPatch(engine: QuiverEngine | null): {
    isLoaded: boolean;
    error: Error | null;
    loadPatch: (patch: unknown) => Promise<void>;
    savePatch: (name: string) => any;
    clearPatch: () => void;
};
/**
 * Hook for managing engine initialization
 */
declare function useQuiverEngine(sampleRate?: number): {
    engine: QuiverEngine | null;
    isReady: boolean;
    error: Error | null;
};

/**
 * @quiver-dsp/react - React utilities for Quiver modular synthesizer
 *
 * This package provides React Flow mapping utilities and hooks for building
 * modular synthesizer UIs with Quiver.
 */

/**
 * Data payload for Quiver module nodes.
 *
 * The `[key: string]: unknown` index signature satisfies React Flow's
 * `Node<Data extends Record<string, unknown>>` constraint (@xyflow/react v12).
 */
interface QuiverNodeData {
    /** Module type identifier */
    moduleType: ModuleTypeId;
    /** Module-specific state */
    state?: Record<string, unknown>;
    /** Display label (defaults to node id) */
    label?: string;
    [key: string]: unknown;
}
/**
 * Data payload for Quiver cable edges.
 *
 * The `[key: string]: unknown` index signature satisfies React Flow's
 * `Edge<Data extends Record<string, unknown>>` constraint (@xyflow/react v12).
 */
interface QuiverEdgeData {
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
    [key: string]: unknown;
}
/**
 * Quiver-typed React Flow Node
 */
type QuiverNode = Node<QuiverNodeData, 'quiverModule'>;
/**
 * Quiver-typed React Flow Edge
 */
type QuiverEdge = Edge<QuiverEdgeData>;
/**
 * Options for converting a patch to React Flow format
 */
interface PatchToReactFlowOptions {
    /** Default position for modules without position data */
    defaultPosition?: XYPosition;
    /** Spacing between modules when auto-positioning */
    moduleSpacing?: number;
    /** Signal colors for edge styling */
    signalColors?: typeof DEFAULT_SIGNAL_COLORS;
    /** Callback to determine signal kind for a port */
    getPortSignalKind?: (moduleType: ModuleTypeId, portName: string, isOutput: boolean) => SignalKind | undefined;
}
/**
 * Result of converting a patch to React Flow format
 */
interface ReactFlowPatch {
    nodes: QuiverNode[];
    edges: QuiverEdge[];
}
/**
 * Convert a Quiver patch definition to React Flow nodes and edges
 */
declare function patchToReactFlow(patch: PatchDef, options?: PatchToReactFlowOptions): ReactFlowPatch;
/**
 * Metadata for the patch when converting from React Flow
 */
interface PatchMetadata {
    name: string;
    author?: string;
    description?: string;
    tags?: string[];
}
/**
 * Convert React Flow nodes and edges back to a Quiver patch definition
 */
declare function reactFlowToPatch(nodes: QuiverNode[], edges: QuiverEdge[], metadata: PatchMetadata): PatchDef;
/**
 * Generate a unique module name
 */
declare function generateModuleName(moduleType: ModuleTypeId, existingNames: Set<string>): string;
/**
 * Create a new Quiver node for adding to the graph
 */
declare function createQuiverNode(moduleType: ModuleTypeId, position: XYPosition, existingNames: Set<string>): QuiverNode;
/**
 * Create a new Quiver edge for adding to the graph
 */
declare function createQuiverEdge(sourceNode: string, sourcePort: string, targetNode: string, targetPort: string, options?: {
    signalKind?: SignalKind;
    attenuation?: number;
    offset?: number;
}): QuiverEdge;
/**
 * Update node positions in a patch definition
 */
declare function updatePatchPositions(patch: PatchDef, positions: Map<string, XYPosition>): PatchDef;
/**
 * Find all cables connected to a module
 */
declare function getCablesForModule(cables: CableDef[], moduleName: string): {
    incoming: CableDef[];
    outgoing: CableDef[];
};
/**
 * Remove a module and all its cables from a patch
 */
declare function removeModuleFromPatch(patch: PatchDef, moduleName: string): PatchDef;

export { type PatchMetadata, type PatchToReactFlowOptions, type QuiverEdge, type QuiverEdgeData, type QuiverNode, type QuiverNodeData, type ReactFlowPatch, createQuiverEdge, createQuiverNode, generateModuleName, getCablesForModule, patchToReactFlow, reactFlowToPatch, removeModuleFromPatch, updatePatchPositions, useQuiverCatalog, useQuiverEngine, useQuiverGate, useQuiverLevel, useQuiverParam, useQuiverPatch, useQuiverSearch, useQuiverUpdates };
