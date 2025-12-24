/**
 * Type declarations for @quiver/wasm module
 *
 * These types are stubs for development. The actual types are generated
 * when building the WASM package with wasm-pack.
 *
 * The QuiverEngine interface used by hooks.ts is defined locally in hooks.ts
 * since it's more extensive and we need to use it with React state.
 */
declare module '@quiver/wasm' {
  /**
   * Initialize the WASM module
   */
  export function initWasm(): Promise<void>;

  /**
   * Create a new Quiver engine
   * Returns an engine that conforms to the QuiverEngine interface in hooks.ts
   */
  // eslint-disable-next-line @typescript-eslint/no-explicit-any
  export function createEngine(sampleRate: number): Promise<any>;

  export class QuiverError extends Error {
    constructor(message: string);
  }
}
