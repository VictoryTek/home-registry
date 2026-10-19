/**
 * Search, filter and sort for the items of one inventory, and how that state is kept in the page
 * address (`?q=drill&sort=price&dir=desc&o=3:12,4:none`).
 *
 * The search matches the same fields, with the same case-insensitive "contains" rule, as
 * `GET /api/items/search` (src/db/search.rs): name, description, notes, legacy category and
 * location, and the option name or text of select and text organizer values.
 * frontend/src/test/fixtures/item_search_cases.json holds the cases both must agree on.
 *
 * Dates are `YYYY-MM-DD` text and are compared as text, never as `Date` objects, so the time
 * zone cannot change an order (see dateFormat.ts).
 */

import type { InventoryOrganizerValue, Item, OrganizerTypeWithOptions } from '@/types';
import { encodeOrganizerFilters, type OrganizerSelection } from './reportQuery';

export type SortKey = 'created' | 'name' | 'price' | 'purchase_date' | 'warranty_expiry';
export type SortDir = 'asc' | 'desc';

export interface ItemFilterState {
  /** The search text, already normalised */
  q: string;
  sort: SortKey;
  dir: SortDir;
  /** The option chosen for each organizer, or `'none'` for items without a value */
  organizers: OrganizerSelection;
}

/** Longest search term the API accepts, in characters. */
export const SEARCH_TERM_MAX = 100;
/** At most this many organizer choices are read from an address. */
const MAX_ORGANIZER_CHOICES = 50;

export const SORT_LABELS: Record<SortKey, string> = {
  created: 'Recently added',
  name: 'Name',
  price: 'Price',
  purchase_date: 'Purchase date',
  warranty_expiry: 'Warranty expiry',
};
export const SORT_KEYS = Object.keys(SORT_LABELS) as SortKey[];

export const DEFAULT_FILTER: ItemFilterState = {
  q: '',
  sort: 'created',
  dir: 'desc',
  organizers: {},
};

/** The direction a sort starts in: newest first, everything else lowest first. */
export function defaultDir(sort: SortKey): SortDir {
  return sort === 'created' ? 'desc' : 'asc';
}

/** A search term as the API reads it: control characters become spaces, then trimmed. */
export function normalizeTerm(raw: string): string {
  // eslint-disable-next-line no-control-regex
  return raw.replace(/[\u0000-\u001f\u007f-\u009f]/g, ' ').trim();
}

/** The organizer values of each item, by item id. */
export function indexOrganizerValues(
  rows: InventoryOrganizerValue[]
): Map<number, InventoryOrganizerValue[]> {
  const byItem = new Map<number, InventoryOrganizerValue[]>();
  for (const row of rows) {
    const list = byItem.get(row.item_id);
    if (list) {
      list.push(row);
    } else {
      byItem.set(row.item_id, [row]);
    }
  }
  return byItem;
}

/** Whether the item matches an already normalised term. An empty term matches everything. */
export function itemMatchesTerm(
  item: Item,
  values: InventoryOrganizerValue[],
  term: string
): boolean {
  if (term === '') {
    return true;
  }
  const needle = term.toLowerCase();
  const fields = [item.name, item.description, item.notes, item.category, item.location];
  return (
    fields.some((field) => field?.toLowerCase().includes(needle) === true) ||
    values.some((row) => row.value.toLowerCase().includes(needle))
  );
}

function itemMatchesOrganizers(
  values: InventoryOrganizerValue[],
  selection: OrganizerSelection
): boolean {
  return Object.entries(selection).every(([type, choice]) => {
    if (choice === undefined) {
      return true;
    }
    const typeId = Number(type);
    const mine = values.filter((row) => row.organizer_type_id === typeId);
    return choice === 'none'
      ? mine.length === 0
      : mine.some((row) => row.organizer_option_id === choice);
  });
}

export function filterItems(
  items: Item[],
  valuesByItem: Map<number, InventoryOrganizerValue[]>,
  filter: ItemFilterState
): Item[] {
  const term = normalizeTerm(filter.q);
  return items.filter((item) => {
    const values = (item.id === undefined ? undefined : valuesByItem.get(item.id)) ?? [];
    return itemMatchesTerm(item, values, term) && itemMatchesOrganizers(values, filter.organizers);
  });
}

function sortValue(item: Item, key: SortKey): number | string | null {
  switch (key) {
    case 'name':
      return item.name;
    case 'price':
      return typeof item.purchase_price === 'number' && Number.isFinite(item.purchase_price)
        ? item.purchase_price
        : null;
    case 'purchase_date':
      return item.purchase_date ? item.purchase_date : null;
    case 'warranty_expiry':
      return item.warranty_expiry ? item.warranty_expiry : null;
    case 'created': {
      // A moment in time (not a calendar date), so it is read as one: the text form varies in
      // its fraction of a second
      const time = item.created_at === undefined ? Number.NaN : Date.parse(item.created_at);
      return Number.isNaN(time) ? null : time;
    }
  }
}

function compareValues(a: number | string, b: number | string): number {
  if (typeof a === 'number' && typeof b === 'number') {
    return a - b;
  }
  const left = String(a);
  const right = String(b);
  return left.localeCompare(right, undefined, { numeric: true, sensitivity: 'base' });
}

/**
 * The items in the chosen order, as a new list. An item without a value for the sort (no price,
 * no date) comes after the others in both directions. Ties go by id, in the direction of the sort,
 * so the order never depends on how the list arrived.
 */
export function sortItems(items: Item[], key: SortKey, dir: SortDir): Item[] {
  const sign = dir === 'asc' ? 1 : -1;
  const byId = (a: Item, b: Item) => ((a.id ?? 0) - (b.id ?? 0)) * sign;
  return [...items].sort((a, b) => {
    const left = sortValue(a, key);
    const right = sortValue(b, key);
    if (left === null || right === null) {
      if (left === right) {
        return byId(a, b);
      }
      return left === null ? 1 : -1;
    }
    // Dates are text in the form YYYY-MM-DD: plain comparison keeps them in calendar order
    const order =
      key === 'purchase_date' || key === 'warranty_expiry'
        ? left < right
          ? -1
          : left > right
            ? 1
            : 0
        : compareValues(left, right);
    return order === 0 ? byId(a, b) : order * sign;
  });
}

/** The items to show: filtered, then sorted. */
export function applyFilter(
  items: Item[],
  valuesByItem: Map<number, InventoryOrganizerValue[]>,
  filter: ItemFilterState
): Item[] {
  return sortItems(filterItems(items, valuesByItem, filter), filter.sort, filter.dir);
}

/** Whether a search term or an organizer choice is applied (the sort order does not count). */
export function isFiltered(filter: ItemFilterState): boolean {
  return (
    normalizeTerm(filter.q) !== '' ||
    Object.values(filter.organizers).some((choice) => choice !== undefined)
  );
}

/**
 * The choices that still exist in this inventory: a select organizer of it, and an option of that
 * organizer (or `'none'`). An address can outlive an organizer or option, or belong to another
 * inventory; those choices are ignored rather than hiding every item.
 */
export function pruneSelection(
  selection: OrganizerSelection,
  organizers: OrganizerTypeWithOptions[]
): OrganizerSelection {
  const kept: OrganizerSelection = {};
  for (const [type, choice] of Object.entries(selection)) {
    const organizer = organizers.find((o) => o.id === Number(type) && o.input_type === 'select');
    if (
      organizer &&
      choice !== undefined &&
      (choice === 'none' || organizer.options.some((option) => option.id === choice))
    ) {
      kept[Number(type)] = choice;
    }
  }
  return kept;
}

function parseOrganizerChoices(text: string): OrganizerSelection {
  const selection: OrganizerSelection = {};
  for (const pair of text.split(',').slice(0, MAX_ORGANIZER_CHOICES)) {
    const match = /^(\d+):(\d+|none)$/.exec(pair);
    if (!match) {
      continue;
    }
    const type = Number(match[1]);
    const choice = match[2] === 'none' ? 'none' : Number(match[2]);
    if (
      Number.isSafeInteger(type) &&
      type > 0 &&
      (choice === 'none' || (Number.isSafeInteger(choice) && choice > 0))
    ) {
      selection[type] = choice;
    }
  }
  return selection;
}

/** Read the state from a page address; anything not understood is ignored. */
export function parseFilterParams(params: URLSearchParams): ItemFilterState {
  const sortParam = params.get('sort');
  const sort = SORT_KEYS.find((key) => key === sortParam) ?? DEFAULT_FILTER.sort;
  const dirParam = params.get('dir');
  const dir = dirParam === 'asc' || dirParam === 'desc' ? dirParam : defaultDir(sort);
  const q = [...normalizeTerm(params.get('q') ?? '')].slice(0, SEARCH_TERM_MAX).join('');
  return { q, sort, dir, organizers: parseOrganizerChoices(params.get('o') ?? '') };
}

/**
 * The address parameters for a state, added to `base` (a copy of it, so other parameters stay).
 * Values that are the defaults are left out, so a clean address means a clean page.
 */
export function writeFilterParams(filter: ItemFilterState, base: URLSearchParams): URLSearchParams {
  const params = new URLSearchParams(base);
  for (const key of ['q', 'sort', 'dir', 'o']) {
    params.delete(key);
  }
  const q = normalizeTerm(filter.q);
  if (q !== '') {
    params.set('q', q);
  }
  if (filter.sort !== DEFAULT_FILTER.sort) {
    params.set('sort', filter.sort);
  }
  if (filter.dir !== defaultDir(filter.sort)) {
    params.set('dir', filter.dir);
  }
  const organizers = encodeOrganizerFilters(filter.organizers);
  if (organizers !== undefined) {
    params.set('o', organizers);
  }
  return params;
}

export interface CardChip {
  organizer: string;
  value: string;
}

/**
 * The organizer values to show on an item's card, in organizer order, at most `max` of them, and
 * how many more there are. Values of organizers the inventory no longer has are left out.
 */
export function cardChips(
  values: InventoryOrganizerValue[],
  organizers: OrganizerTypeWithOptions[],
  max: number
): { chips: CardChip[]; more: number } {
  const order = new Map(
    [...organizers]
      .sort((a, b) => a.display_order - b.display_order || (a.id ?? 0) - (b.id ?? 0))
      .flatMap((o, index) => (o.id === undefined ? [] : [[o.id, { index, name: o.name }] as const]))
  );
  const known = values
    .flatMap((row) => {
      const organizer = order.get(row.organizer_type_id);
      return organizer
        ? [{ index: organizer.index, chip: { organizer: organizer.name, value: row.value } }]
        : [];
    })
    .sort((a, b) => a.index - b.index);
  return {
    chips: known.slice(0, Math.max(0, max)).map((entry) => entry.chip),
    more: Math.max(0, known.length - Math.max(0, max)),
  };
}
