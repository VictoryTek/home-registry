import { describe, expect, it } from 'vitest';
import type { OrganizerTypeWithOptions } from '@/types';
import { defaultGroupBy, encodeOrganizerFilters, roundShares } from './reportQuery';

function organizer(
  id: number | undefined,
  input_type: 'select' | 'text' | 'image',
  display_order: number
): OrganizerTypeWithOptions {
  return {
    id,
    inventory_id: 1,
    name: `Organizer ${String(id)}`,
    input_type,
    is_required: false,
    display_order,
    options: [],
  };
}

describe('organizer filter parameter', () => {
  it('is left out when nothing is selected', () => {
    expect(encodeOrganizerFilters({})).toBeUndefined();
    expect(encodeOrganizerFilters({ 3: undefined })).toBeUndefined();
  });

  it('writes type:option pairs in organizer order', () => {
    expect(encodeOrganizerFilters({ 4: 9, 3: 12 })).toBe('3:12,4:9');
    expect(encodeOrganizerFilters({ 10: 'none', 2: 7 })).toBe('2:7,10:none');
  });

  it('skips organizers without a choice', () => {
    expect(encodeOrganizerFilters({ 3: 12, 4: undefined, 5: 'none' })).toBe('3:12,5:none');
  });

  it('writes only what the server accepts', () => {
    for (const encoded of [
      encodeOrganizerFilters({ 3: 12 }),
      encodeOrganizerFilters({ 3: 'none', 99: 1 }),
    ]) {
      expect(encoded).toMatch(/^\d+:(\d+|none)(,\d+:(\d+|none))*$/);
    }
  });
});

describe('default grouping', () => {
  it('is the first select organizer by display order, then id', () => {
    expect(
      defaultGroupBy([
        organizer(5, 'select', 2),
        organizer(3, 'select', 2),
        organizer(9, 'select', 1),
      ])
    ).toBe(9);
    expect(defaultGroupBy([organizer(5, 'select', 2), organizer(3, 'select', 2)])).toBe(3);
  });

  it('ignores text and image organizers', () => {
    expect(
      defaultGroupBy([organizer(1, 'text', 0), organizer(2, 'image', 0), organizer(3, 'select', 9)])
    ).toBe(3);
  });

  it('is undefined when there is no select organizer', () => {
    expect(defaultGroupBy([])).toBeUndefined();
    expect(defaultGroupBy([organizer(1, 'text', 0)])).toBeUndefined();
    expect(defaultGroupBy([organizer(undefined, 'select', 0)])).toBeUndefined();
  });

  it('does not depend on the order the organizers arrive in', () => {
    const list = [organizer(7, 'select', 3), organizer(2, 'select', 1), organizer(4, 'select', 2)];
    expect(defaultGroupBy(list)).toBe(defaultGroupBy([...list].reverse()));
  });
});

describe('rounding shares', () => {
  const sum = (values: number[]) => Math.round(values.reduce((a, b) => a + b, 0) * 10) / 10;

  it('adds up to exactly 100.0 where plain rounding would not', () => {
    // Thirds: 33.3 + 33.3 + 33.3 would be 99.9
    const thirds = roundShares([100 / 3, 100 / 3, 100 / 3]);
    expect(sum(thirds)).toBe(100);
    expect([...thirds].sort()).toEqual([33.3, 33.3, 33.4]);

    // The example from the tests of the API: 250 / 100 / 40 / 0 of 390
    const real = roundShares([(250 / 390) * 100, (100 / 390) * 100, (40 / 390) * 100, 0]);
    expect(sum(real)).toBe(100);
    expect(real[3]).toBe(0);
  });

  it('keeps the order and the closest value for each share', () => {
    const rounded = roundShares([64.1025641, 25.6410256, 10.2564103, 0]);
    expect(rounded).toEqual([64.1, 25.6, 10.3, 0]);
  });

  it('gives the extra unit to the largest remainders, ties to the earlier share', () => {
    expect(roundShares([50.05, 49.95])).toEqual([50.1, 49.9]);
    expect(roundShares([33.35, 33.35, 33.3])).toEqual([33.4, 33.3, 33.3]);
  });

  it('stays zero for nothing or all zeros', () => {
    expect(roundShares([])).toEqual([]);
    expect(roundShares([0, 0, 0])).toEqual([0, 0, 0]);
  });

  it('supports other precisions and never exceeds the total', () => {
    expect(roundShares([100 / 3, 100 / 3, 100 / 3], 0)).toEqual([34, 33, 33]);
    for (let n = 1; n <= 12; n += 1) {
      const equal = Array.from({ length: n }, () => 100 / n);
      expect(sum(roundShares(equal))).toBe(100);
    }
  });
});
