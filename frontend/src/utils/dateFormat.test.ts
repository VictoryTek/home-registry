import { describe, expect, it } from 'vitest';
import { TEST_TZ } from '../test/zone';
import {
  daysUntilDate,
  formatDateOnly,
  formatTimestamp,
  todayAsDateOnly,
  type DateFormatType,
} from './dateFormat';

/** The day of each zone's DST changes in 2026: [clocks go forward, clocks go back] as [month, day]. */
const DST_CHANGES: Record<string, [[number, number], [number, number]] | undefined> = {
  'America/Los_Angeles': [
    [3, 8],
    [11, 1],
  ],
  'America/New_York': [
    [3, 8],
    [11, 1],
  ],
  'Europe/Berlin': [
    [3, 29],
    [10, 25],
  ],
  // Southern hemisphere: DST starts in the spring (Sep/Oct) and ends in April
  'Australia/Adelaide': [
    [10, 4],
    [4, 5],
  ],
  'Pacific/Auckland': [
    [9, 27],
    [4, 5],
  ],
};

/** `YYYY-MM-DD` for a calendar date, using UTC arithmetic so the machine's zone cannot matter. */
function isoDate(year: number, month: number, day: number): string {
  const d = new Date(Date.UTC(year, month - 1, day));
  const mm = String(d.getUTCMonth() + 1).padStart(2, '0');
  const dd = String(d.getUTCDate()).padStart(2, '0');
  return `${String(d.getUTCFullYear())}-${mm}-${dd}`;
}

describe(`date-only values (purchase_date, warranty_expiry) in ${TEST_TZ}`, () => {
  const formats: [DateFormatType, string][] = [
    ['MM/DD/YYYY', '10/17/2026'],
    ['DD/MM/YYYY', '17/10/2026'],
    ['YYYY-MM-DD', '2026-10-17'],
    ['DD.MM.YYYY', '17.10.2026'],
  ];

  it.each(formats)('formats 2026-10-17 as %s', (format, expected) => {
    expect(formatDateOnly('2026-10-17', format)).toBe(expected);
  });

  it('shows the calendar day that was stored, whatever the time zone', () => {
    const dates = [
      '2026-10-17',
      '2026-01-01',
      '2026-12-31',
      '2024-02-29',
      '2026-03-08',
      '2026-11-01',
      '2026-04-05',
      '2026-09-27',
      '2026-10-04',
    ];
    for (const date of dates) {
      expect(formatDateOnly(date, 'YYYY-MM-DD'), date).toBe(date);
    }
  });

  it('formats nothing for a missing or invalid value', () => {
    const invalid = [
      undefined,
      null,
      '',
      'garbage',
      '2026-02-31',
      '2026-13-01',
      '2026-00-10',
      '2026-10-00',
      '2025-02-29',
      ' 2026-10-17',
      '2026-10-17 ',
      '2026-10-17T00:00:00Z',
      '2026-10-17T00:00:00',
      '10/17/2026',
    ];
    for (const value of invalid) {
      expect(formatDateOnly(value, 'YYYY-MM-DD'), String(value)).toBe('');
    }
  });

  it('accepts a leap day only in a leap year', () => {
    expect(formatDateOnly('2028-02-29', 'YYYY-MM-DD')).toBe('2028-02-29');
    expect(formatDateOnly('2100-02-29', 'YYYY-MM-DD')).toBe('');
  });
});

describe(`timestamps (created_at, updated_at) in ${TEST_TZ}`, () => {
  // 2026-10-17T03:30:00Z as the local calendar date in each test zone
  const EXPECTED: Record<string, string> = {
    'America/Los_Angeles': '2026-10-16',
    'America/New_York': '2026-10-16',
    UTC: '2026-10-17',
    'Europe/Berlin': '2026-10-17',
    'Australia/Adelaide': '2026-10-17',
    'Pacific/Auckland': '2026-10-17',
    'Pacific/Pago_Pago': '2026-10-16',
    'Pacific/Kiritimati': '2026-10-17',
  };

  it('shows the local calendar date of the instant', () => {
    const expected = EXPECTED[TEST_TZ];
    expect(expected, `no expectation for ${TEST_TZ}`).toBeDefined();
    expect(formatTimestamp('2026-10-17T03:30:00Z', 'YYYY-MM-DD')).toBe(expected);
  });

  it('shows the same day for an instant built from local components', () => {
    const lateEvening = new Date(2026, 9, 17, 23, 30).toISOString();
    const justAfterMidnight = new Date(2026, 9, 17, 0, 30).toISOString();
    expect(formatTimestamp(lateEvening, 'YYYY-MM-DD')).toBe('2026-10-17');
    expect(formatTimestamp(justAfterMidnight, 'YYYY-MM-DD')).toBe('2026-10-17');
  });

  it('formats nothing for a missing or invalid value', () => {
    for (const value of [undefined, null, '', 'garbage']) {
      expect(formatTimestamp(value, 'YYYY-MM-DD'), String(value)).toBe('');
    }
  });
});

describe(`days until a date in ${TEST_TZ}`, () => {
  it('counts calendar days from the local date, at any time of day', () => {
    const times: [number, number, number][] = [
      [0, 0, 1],
      [6, 30, 0],
      [12, 0, 0],
      [20, 0, 0],
      [23, 59, 59],
    ];
    for (const [h, m, s] of times) {
      const now = new Date(2026, 9, 16, h, m, s);
      expect(daysUntilDate('2026-10-17', now), `${String(h)}:${String(m)}`).toBe(1);
      expect(daysUntilDate('2026-10-16', now), `${String(h)}:${String(m)}`).toBe(0);
      expect(daysUntilDate('2026-10-15', now), `${String(h)}:${String(m)}`).toBe(-1);
      expect(daysUntilDate('2026-11-15', now), `${String(h)}:${String(m)}`).toBe(30);
    }
  });

  it('counts across month, year and leap-day boundaries', () => {
    expect(daysUntilDate('2027-01-01', new Date(2026, 11, 31, 23, 59))).toBe(1);
    expect(daysUntilDate('2028-03-01', new Date(2028, 1, 28, 12, 0))).toBe(2);
    expect(daysUntilDate('2027-03-01', new Date(2027, 1, 28, 12, 0))).toBe(1);
  });

  it('is null for an invalid date', () => {
    const now = new Date(2026, 9, 16, 12, 0);
    for (const value of ['', 'garbage', '2026-02-31', '2026-10-17T00:00:00Z']) {
      expect(daysUntilDate(value, now), value).toBeNull();
    }
  });

  const changes = DST_CHANGES[TEST_TZ];

  it.runIf(changes !== undefined)('the test dates really are DST changes in this zone', () => {
    for (const [month, day] of changes ?? []) {
      const length =
        new Date(2026, month - 1, day + 1).getTime() - new Date(2026, month - 1, day).getTime();
      expect(length, `${String(month)}/${String(day)}`).not.toBe(24 * 60 * 60 * 1000);
    }
  });

  it.runIf(changes !== undefined)('counts exactly 7 days across a DST change', () => {
    for (const [month, day] of changes ?? []) {
      // One week out, starting the day before, on the day itself (early and late), and the day after
      const starts: [number, number, number, number][] = [
        [month, day - 1, 12, 0],
        [month, day, 0, 30],
        [month, day, 12, 0],
        [month, day, 23, 30],
        [month, day + 1, 0, 30],
      ];
      for (const [m, d, h, min] of starts) {
        const now = new Date(2026, m - 1, d, h, min);
        const target = isoDate(2026, m, d + 7);
        expect(
          daysUntilDate(target, now),
          `from ${isoDate(2026, m, d)} ${String(h)}:${String(min)} to ${target}`
        ).toBe(7);
      }
    }
  });
});

describe(`today's date in ${TEST_TZ}`, () => {
  it('is the local calendar date, not the UTC date', () => {
    expect(todayAsDateOnly(new Date(2026, 9, 16, 20, 0))).toBe('2026-10-16');
    expect(todayAsDateOnly(new Date(2026, 9, 16, 0, 0, 30))).toBe('2026-10-16');
    expect(todayAsDateOnly(new Date(2026, 9, 16, 23, 59, 59))).toBe('2026-10-16');
    expect(todayAsDateOnly(new Date(2026, 11, 31, 23, 59))).toBe('2026-12-31');
    expect(todayAsDateOnly(new Date(2027, 0, 1, 0, 1))).toBe('2027-01-01');
  });
});
