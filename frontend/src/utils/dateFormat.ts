/**
 * Date formatting utility
 * Formats dates according to user preferences
 *
 * The API sends two different kinds of date and they must not be mixed up:
 *
 * - Calendar dates (`purchase_date`, `warranty_expiry`): `YYYY-MM-DD` text, a day with no time
 *   and no time zone. Use `formatDateOnly`. `new Date('2026-10-17')` reads such a string as UTC
 *   midnight, which is still the 16th west of UTC, so these are never turned into a `Date`.
 * - Timestamps (`created_at`, `updated_at`): an instant in time. Use `formatTimestamp`, which
 *   shows the calendar date of that instant in the user's own time zone.
 */

export type DateFormatType = 'MM/DD/YYYY' | 'DD/MM/YYYY' | 'YYYY-MM-DD' | 'DD.MM.YYYY';

interface DateParts {
  year: number;
  month: number; // 1-12
  day: number; // 1-31
}

const DATE_ONLY = /^(\d{4})-(\d{2})-(\d{2})$/;

const MS_PER_DAY = 24 * 60 * 60 * 1000;

/**
 * Parse a `YYYY-MM-DD` calendar date. Returns null for anything else, including a date that does
 * not exist (2026-02-31) and a string that carries a time.
 */
export function parseDateOnly(value: string): DateParts | null {
  const match = DATE_ONLY.exec(value);
  if (!match) {
    return null;
  }
  const [, y = '', m = '', d = ''] = match;
  const year = Number(y);
  const month = Number(m);
  const day = Number(d);

  // Date rolls an impossible date over to the next month, so read it back to reject it. This is
  // done in UTC, so the machine's time zone cannot change the answer.
  const check = new Date(0);
  check.setUTCFullYear(year, month - 1, day);
  if (
    check.getUTCFullYear() !== year ||
    check.getUTCMonth() !== month - 1 ||
    check.getUTCDate() !== day
  ) {
    return null;
  }
  return { year, month, day };
}

function formatParts({ year, month, day }: DateParts, format: DateFormatType): string {
  const dd = day.toString().padStart(2, '0');
  const mm = month.toString().padStart(2, '0');
  const yyyy = year.toString().padStart(4, '0');

  switch (format) {
    case 'MM/DD/YYYY':
      return `${mm}/${dd}/${yyyy}`;
    case 'DD/MM/YYYY':
      return `${dd}/${mm}/${yyyy}`;
    case 'YYYY-MM-DD':
      return `${yyyy}-${mm}-${dd}`;
    case 'DD.MM.YYYY':
      return `${dd}.${mm}.${yyyy}`;
    default:
      return `${mm}/${dd}/${yyyy}`;
  }
}

/**
 * Format a calendar date such as `purchase_date` or `warranty_expiry`.
 * @param value - `YYYY-MM-DD` text from the API
 * @param format - Desired date format
 * @returns Formatted date, or an empty string when the value is missing or not a valid date
 */
export function formatDateOnly(
  value: string | undefined | null,
  format: DateFormatType = 'MM/DD/YYYY'
): string {
  if (!value) {
    return '';
  }
  const parts = parseDateOnly(value);
  return parts ? formatParts(parts, format) : '';
}

/**
 * Format a timestamp such as `created_at` or `updated_at` as the calendar date it falls on in
 * the user's time zone.
 * @param value - ISO 8601 timestamp from the API
 * @param format - Desired date format
 * @returns Formatted date, or an empty string when the value is missing or not a valid timestamp
 */
export function formatTimestamp(
  value: string | undefined | null,
  format: DateFormatType = 'MM/DD/YYYY'
): string {
  if (!value) {
    return '';
  }
  const date = new Date(value);
  if (isNaN(date.getTime())) {
    return '';
  }
  return formatParts(
    { year: date.getFullYear(), month: date.getMonth() + 1, day: date.getDate() },
    format
  );
}

/**
 * Whole calendar days from today (in the user's time zone) to a calendar date: 0 for today, 1
 * for tomorrow, negative for the past. Counted on the calendar, not in 24-hour blocks, so a day
 * with a daylight saving change cannot throw it off by one.
 * @param value - `YYYY-MM-DD` text from the API
 * @param now - The current moment; only the tests pass this
 * @returns The number of days, or null when the value is not a valid date
 */
export function daysUntilDate(value: string, now: Date = new Date()): number | null {
  const target = parseDateOnly(value);
  if (!target) {
    return null;
  }
  const targetDay = Date.UTC(target.year, target.month - 1, target.day);
  const today = Date.UTC(now.getFullYear(), now.getMonth(), now.getDate());
  return Math.round((targetDay - today) / MS_PER_DAY);
}

/**
 * Today's date in the user's time zone as `YYYY-MM-DD`. `new Date().toISOString()` gives the UTC
 * date, which is tomorrow's date in the evening west of UTC.
 */
export function todayAsDateOnly(now: Date = new Date()): string {
  return formatParts(
    { year: now.getFullYear(), month: now.getMonth() + 1, day: now.getDate() },
    'YYYY-MM-DD'
  );
}
