/**
 * React hooks for WASM bridge integration
 *
 * These hooks provide reactive bindings for the Quiver WASM engine,
 * enabling real-time updates of parameters, levels, and other values.
 */

import { useEffect, useState, useRef, useCallback, useMemo } from 'react';
import type {
  ObservableValue,
  SubscriptionTarget,
  CatalogResponse,
  ModuleCatalogEntry,
} from '@quiver/types';
import {
  getObservableValueKey,
  getSubscriptionTargetKey,
} from '@quiver/types';

/**
 * Type for the QuiverEngine WASM instance
 * This matches the API exposed by the Rust wasm-bindgen bindings in src/wasm/engine.rs
 *
 * Note: Methods that return `unknown` wrap Rust's `Result<JsValue, JsValue>` -
 * they may throw errors and return JSON-serialized data that needs parsing.
 */
export interface QuiverEngine {
  // Catalog & Introspection
  get_catalog(): CatalogResponse;
  search_modules(query: string): ModuleCatalogEntry[];
  get_modules_by_category(category: string): ModuleCatalogEntry[];
  get_categories(): string[];
  get_port_spec(typeId: string): unknown;

  // Signal semantics
  get_signal_colors(): unknown;
  check_compatibility(from: string, to: string): unknown;

  // Patch operations
  load_patch(patch: unknown): void;
  save_patch(name: string): unknown;
  validate_patch(patch: unknown): unknown;
  clear_patch(): void;

  // Module operations
  add_module(typeId: string, name: string): void;
  remove_module(name: string): void;
  set_module_position(name: string, x: number, y: number): void;
  get_module_position(name: string): [number, number] | null;
  module_count(): number;
  cable_count(): number;
  get_module_names(): string[];
  set_output(name: string): void;

  // Cable operations
  connect(from: string, to: string): void;
  connect_attenuated(from: string, to: string, attenuation: number): void;
  connect_modulated(
    from: string,
    to: string,
    attenuation: number,
    offset: number
  ): void;
  disconnect(from: string, to: string): void;
  disconnect_by_index(index: number): void;

  // Parameters
  get_params(nodeName: string): unknown;
  set_param(nodeName: string, paramIndex: number, value: number): void;
  get_param(nodeName: string, paramIndex: number): number;
  set_param_by_name(nodeName: string, paramName: string, value: number): void;

  // Observer / Real-time bridge
  subscribe(targets: SubscriptionTarget[]): void;
  unsubscribe(targetIds: string[]): void;
  clear_subscriptions(): void;
  poll_updates(): ObservableValue[];
  pending_update_count(): number;

  // Audio processing
  tick(): Float64Array;
  process_block(numSamples: number): Float32Array;
  reset(): void;
  compile(): void;

  // MIDI
  midi_note_on(note: number, velocity: number): void;
  midi_note_off(note: number, velocity: number): void;
  midi_cc(cc: number, value: number): void;
  get_midi_cc(cc: number): number;
  midi_pitch_bend(value: number): void;

  // Properties (getters)
  readonly sample_rate: number;
  readonly midi_note: number;
  readonly midi_velocity: number;
  readonly midi_gate: boolean;
  readonly pitch_bend: number;

  // Cleanup
  free(): void;
}

/**
 * Hook for subscribing to real-time Quiver value updates
 */
export function useQuiverUpdates(
  engine: QuiverEngine | null,
  targets: SubscriptionTarget[]
): Map<string, ObservableValue> {
  const [values, setValues] = useState<Map<string, ObservableValue>>(new Map());
  const targetsRef = useRef(targets);
  const frameRef = useRef<number>();

  // Deep compare targets
  const targetsKey = useMemo(
    () => JSON.stringify(targets.map(getSubscriptionTargetKey).sort()),
    [targets]
  );

  useEffect(() => {
    if (!engine) return;

    // Subscribe to targets
    engine.subscribe(targets);
    targetsRef.current = targets;

    // Poll for updates using requestAnimationFrame
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
        console.error('Error polling Quiver updates:', e);
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

/**
 * Hook for a single parameter value with setter
 */
export function useQuiverParam(
  engine: QuiverEngine | null,
  nodeId: string,
  paramIndex: number
): [number, (value: number) => void] {
  const [value, setValue] = useState(0);

  // Get initial value
  useEffect(() => {
    if (!engine || !nodeId) return;

    try {
      const v = engine.get_param(nodeId, paramIndex);
      setValue(v);
    } catch (e) {
      console.error('Error getting param:', e);
    }
  }, [engine, nodeId, paramIndex]);

  const setParam = useCallback(
    (newValue: number) => {
      if (!engine) return;
      try {
        engine.set_param(nodeId, paramIndex, newValue);
        setValue(newValue);
      } catch (e) {
        console.error('Error setting param:', e);
      }
    },
    [engine, nodeId, paramIndex]
  );

  return [value, setParam];
}

/**
 * Hook for level meter values
 */
export function useQuiverLevel(
  engine: QuiverEngine | null,
  nodeId: string,
  portId: number
): { rmsDb: number; peakDb: number } {
  const targets = useMemo(
    () => [{ type: 'level' as const, node_id: nodeId, port_id: portId }],
    [nodeId, portId]
  );

  const updates = useQuiverUpdates(engine, targets);

  const key = `level:${nodeId}:${portId}`;
  const update = updates.get(key);

  if (update?.type === 'level') {
    return { rmsDb: update.rms_db, peakDb: update.peak_db };
  }

  return { rmsDb: -Infinity, peakDb: -Infinity };
}

/**
 * Hook for gate state
 */
export function useQuiverGate(
  engine: QuiverEngine | null,
  nodeId: string,
  portId: number
): boolean {
  const targets = useMemo(
    () => [{ type: 'gate' as const, node_id: nodeId, port_id: portId }],
    [nodeId, portId]
  );

  const updates = useQuiverUpdates(engine, targets);

  const key = `gate:${nodeId}:${portId}`;
  const update = updates.get(key);

  if (update?.type === 'gate') {
    return update.active;
  }

  return false;
}

/**
 * Hook for module catalog
 */
export function useQuiverCatalog(
  engine: QuiverEngine | null
): CatalogResponse | null {
  const [catalog, setCatalog] = useState<CatalogResponse | null>(null);

  useEffect(() => {
    if (!engine) return;

    try {
      const cat = engine.get_catalog();
      setCatalog(cat);
    } catch (e) {
      console.error('Error getting catalog:', e);
    }
  }, [engine]);

  return catalog;
}

/**
 * Hook for searching modules
 */
export function useQuiverSearch(
  engine: QuiverEngine | null,
  query: string
): ModuleCatalogEntry[] {
  const [results, setResults] = useState<ModuleCatalogEntry[]>([]);

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
      console.error('Error searching modules:', e);
      setResults([]);
    }
  }, [engine, query]);

  return results;
}

/**
 * Hook for loading and managing a patch
 */
export function useQuiverPatch(engine: QuiverEngine | null) {
  const [isLoaded, setIsLoaded] = useState(false);
  const [error, setError] = useState<Error | null>(null);

  const loadPatch = useCallback(
    async (patch: unknown) => {
      if (!engine) return;

      try {
        setError(null);
        engine.load_patch(patch);
        engine.compile();
        setIsLoaded(true);
      } catch (e) {
        setError(e as Error);
        setIsLoaded(false);
      }
    },
    [engine]
  );

  const savePatch = useCallback(
    (name: string) => {
      if (!engine) return null;

      try {
        return engine.save_patch(name);
      } catch (e) {
        setError(e as Error);
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
      setError(e as Error);
    }
  }, [engine]);

  return {
    isLoaded,
    error,
    loadPatch,
    savePatch,
    clearPatch,
  };
}

/**
 * Hook for managing engine initialization
 */
export function useQuiverEngine(sampleRate: number = 44100) {
  const [engine, setEngine] = useState<QuiverEngine | null>(null);
  const [isReady, setIsReady] = useState(false);
  const [error, setError] = useState<Error | null>(null);

  useEffect(() => {
    let mounted = true;

    async function init() {
      try {
        // Dynamic import the WASM module
        const { createEngine } = await import('@quiver/wasm');
        const eng = await createEngine(sampleRate);

        if (mounted) {
          setEngine(eng);
          setIsReady(true);
        }
      } catch (e) {
        if (mounted) {
          setError(e as Error);
        }
      }
    }

    init();

    return () => {
      mounted = false;
    };
  }, [sampleRate]);

  return { engine, isReady, error };
}
