import { test, expect } from '@playwright/test';
import * as path from 'path';
import { fileURLToPath } from 'url';

const __dirname = path.dirname(fileURLToPath(import.meta.url));

// Real AudioWorklet integration: load the actual built worklet bundle
// (dist/worklet.js), register the processor, and boot the WASM engine inside the
// AudioWorkletGlobalScope. This is the demo's true audio path, and it is exactly
// what the API-level tests miss: the worklet scope has no TextDecoder/TextEncoder,
// so a glue regression there only surfaces when the real bundle is evaluated in a
// real worklet (Chromium resolves addModule() even when module evaluation throws,
// masking the failure until node construction).

const wasmPkgRoot = path.resolve(__dirname, '../../../../packages/@quiver-dsp/wasm');
// Vite dev server escape hatch for files outside the served root.
const workletUrl = `/@fs/${wasmPkgRoot}/dist/worklet.js`;
const wasmUrl = `/@fs/${wasmPkgRoot}/quiver_bg.wasm`;

test.describe('AudioWorklet integration (real worklet bundle)', () => {
  test('worklet registers, engine boots, and a patch renders in the worklet', async ({
    page,
    browserName,
  }) => {
    test.skip(browserName === 'webkit', 'WebKit in CI lacks stable AudioWorklet timing');

    await page.goto('/');

    const result = await page.evaluate(
      async ({ workletUrl, wasmUrl }) => {
        const ctx = new AudioContext({ sampleRate: 44100 });
        try {
          const wasmBytes = await fetch(wasmUrl).then((r) => {
            if (!r.ok) throw new Error(`wasm fetch failed: ${r.status}`);
            return r.arrayBuffer();
          });

          await ctx.audioWorklet.addModule(workletUrl);

          // Throws "node name not defined" if the worklet module failed to evaluate
          // (e.g. missing TextDecoder polyfill) — the regression this test pins.
          const node = new AudioWorkletNode(ctx, 'quiver-processor', {
            numberOfInputs: 0,
            numberOfOutputs: 1,
            outputChannelCount: [2],
          });

          const ready = new Promise<Record<string, unknown>>((resolve, reject) => {
            const timer = setTimeout(() => reject(new Error('worklet init timeout')), 10000);
            node.port.onmessage = (e: MessageEvent) => {
              if (e.data?.type === 'ready') {
                clearTimeout(timer);
                resolve(e.data);
              } else if (e.data?.type === 'error') {
                clearTimeout(timer);
                reject(new Error(String(e.data.error)));
              }
            };
          });
          node.port.postMessage({ type: 'init', wasmBytes, sampleRate: 44100 }, [wasmBytes]);
          await ready;

          // Build a minimal audible patch inside the worklet engine and make sure a
          // structural round-trip (save_patch) comes back coherent.
          const ack = new Promise<Record<string, unknown>>((resolve, reject) => {
            const timer = setTimeout(() => reject(new Error('save_patch timeout')), 10000);
            node.port.onmessage = (e: MessageEvent) => {
              if (e.data?.type === 'patch_saved') {
                clearTimeout(timer);
                resolve(e.data);
              } else if (e.data?.type === 'error') {
                clearTimeout(timer);
                reject(new Error(String(e.data.error)));
              }
            };
          });
          node.port.postMessage({ type: 'add_module', typeId: 'vco', name: 'osc' });
          node.port.postMessage({ type: 'add_module', typeId: 'stereo_output', name: 'out' });
          node.port.postMessage({ type: 'connect', from: 'osc.saw', to: 'out.left' });
          node.port.postMessage({ type: 'set_output', name: 'out' });
          node.port.postMessage({ type: 'save_patch', name: 'smoke', requestId: 1 });
          const saved = (await ack) as { patch?: { modules?: unknown[]; cables?: unknown[] } };

          node.port.postMessage({ type: 'destroy' });
          await ctx.close();
          return {
            ok: true,
            modules: saved.patch?.modules?.length ?? 0,
            cables: saved.patch?.cables?.length ?? 0,
            error: null as string | null,
          };
        } catch (e) {
          await ctx.close();
          return { ok: false, modules: 0, cables: 0, error: (e as Error).message };
        }
      },
      { workletUrl, wasmUrl }
    );

    expect(result.error).toBeNull();
    expect(result.ok).toBe(true);
    expect(result.modules).toBe(2);
    expect(result.cables).toBe(1);
  });
});
