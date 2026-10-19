import path from 'node:path';
import { defineConfig } from 'vitest/config';

// Pure utility tests and server-rendered markup: no DOM, so the node environment is enough. Run
// through `npm test`, which starts one Vitest process per time zone (see scripts/test-zones.mjs).
export default defineConfig({
  resolve: {
    alias: {
      '@': path.resolve(__dirname, './src'),
    },
  },
  test: {
    environment: 'node',
    include: ['src/**/*.test.{ts,tsx}'],
    setupFiles: ['src/test/setup.ts'],
  },
});
