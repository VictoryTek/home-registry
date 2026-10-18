// Runs the Vitest suite once per time zone, each in its own process with TZ set at launch.
//
// Date handling bugs depend on the machine's zone (dates showed one day early west of UTC), so
// the suite has to run under several. Changing process.env.TZ in the middle of a run is not
// reliable, hence one process per zone. src/test/setup.ts checks that the zone really took
// effect and fails the run otherwise.
//
//   npm test                                  every zone
//   npm test -- --zone America/Los_Angeles    one zone (repeatable)
//   npm test -- --zone UTC src/utils          other arguments go to `vitest run`

import { spawnSync } from 'node:child_process';
import { fileURLToPath } from 'node:url';

// Zones chosen for their offsets and DST rules:
// west of UTC with DST (the bug), UTC, east with DST, a half-hour offset with southern DST,
// southern DST (Auckland), UTC-11 and UTC+14 (the extremes), the latter two without DST.
export const ZONES = [
  'America/Los_Angeles',
  'America/New_York',
  'UTC',
  'Europe/Berlin',
  'Australia/Adelaide',
  'Pacific/Auckland',
  'Pacific/Pago_Pago',
  'Pacific/Kiritimati',
];

const requested = [];
const passthrough = [];
const args = process.argv.slice(2);
for (let i = 0; i < args.length; i += 1) {
  if (args[i] === '--zone') {
    const zone = args[i + 1];
    if (!zone) {
      console.error('--zone needs a value');
      process.exit(2);
    }
    requested.push(zone);
    i += 1;
  } else {
    passthrough.push(args[i]);
  }
}

const zones = requested.length > 0 ? requested : ZONES;
const vitest = fileURLToPath(new URL('../node_modules/vitest/vitest.mjs', import.meta.url));

const failed = [];
for (const zone of zones) {
  console.log(`\n=== Time zone: ${zone} ===`);
  const result = spawnSync(process.execPath, [vitest, 'run', ...passthrough], {
    stdio: 'inherit',
    env: { ...process.env, TZ: zone, TEST_TZ: zone },
  });
  if (result.status !== 0) {
    failed.push(zone);
  }
}

console.log(`\nTime zones run: ${zones.length}, failed: ${failed.length}`);
if (failed.length > 0) {
  console.error(`FAILED in: ${failed.join(', ')}`);
  process.exit(1);
}
