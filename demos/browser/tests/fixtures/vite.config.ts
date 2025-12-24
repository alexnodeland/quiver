import { defineConfig } from 'vite';
import { resolve } from 'path';

export default defineConfig({
  server: {
    port: 5173,
    headers: {
      'Cross-Origin-Opener-Policy': 'same-origin',
      'Cross-Origin-Embedder-Policy': 'require-corp',
    },
  },
  resolve: {
    alias: {
      '@quiver/wasm': resolve(__dirname, '../../../../packages/@quiver/wasm'),
    },
  },
  optimizeDeps: {
    exclude: ['@quiver/wasm'],
  },
  assetsInclude: ['**/*.wasm'],
});
