import { defineConfig } from 'vite';

export default defineConfig({
  // Relative asset URLs so the built demo works from any mount point (it is
  // deployed to GitHub Pages under /quiver/playground/, not the site root).
  base: './',
  server: {
    port: 3000,
  },
  build: {
    outDir: 'dist',
  },
  optimizeDeps: {
    exclude: ['@quiver-dsp/wasm'],
  },
});
