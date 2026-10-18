/**
 * Runs before every test file. Fails the run loudly when the intended time zone did not take
 * effect, so a zone that was silently ignored (for example by Windows) can never report a pass.
 */
import { TEST_TZ, nodeEnv } from './zone';

if (TEST_TZ === '') {
  throw new Error(
    'TEST_TZ is not set. Run the tests with `npm test`: it runs the suite once per time zone.'
  );
}

// Node reads TZ at start-up on Linux and macOS. Re-applying it before any date is created makes
// Node (ICU) pick it up on platforms that ignored the start-up value.
nodeEnv.TZ = TEST_TZ;

const actual = Intl.DateTimeFormat().resolvedOptions().timeZone;
if (actual !== TEST_TZ) {
  throw new Error(
    `Time zone ${TEST_TZ} was requested but this process uses ${actual}. ` +
      'The date tests would pass or fail for the wrong reason, so the run is stopped.'
  );
}
