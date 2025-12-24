import { defineConfig } from 'vite';

export default defineConfig({
  server: {
    port: 3000,
    // Required for SharedArrayBuffer (needed for AudioWorklet with WASM)
    headers: {
      'Cross-Origin-Opener-Policy': 'same-origin',
      'Cross-Origin-Embedder-Policy': 'require-corp',
    },
  },
  build: {
    outDir: 'dist',
  },
  optimizeDeps: {
    exclude: ['@quiver/wasm'],
  },
  // Preview server also needs CORS headers
  preview: {
    headers: {
      'Cross-Origin-Opener-Policy': 'same-origin',
      'Cross-Origin-Embedder-Policy': 'require-corp',
    },
  },
});
