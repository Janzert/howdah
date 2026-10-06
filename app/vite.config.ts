import { defineConfig } from 'vitest/config';
import { svelte } from '@sveltejs/vite-plugin-svelte';

// Tauri expects a fixed dev port (1420) and doesn't want the terminal
// cleared. Browser-only runs (the preview pane, Playwright) use `--port 1430`
// so they don't block `tauri dev`.
export default defineConfig({
  plugins: [svelte()],
  clearScreen: false,
  server: {
    port: 1420,
    strictPort: true,
    watch: { ignored: ['**/src-tauri/**'] },
    // The dev bridge (npm run bridge), for running outside Tauri.
    // BRIDGE_PORT points at a second bridge (`npm run bridge -- --port N`).
    proxy: {
      '/bridge': {
        target: `http://127.0.0.1:${process.env.BRIDGE_PORT ?? 1421}`,
        rewrite: (path) => path.replace(/^\/bridge/, ''),
      },
    },
  },
  build: { target: 'es2022' },
  test: {
    environment: 'node',
    include: ['src/**/*.test.ts'],
  },
});
