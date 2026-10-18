import { defineConfig } from 'vitest/config';

// Pure utility tests: no DOM, so the node environment is enough. Run through `npm test`, which
// starts one Vitest process per time zone (see scripts/test-zones.mjs).
export default defineConfig({
  test: {
    environment: 'node',
    include: ['src/**/*.test.ts'],
    setupFiles: ['src/test/setup.ts'],
  },
});
