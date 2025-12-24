/**
 * Type declarations for @quiver/wasm/quiver module
 *
 * These types are stubs for development. The actual types are generated
 * when building the WASM package with wasm-pack.
 */
declare module '@quiver/wasm/quiver' {
  /**
   * Initialize the WASM module (default export)
   */
  export default function init(): Promise<void>;

  /**
   * Quiver engine class
   */
  export class QuiverEngine {
    constructor(sampleRate: number);

    // Module operations
    add_module(typeId: string, name: string): void;
    remove_module(name: string): void;
    module_count(): number;
    cable_count(): number;

    // Connection operations
    connect(from: string, to: string): void;
    disconnect(from: string, to: string): void;
    connect_attenuated(from: string, to: string, attenuation: number): void;

    // Parameters
    set_param(nodeName: string, paramIndex: number, value: number): void;
    set_param_by_name(nodeName: string, paramName: string, value: number): void;

    // Patch operations
    set_output(name: string): void;
    compile(): void;
    reset(): void;
    load_patch(patch: unknown): void;
    save_patch(name: string): unknown;
    clear_patch(): void;
    validate_patch(patch: unknown): { valid: boolean; errors?: string[] };

    // Audio processing
    tick(): [number, number];
    process_block(numSamples: number): Float32Array;

    // Catalog
    get_catalog(): {
      modules: Array<{ type_id: string; name: string; category?: string }>;
      categories: string[];
    };
    get_categories(): string[];
    get_port_spec(typeId: string): {
      inputs: Array<{ name: string; signal_kind?: string }>;
      outputs: Array<{ name: string; signal_kind?: string }>;
    };

    // MIDI
    midi_note_on(note: number, velocity: number): void;
    midi_note_off(note: number, velocity: number): void;
    midi_cc(cc: number, value: number): void;
    midi_pitch_bend(value: number): void;

    // Cleanup
    free(): void;
  }

  export class QuiverError extends Error {
    constructor(message: string);
  }
}
