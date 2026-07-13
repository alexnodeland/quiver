import { defineConfig } from 'tsup';

export default defineConfig({
  entry: ['src/index.ts'],
  format: ['cjs', 'esm'],
  dts: true,
  clean: true,
  sourcemap: true,
  target: 'es2020',
  // These hooks are client-only (they use React state/effects and dynamically
  // import the WASM engine). Emit the directive at the very top of every bundle so
  // Next.js / RSC treats the module as a client boundary (Q178).
  banner: {
    js: "'use client';",
  },
  // Peer/host packages stay external; @quiver-dsp/* are resolved by the consumer.
  external: ['react', '@xyflow/react', '@quiver-dsp/types', '@quiver-dsp/wasm'],
});
