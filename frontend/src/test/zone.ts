/**
 * Access to the Node environment from tests. The project does not depend on @types/node, so the
 * one piece of `process` the tests need is typed here.
 */
interface NodeProcess {
  env: Record<string, string | undefined>;
}

export const nodeEnv = (globalThis as unknown as { process: NodeProcess }).process.env;

/** The time zone this run is meant to use, set by scripts/test-zones.mjs. */
export const TEST_TZ = nodeEnv.TEST_TZ ?? '';
