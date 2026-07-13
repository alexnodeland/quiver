import { defineConfig } from 'tsup';

// Two builds with opposite handling of the wasm-bindgen glue ('../quiver'):
//
//  1. Library entries (index, audio): the glue is kept EXTERNAL. The dynamic
//     `import('../quiver')` and the value re-exports resolve at runtime to the
//     package-root `quiver.js` (a sibling of `dist/`). Bundling the glue would
//     break its `fetch`/`import.meta.url`-based init.
//
//  2. Worklet entry: the glue is BUNDLED (relative imports bundle by default) so
//     `dist/worklet.js` is a single importless module script usable from
//     `audioWorklet.addModule(url)`. The worklet loads the wasm via `initSync`
//     from bytes posted by the main thread, so the glue's fetch path is never hit.
export default defineConfig([
  {
    entry: ['src/index.ts', 'src/audio.ts'],
    format: ['esm'],
    dts: true,
    outDir: 'dist',
    target: 'es2020',
    platform: 'browser',
    clean: true,
    sourcemap: true,
    external: ['../quiver'],
  },
  {
    entry: ['src/worklet.ts'],
    format: ['esm'],
    dts: false,
    outDir: 'dist',
    target: 'es2020',
    platform: 'browser',
    clean: false,
    sourcemap: true,
  },
]);
