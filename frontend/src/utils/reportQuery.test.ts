import { describe, expect, it } from 'vitest';
import type { OrganizerTypeWithOptions } from '@/types';
import {
  breakdownNotice,
  defaultGroupBy,
  encodeOrganizerFilters,
  isStaleOrganizerError,
  itemTotalValue,
  keepGroupBy,
  roundShares,
  shouldRecoverFromStaleOrganizers,
} from './reportQuery';

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

describe('the total value of an item row', () => {
  it('is price times quantity, with a missing quantity counted as 1', () => {
    expect(itemTotalValue({ purchase_price: 10, quantity: 3 })).toBe(30);
    expect(itemTotalValue({ purchase_price: 10 })).toBe(10);
    expect(itemTotalValue({ purchase_price: 0, quantity: 5 })).toBe(0);
  });

  it('is nothing for an item without a price, as in the CSV, not 0', () => {
    expect(itemTotalValue({})).toBeNull();
    expect(itemTotalValue({ quantity: 4 })).toBeNull();
    expect(itemTotalValue({ purchase_price: undefined, quantity: 2 })).toBeNull();
  });
});

describe('what an empty breakdown says', () => {
  const base = {
    groupCount: 0,
    grouped: false,
    onlyUncategorized: true,
    selectOrganizerCount: 0,
    hasActiveFilters: false,
  };

  it('says no item matches when filters leave nothing, whatever organizers exist', () => {
    for (const selectOrganizerCount of [0, 2]) {
      for (const grouped of [true, false]) {
        expect(
          breakdownNotice({ ...base, selectOrganizerCount, grouped, hasActiveFilters: true })
        ).toBe('No items match the current filters.');
      }
    }
  });

  it('says the inventory is empty when there are no items and no filters', () => {
    expect(breakdownNotice(base)).toBe('This inventory has no items yet.');
  });

  it('asks for an organizer only when the legacy breakdown has nothing to show', () => {
    const legacy = { ...base, groupCount: 1 };
    expect(breakdownNotice(legacy)).toBe(
      'Add a select organizer (for example "Category") to this inventory to see a breakdown of its items.'
    );
    expect(breakdownNotice({ ...legacy, selectOrganizerCount: 2 })).toBe(
      'Choose an organizer to group by.'
    );
  });

  it('shows the table when there are groups worth showing', () => {
    expect(breakdownNotice({ ...base, groupCount: 3, onlyUncategorized: false })).toBeNull();
    expect(breakdownNotice({ ...base, groupCount: 1, grouped: true })).toBeNull();
  });
});

describe('a report asked for an organizer that no longer exists', () => {
  it('recognises the two fixed errors the API answers with', () => {
    expect(isStaleOrganizerError('Unknown organizer or option')).toBe(true);
    expect(isStaleOrganizerError('Not a select organizer')).toBe(true);
    for (const other of [
      undefined,
      '',
      'Invalid date',
      'Invalid organizer filter',
      'Access denied',
    ]) {
      expect(isStaleOrganizerError(other), String(other)).toBe(false);
    }
  });

  it('reloads the organizers once, never twice in a row', () => {
    expect(shouldRecoverFromStaleOrganizers('Unknown organizer or option', false)).toBe(true);
    // The reload did not cure it: show the error instead of looping
    expect(shouldRecoverFromStaleOrganizers('Unknown organizer or option', true)).toBe(false);
    expect(shouldRecoverFromStaleOrganizers('Invalid date', false)).toBe(false);
    expect(shouldRecoverFromStaleOrganizers(undefined, false)).toBe(false);
  });

  const select = (id: number, display_order = 0): OrganizerTypeWithOptions => ({
    ...organizer(id, 'select', display_order),
  });

  it('keeps the grouping while its organizer exists and picks the default when it is gone', () => {
    const list = [select(3, 2), select(5, 1), organizer(7, 'text', 0)];
    expect(keepGroupBy(3, list)).toBe(3);
    expect(keepGroupBy(9, list)).toBe(5);
    // A text organizer cannot group
    expect(keepGroupBy(7, list)).toBe(5);
    expect(keepGroupBy(undefined, list)).toBe(5);
    expect(keepGroupBy(3, [])).toBeUndefined();
  });
});
