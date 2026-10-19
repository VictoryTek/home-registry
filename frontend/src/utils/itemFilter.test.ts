import { describe, expect, it } from 'vitest';
import type { InventoryOrganizerValue, Item, OrganizerTypeWithOptions } from '@/types';
import fixture from '../test/fixtures/item_search_cases.json';
import {
  DEFAULT_FILTER,
  applyFilter,
  cardChips,
  filterItems,
  indexOrganizerValues,
  isFiltered,
  itemMatchesTerm,
  normalizeTerm,
  parseFilterParams,
  pruneSelection,
  sortItems,
  writeFilterParams,
  type ItemFilterState,
  type SortKey,
} from './itemFilter';

function item(id: number, fields: Partial<Item> = {}): Item {
  return { id, inventory_id: 1, name: `Item ${String(id)}`, ...fields };
}

function ids(items: Item[]): number[] {
  return items.map((i) => i.id ?? 0);
}

function value(
  item_id: number,
  organizer_type_id: number,
  organizer_option_id: number | null,
  text: string
): InventoryOrganizerValue {
  return { item_id, organizer_type_id, organizer_option_id, value: text };
}

function state(changes: Partial<ItemFilterState> = {}): ItemFilterState {
  return { ...DEFAULT_FILTER, ...changes };
}

describe('the cases shared with the search API', () => {
  // The same fixture runs against the API in tests/test_item_search.rs. Image organizer values
  // are not in the batch call the page filters with, so they are left out here too.
  const organizers = fixture.organizers;
  const typeId = new Map(organizers.map((o, index) => [o.name, index + 1]));
  const kindOf = new Map(organizers.map((o) => [o.name, o.input_type]));
  const items: Item[] = [];
  const rows: InventoryOrganizerValue[] = [];
  const keyOf = new Map<number, string>();
  fixture.items.forEach((entry, index) => {
    const id = index + 1;
    keyOf.set(id, entry.key);
    items.push({
      id,
      inventory_id: 1,
      name: entry.name,
      ...(entry.description === null ? {} : { description: entry.description }),
      ...(entry.notes === null ? {} : { notes: entry.notes }),
      ...(entry.category === null ? {} : { category: entry.category }),
      ...(entry.location === null ? {} : { location: entry.location }),
    });
    for (const [organizer, text] of Object.entries(entry.values) as [string, string][]) {
      if (kindOf.get(organizer) === 'image') {
        continue;
      }
      rows.push(
        value(id, typeId.get(organizer) ?? 0, kindOf.get(organizer) === 'select' ? id : null, text)
      );
    }
  });
  const byItem = indexOrganizerValues(rows);

  it.each(fixture.cases.map((c) => [c.label, c] as const))('%s', (_label, c) => {
    const found = filterItems(items, byItem, state({ q: c.term })).map((i) => keyOf.get(i.id ?? 0));
    expect(new Set(found)).toEqual(new Set(c.expect));
  });
});

describe('the search term', () => {
  it('is trimmed and control characters become spaces', () => {
    expect(normalizeTerm('  drill  ')).toBe('drill');
    expect(normalizeTerm('a\tb\nc\u0000d')).toBe('a b c d');
    expect(normalizeTerm(' x ')).toBe('x');
    expect(normalizeTerm('   ')).toBe('');
  });

  it('matches without regard to case in the item and in organizer values', () => {
    const i = item(1, { name: 'Cordless DRILL' });
    expect(itemMatchesTerm(i, [], 'drill')).toBe(true);
    expect(itemMatchesTerm(i, [], 'DRILL')).toBe(true);
    expect(itemMatchesTerm(i, [value(1, 1, 5, 'Bosch')], 'bOsCh')).toBe(true);
    expect(itemMatchesTerm(i, [], 'bosch')).toBe(false);
  });

  it('treats an empty term as matching everything', () => {
    expect(itemMatchesTerm(item(1), [], '')).toBe(true);
    expect(filterItems([item(1), item(2)], new Map(), state({ q: '   ' }))).toHaveLength(2);
  });

  it('does not read % _ or \\ as pattern characters', () => {
    const items = [item(1, { name: 'a%b' }), item(2, { name: 'axb' }), item(3, { name: 'a_b' })];
    expect(ids(filterItems(items, new Map(), state({ q: '%' })))).toEqual([1]);
    expect(ids(filterItems(items, new Map(), state({ q: '_' })))).toEqual([3]);
    expect(ids(filterItems(items, new Map(), state({ q: 'a_b' })))).toEqual([3]);
    expect(ids(filterItems(items, new Map(), state({ q: '\\' })))).toEqual([]);
  });
});

describe('organizer choices', () => {
  const items = [item(1), item(2), item(3), item(4)];
  const byItem = indexOrganizerValues([
    value(1, 10, 100, 'Garage'),
    value(2, 10, 101, 'Kitchen'),
    value(2, 20, 200, 'Red'),
    value(3, 20, 200, 'Red'),
    // item 4 has no values at all
  ]);

  it('keeps the items with that option', () => {
    expect(ids(filterItems(items, byItem, state({ organizers: { 10: 100 } })))).toEqual([1]);
    expect(ids(filterItems(items, byItem, state({ organizers: { 20: 200 } })))).toEqual([2, 3]);
  });

  it('"none" keeps the items without a value for that organizer', () => {
    expect(ids(filterItems(items, byItem, state({ organizers: { 10: 'none' } })))).toEqual([3, 4]);
  });

  it('needs every chosen organizer to match', () => {
    expect(ids(filterItems(items, byItem, state({ organizers: { 10: 101, 20: 200 } })))).toEqual([
      2,
    ]);
    expect(ids(filterItems(items, byItem, state({ organizers: { 10: 'none', 20: 200 } })))).toEqual(
      [3]
    );
  });

  it('ignores an organizer with no choice', () => {
    expect(filterItems(items, byItem, state({ organizers: { 10: undefined } }))).toHaveLength(4);
  });

  it('combines with the search term', () => {
    expect(
      ids(filterItems(items, byItem, state({ q: 'kitchen', organizers: { 20: 200 } })))
    ).toEqual([2]);
    expect(
      ids(filterItems(items, byItem, state({ q: 'garage', organizers: { 20: 200 } })))
    ).toEqual([]);
  });
});

describe('sorting', () => {
  const sorted = (key: SortKey, dir: 'asc' | 'desc', items: Item[]) =>
    ids(sortItems(items, key, dir));

  it('puts items without a price last, whichever way it sorts (those in id order of the sort)', () => {
    const items = [
      item(1, { purchase_price: 30 }),
      item(2),
      item(3, { purchase_price: 10 }),
      item(4),
      item(5, { purchase_price: 20 }),
    ];
    expect(sorted('price', 'asc', items)).toEqual([3, 5, 1, 2, 4]);
    expect(sorted('price', 'desc', items)).toEqual([1, 5, 3, 4, 2]);
  });

  it('puts items without a date last, whichever way it sorts', () => {
    const items = [
      item(1, { purchase_date: '2024-03-10', warranty_expiry: '2026-01-01' }),
      item(2),
      item(3, { purchase_date: '2023-12-31', warranty_expiry: '' }),
      item(4, { purchase_date: '2024-03-09' }),
    ];
    expect(sorted('purchase_date', 'asc', items)).toEqual([3, 4, 1, 2]);
    expect(sorted('purchase_date', 'desc', items)).toEqual([1, 4, 3, 2]);
    expect(sorted('warranty_expiry', 'asc', items)).toEqual([1, 2, 3, 4]);
    expect(sorted('warranty_expiry', 'desc', items)).toEqual([1, 4, 3, 2]);
  });

  it('compares dates as text, so a time zone cannot change the order', () => {
    // Around a year, a month and a DST change, in every zone the suite runs in
    const dates = [
      '2024-03-10',
      '2024-03-09',
      '2023-12-31',
      '2024-01-01',
      '2024-11-03',
      '2024-03-31',
    ];
    const items = dates.map((d, index) => item(index + 1, { purchase_date: d }));
    const expected = [...dates].sort();
    expect(sortItems(items, 'purchase_date', 'asc').map((i) => i.purchase_date)).toEqual(expected);
    expect(sortItems(items, 'purchase_date', 'desc').map((i) => i.purchase_date)).toEqual(
      [...expected].reverse()
    );
  });

  it('sorts by name, ignoring case and reading numbers as numbers', () => {
    const items = [
      item(1, { name: 'shelf 10' }),
      item(2, { name: 'Shelf 2' }),
      item(3, { name: 'Alpha' }),
      item(4, { name: 'bravo' }),
    ];
    expect(sorted('name', 'asc', items)).toEqual([3, 4, 2, 1]);
    expect(sorted('name', 'desc', items)).toEqual([1, 2, 4, 3]);
  });

  it('sorts by when the item was added, including fractions of a second', () => {
    const items = [
      item(1, { created_at: '2025-01-01T10:00:00.5Z' }),
      item(2, { created_at: '2025-01-01T10:00:00Z' }),
      item(3, { created_at: '2025-01-01T10:00:00.123456Z' }),
      item(4),
    ];
    expect(sorted('created', 'asc', items)).toEqual([2, 3, 1, 4]);
    expect(sorted('created', 'desc', items)).toEqual([1, 3, 2, 4]);
  });

  it('breaks ties by id in the direction of the sort, every time', () => {
    const items = [item(3, { purchase_price: 5 }), item(1, { purchase_price: 5 }), item(2)];
    expect(sorted('price', 'asc', items)).toEqual([1, 3, 2]);
    expect(sorted('price', 'desc', items)).toEqual([3, 1, 2]);
    expect(sorted('price', 'asc', [...items].reverse())).toEqual([1, 3, 2]);
  });

  it('leaves the list it was given alone', () => {
    const items = [item(2, { purchase_price: 2 }), item(1, { purchase_price: 1 })];
    sortItems(items, 'price', 'asc');
    expect(ids(items)).toEqual([2, 1]);
  });

  it('filters and then sorts', () => {
    const items = [
      item(1, { name: 'b lamp', purchase_price: 5 }),
      item(2, { name: 'a lamp', purchase_price: 9 }),
      item(3, { name: 'chair', purchase_price: 1 }),
    ];
    expect(
      ids(applyFilter(items, new Map(), state({ q: 'lamp', sort: 'name', dir: 'asc' })))
    ).toEqual([2, 1]);
    expect(
      ids(applyFilter(items, new Map(), state({ q: 'lamp', sort: 'price', dir: 'desc' })))
    ).toEqual([2, 1]);
  });
});

describe('filter and sort in the page address', () => {
  const read = (query: string) => parseFilterParams(new URLSearchParams(query));
  const write = (s: ItemFilterState, query = '') =>
    writeFilterParams(s, new URLSearchParams(query)).toString();

  it('reads nothing as the defaults', () => {
    expect(read('')).toEqual(DEFAULT_FILTER);
    expect(isFiltered(read(''))).toBe(false);
  });

  it('writes nothing for the defaults', () => {
    expect(write(DEFAULT_FILTER)).toBe('');
    expect(write(DEFAULT_FILTER, 'q=old&sort=name&o=1:2')).toBe('');
  });

  it('round-trips every part', () => {
    const s = state({
      q: 'cord less',
      sort: 'price',
      dir: 'desc',
      organizers: { 3: 12, 10: 'none' },
    });
    expect(write(s)).toBe('q=cord+less&sort=price&dir=desc&o=3%3A12%2C10%3Anone');
    expect(read(write(s))).toEqual(s);
  });

  it('gives each sort its own natural direction and writes the direction only when it differs', () => {
    expect(read('sort=name').dir).toBe('asc');
    expect(read('sort=created').dir).toBe('desc');
    expect(read('sort=price&dir=desc').dir).toBe('desc');
    expect(write(state({ sort: 'name', dir: 'asc' }))).toBe('sort=name');
    expect(write(state({ sort: 'name', dir: 'desc' }))).toBe('sort=name&dir=desc');
    expect(write(state({ sort: 'created', dir: 'asc' }))).toBe('dir=asc');
  });

  it('ignores values it does not know', () => {
    expect(read('sort=bogus&dir=sideways&o=abc')).toEqual(DEFAULT_FILTER);
    expect(read('o=3:12,x:1,4:,:5,6:none,7:-1,8:1.5').organizers).toEqual({ 3: 12, 6: 'none' });
    expect(read('o=0:1,1:0').organizers).toEqual({});
  });

  it('keeps other parameters and does not repeat one of its own', () => {
    expect(write(state({ q: 'x' }), 'q=a&q=b&other=1')).toBe('other=1&q=x');
  });

  it('shortens a term that is too long and leaves blank terms out', () => {
    expect(read(`q=${'x'.repeat(300)}`).q).toHaveLength(100);
    expect(write(state({ q: '   ' }))).toBe('');
    expect(read('q=%20%20').q).toBe('');
  });

  it('says whether anything is applied', () => {
    expect(isFiltered(state({ q: 'x' }))).toBe(true);
    expect(isFiltered(state({ organizers: { 1: 2 } }))).toBe(true);
    expect(isFiltered(state({ organizers: { 1: undefined } }))).toBe(false);
    // Sorting alone is not filtering
    expect(isFiltered(state({ sort: 'name' }))).toBe(false);
  });
});

describe('organizer chips on a card', () => {
  const organizer = (
    id: number,
    name: string,
    display_order: number,
    input_type: 'select' | 'text' | 'image' = 'select'
  ): OrganizerTypeWithOptions => ({
    id,
    inventory_id: 1,
    name,
    input_type,
    is_required: false,
    display_order,
    options: [],
  });
  const organizers = [
    organizer(3, 'Serial', 3, 'text'),
    organizer(1, 'Brand', 1),
    organizer(2, 'Room', 2),
  ];

  it('lists the values in organizer order with their organizer names', () => {
    const rows = [value(1, 2, 20, 'Garage'), value(1, 3, null, 'SN-1'), value(1, 1, 10, 'Bosch')];
    expect(cardChips(rows, organizers, 3)).toEqual({
      chips: [
        { organizer: 'Brand', value: 'Bosch' },
        { organizer: 'Room', value: 'Garage' },
        { organizer: 'Serial', value: 'SN-1' },
      ],
      more: 0,
    });
  });

  it('shows at most the limit and counts the rest', () => {
    const rows = [value(1, 1, 10, 'Bosch'), value(1, 2, 20, 'Garage'), value(1, 3, null, 'SN-1')];
    expect(cardChips(rows, organizers, 2)).toEqual({
      chips: [
        { organizer: 'Brand', value: 'Bosch' },
        { organizer: 'Room', value: 'Garage' },
      ],
      more: 1,
    });
    expect(cardChips(rows, organizers, 0).more).toBe(3);
  });

  it('has nothing to show for an item without values or with a value of a deleted organizer', () => {
    expect(cardChips([], organizers, 3)).toEqual({ chips: [], more: 0 });
    expect(cardChips([value(1, 99, 1, 'Ghost')], organizers, 3)).toEqual({ chips: [], more: 0 });
  });
});

describe('organizer choices that the inventory no longer has', () => {
  const organizers: OrganizerTypeWithOptions[] = [
    {
      id: 1,
      inventory_id: 1,
      name: 'Brand',
      input_type: 'select',
      is_required: false,
      display_order: 1,
      options: [
        { id: 10, organizer_type_id: 1, name: 'Bosch', display_order: 1 },
        { id: 11, organizer_type_id: 1, name: 'Makita', display_order: 2 },
      ],
    },
    {
      id: 2,
      inventory_id: 1,
      name: 'Serial',
      input_type: 'text',
      is_required: false,
      display_order: 2,
      options: [],
    },
  ];

  it('keeps a valid option and "none"', () => {
    expect(pruneSelection({ 1: 10 }, organizers)).toEqual({ 1: 10 });
    expect(pruneSelection({ 1: 'none' }, organizers)).toEqual({ 1: 'none' });
  });

  it('drops a deleted option, an unknown organizer and a non-select organizer', () => {
    expect(pruneSelection({ 1: 99 }, organizers)).toEqual({});
    expect(pruneSelection({ 7: 10 }, organizers)).toEqual({});
    expect(pruneSelection({ 2: 'none' }, organizers)).toEqual({});
    expect(pruneSelection({ 1: 11, 2: 5, 3: 'none' }, organizers)).toEqual({ 1: 11 });
  });
});
