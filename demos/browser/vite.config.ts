import { defineConfig } from 'vite';
import { resolve } from 'path';

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
    rollupOptions: {
      input: {
        main: resolve(__dirname, 'index.html'),
      },
    },
  },
  resolve: {
    alias: {
      '@quiver/wasm': resolve(__dirname, '../../packages/@quiver/wasm'),
    },
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
  // Handle WASM files properly
  assetsInclude: ['**/*.wasm'],
});
