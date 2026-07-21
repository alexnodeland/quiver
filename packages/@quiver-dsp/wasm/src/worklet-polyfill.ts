/**
 * TextDecoder/TextEncoder polyfill for AudioWorkletGlobalScope.
 *
 * The wasm-bindgen glue constructs `new TextDecoder(...)` / `new TextEncoder()` at
 * module top level, but the Worklet spec exposes neither (they exist on Window and
 * Worker only). Without this shim the bundled worklet module throws during
 * evaluation, `registerProcessor` never runs, and — because Chromium resolves
 * `addModule()` even when evaluation fails — the failure only surfaces later as
 * "The node name 'quiver-processor' is not defined in AudioWorkletGlobalScope".
 *
 * This module MUST be imported before the glue (it is the first import in
 * `worklet.ts`; ESM evaluates imports in order). It implements just what the glue
 * uses: `decode()` over WASM memory views and `encode`/`encodeInto` for strings
 * crossing into WASM. Pure UTF-8, no surrogate-pair edge cases skipped.
 */

/* eslint-disable @typescript-eslint/no-explicit-any */

const globalScope = globalThis as any;

if (typeof globalScope.TextDecoder === 'undefined') {
  class TextDecoderPolyfill {
    // The glue always constructs with 'utf-8'; accept and ignore the label/options.
    decode(input?: ArrayBufferView | ArrayBuffer): string {
      if (input === undefined) return '';
      const bytes =
        input instanceof Uint8Array
          ? input
          : ArrayBuffer.isView(input)
            ? new Uint8Array(input.buffer, input.byteOffset, input.byteLength)
            : new Uint8Array(input);
      let out = '';
      let i = 0;
      const len = bytes.length;
      while (i < len) {
        const b0 = bytes[i++];
        let cp: number;
        if (b0 < 0x80) {
          cp = b0;
        } else if (b0 < 0xe0) {
          cp = ((b0 & 0x1f) << 6) | (bytes[i++] & 0x3f);
        } else if (b0 < 0xf0) {
          cp = ((b0 & 0x0f) << 12) | ((bytes[i++] & 0x3f) << 6) | (bytes[i++] & 0x3f);
        } else {
          cp =
            ((b0 & 0x07) << 18) |
            ((bytes[i++] & 0x3f) << 12) |
            ((bytes[i++] & 0x3f) << 6) |
            (bytes[i++] & 0x3f);
        }
        if (cp < 0x10000) {
          out += String.fromCharCode(cp);
        } else {
          cp -= 0x10000;
          out += String.fromCharCode(0xd800 + (cp >> 10), 0xdc00 + (cp & 0x3ff));
        }
      }
      return out;
    }
  }
  globalScope.TextDecoder = TextDecoderPolyfill;
}

if (typeof globalScope.TextEncoder === 'undefined') {
  class TextEncoderPolyfill {
    encode(input = ''): Uint8Array {
      // Worst case 3 bytes per UTF-16 code unit (surrogate pairs: 2 units -> 4 bytes).
      const buf = new Uint8Array(input.length * 3);
      const { written } = this.encodeInto(input, buf);
      return buf.subarray(0, written);
    }

    encodeInto(input: string, dest: Uint8Array): { read: number; written: number } {
      let read = 0;
      let written = 0;
      const len = input.length;
      while (read < len) {
        let cp = input.charCodeAt(read);
        // Combine surrogate pairs into a single code point.
        if (cp >= 0xd800 && cp <= 0xdbff && read + 1 < len) {
          const lo = input.charCodeAt(read + 1);
          if (lo >= 0xdc00 && lo <= 0xdfff) {
            cp = 0x10000 + ((cp - 0xd800) << 10) + (lo - 0xdc00);
          }
        }
        const bytesNeeded = cp < 0x80 ? 1 : cp < 0x800 ? 2 : cp < 0x10000 ? 3 : 4;
        if (written + bytesNeeded > dest.length) break;
        if (cp < 0x80) {
          dest[written++] = cp;
        } else if (cp < 0x800) {
          dest[written++] = 0xc0 | (cp >> 6);
          dest[written++] = 0x80 | (cp & 0x3f);
        } else if (cp < 0x10000) {
          dest[written++] = 0xe0 | (cp >> 12);
          dest[written++] = 0x80 | ((cp >> 6) & 0x3f);
          dest[written++] = 0x80 | (cp & 0x3f);
        } else {
          dest[written++] = 0xf0 | (cp >> 18);
          dest[written++] = 0x80 | ((cp >> 12) & 0x3f);
          dest[written++] = 0x80 | ((cp >> 6) & 0x3f);
          dest[written++] = 0x80 | (cp & 0x3f);
        }
        read += cp >= 0x10000 ? 2 : 1;
      }
      return { read, written };
    }
  }
  globalScope.TextEncoder = TextEncoderPolyfill;
}

export {};
