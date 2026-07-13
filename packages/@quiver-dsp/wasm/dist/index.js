import {
  createQuiverAudio,
  createQuiverAudioNode
} from "./chunk-NJXZK6MB.js";

// src/index.ts
import { QuiverEngine, QuiverError } from "../quiver";
var wasmInitPromise = null;
async function initWasm() {
  if (!wasmInitPromise) {
    wasmInitPromise = import("../quiver").then((wasm) => wasm.default());
  }
  await wasmInitPromise;
}
async function createEngine(sampleRate) {
  await initWasm();
  const { QuiverEngine: QuiverEngine2 } = await import("../quiver");
  return new QuiverEngine2(sampleRate);
}
export {
  QuiverEngine,
  QuiverError,
  createEngine,
  createQuiverAudio,
  createQuiverAudioNode,
  initWasm
};
//# sourceMappingURL=index.js.map