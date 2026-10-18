import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import type { Item } from '@/types';
import { TEST_TZ } from '../test/zone';
import {
  checkWarrantyNotifications,
  getNotificationMessage,
  isWarrantyExpired,
} from './notifications';

/** Pretend it is this local date and time. */
function setNow(year: number, month: number, day: number, hour = 12, minute = 0, second = 0): void {
  vi.setSystemTime(new Date(year, month - 1, day, hour, minute, second));
}

function item(id: number, warrantyExpiry: string | undefined): Item {
  return { id, inventory_id: 1, name: `Item ${String(id)}`, warranty_expiry: warrantyExpiry };
}

/** `YYYY-MM-DD` for a calendar date, using UTC arithmetic so the machine's zone cannot matter. */
function isoDate(year: number, month: number, day: number): string {
  const d = new Date(Date.UTC(year, month - 1, day));
  return d.toISOString().slice(0, 10);
}

describe(`warranty notifications in ${TEST_TZ}`, () => {
  beforeEach(() => {
    vi.useFakeTimers();
  });
  afterEach(() => {
    vi.useRealTimers();
  });

  it('counts a warranty that ends tomorrow as 1 day away, even late in the evening', () => {
    for (const [h, m] of [
      [0, 1],
      [12, 0],
      [20, 0],
      [23, 59],
    ] as const) {
      setNow(2026, 10, 16, h, m);
      const [n] = checkWarrantyNotifications([item(1, '2026-10-17')]);
      expect(n?.daysUntilExpiry, `${String(h)}:${String(m)}`).toBe(1);
      expect(n?.status).toBe('expiring-soon');
    }
  });

  it('treats a warranty that ends today as still valid', () => {
    for (const [h, m] of [
      [0, 1],
      [12, 0],
      [23, 59],
    ] as const) {
      setNow(2026, 10, 17, h, m);
      const [n] = checkWarrantyNotifications([item(1, '2026-10-17')]);
      expect(n?.daysUntilExpiry, `${String(h)}:${String(m)}`).toBe(0);
      expect(n?.status).toBe('expiring-soon');
    }
  });

  it('treats a warranty that ended yesterday as expired by 1 day', () => {
    for (const [h, m] of [
      [0, 1],
      [12, 0],
      [23, 59],
    ] as const) {
      setNow(2026, 10, 18, h, m);
      const [n] = checkWarrantyNotifications([item(1, '2026-10-17')]);
      expect(n?.daysUntilExpiry, `${String(h)}:${String(m)}`).toBe(-1);
      expect(n?.status).toBe('expired');
      expect(n && getNotificationMessage(n)).toBe('Item 1 warranty expired 1 day ago');
    }
  });

  it('keeps the 7-day and 30-day thresholds on the calendar day', () => {
    setNow(2026, 10, 16, 21, 0);
    const result = checkWarrantyNotifications([
      item(1, '2026-10-23'), // 7 days: still "soon"
      item(2, '2026-10-24'), // 8 days: this month
      item(3, '2026-11-15'), // 30 days: included
      item(4, '2026-11-16'), // 31 days: left out
      item(5, '2026-09-16'), // expired 30 days ago: included
      item(6, '2026-09-15'), // expired 31 days ago: left out
    ]);
    const byId = new Map(result.map((n) => [n.id, n]));
    expect(byId.get(1)?.status).toBe('expiring-soon');
    expect(byId.get(2)?.status).toBe('expiring-this-month');
    expect(byId.get(3)?.daysUntilExpiry).toBe(30);
    expect(byId.has(4)).toBe(false);
    expect(byId.get(5)?.daysUntilExpiry).toBe(-30);
    expect(byId.has(6)).toBe(false);
  });

  it('orders notifications the same way in every zone', () => {
    setNow(2026, 10, 16, 21, 0);
    const result = checkWarrantyNotifications([
      item(1, '2026-11-10'),
      item(2, '2026-10-20'),
      item(3, '2026-10-10'),
      item(4, '2026-10-17'),
      item(5, '2026-10-01'),
      item(6, '2026-11-01'),
      item(7, undefined),
    ]);
    // Expired first (most recently expired first is by daysUntilExpiry ascending), then soonest
    expect(result.map((n) => n.id)).toEqual([5, 3, 4, 2, 6, 1]);
  });

  it('counts exactly 7 days across a DST change', () => {
    // Whatever zone this is, a week out is 7 days, also when the week contains a 23h or 25h day
    for (const [month, day] of [
      [3, 8],
      [3, 29],
      [4, 5],
      [9, 27],
      [10, 4],
      [10, 25],
      [11, 1],
    ] as const) {
      for (const hour of [0, 12, 23]) {
        setNow(2026, month, day, hour, 30);
        const [n] = checkWarrantyNotifications([item(1, isoDate(2026, month, day + 7))]);
        expect(n?.daysUntilExpiry, `${isoDate(2026, month, day)} ${String(hour)}:30`).toBe(7);
      }
    }
  });

  it('still honours dismissals', () => {
    setNow(2026, 10, 16, 21, 0);
    const items = [item(1, '2026-10-20')];
    const dismissed = {
      '1': { dismissedAt: '2026-10-15T10:00:00Z', warrantyExpiry: '2026-10-20' },
    };
    expect(checkWarrantyNotifications(items, dismissed)).toHaveLength(0);
    // The warranty date was edited: show it again
    const edited = { '1': { dismissedAt: '2026-10-15T10:00:00Z', warrantyExpiry: '2026-10-19' } };
    expect(checkWarrantyNotifications(items, edited)).toHaveLength(1);
  });
});

describe(`warranty expiry state in ${TEST_TZ}`, () => {
  beforeEach(() => {
    vi.useFakeTimers();
  });
  afterEach(() => {
    vi.useRealTimers();
  });

  it('is valid through the expiry date and expired from the next day', () => {
    for (const [h, m, s] of [
      [0, 0, 1],
      [12, 0, 0],
      [23, 59, 59],
    ] as const) {
      setNow(2026, 10, 17, h, m, s);
      expect(isWarrantyExpired('2026-10-18'), 'tomorrow').toBe(false);
      expect(isWarrantyExpired('2026-10-17'), 'today').toBe(false);
      expect(isWarrantyExpired('2026-10-16'), 'yesterday').toBe(true);
    }
  });

  it('does not flip to expired in the evening west of UTC', () => {
    // 20:00 local on the expiry day is already the next day in UTC
    setNow(2026, 10, 17, 20, 0);
    expect(isWarrantyExpired('2026-10-17')).toBe(false);
  });

  it('is not expired for an invalid date', () => {
    setNow(2026, 10, 17);
    expect(isWarrantyExpired('garbage')).toBe(false);
  });
});
