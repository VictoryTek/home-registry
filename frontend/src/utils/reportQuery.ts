/**
 * Helpers for the inventory report's organizer grouping and filters.
 */

import type { OrganizerTypeWithOptions } from '@/types';

/** The option chosen for each organizer: an option id, `'none'` (no value), or nothing. */
export type OrganizerSelection = Record<number, number | 'none' | undefined>;

/**
 * The `organizers` query parameter for a selection: `type:option` pairs in organizer order, e.g.
 * `3:12,4:none`. Returns undefined when nothing is selected, so the parameter is left out.
 */
export function encodeOrganizerFilters(selection: OrganizerSelection): string | undefined {
  const pairs = Object.entries(selection)
    .map(([type, option]) => [Number(type), option] as const)
    .filter(([type, option]) => Number.isInteger(type) && type > 0 && option !== undefined)
    .sort(([a], [b]) => a - b)
    .map(([type, option]) => `${String(type)}:${String(option)}`);
  return pairs.length > 0 ? pairs.join(',') : undefined;
}

/**
 * The organizer a report is grouped by when the user has not chosen one: the first select
 * organizer in display order, then by id. Undefined when the inventory has no select organizer.
 */
export function defaultGroupBy(organizers: OrganizerTypeWithOptions[]): number | undefined {
  const selects = organizers
    .filter((o) => o.input_type === 'select' && o.id !== undefined)
    .sort((a, b) => a.display_order - b.display_order || (a.id ?? 0) - (b.id ?? 0));
  return selects[0]?.id;
}

/**
 * Shares rounded to `decimals` places that add up to exactly the rounded sum (100 for a full
 * breakdown): each value is rounded down and the leftover units go to the values with the
 * largest remainders (the largest remainder method). An empty or all-zero list stays zero.
 */
export function roundShares(values: number[], decimals = 1): number[] {
  const scale = 10 ** decimals;
  const total = values.reduce((sum, v) => sum + v, 0);
  if (total <= 0) {
    return values.map(() => 0);
  }
  const target = Math.round(total * scale);
  const scaled = values.map((v) => v * scale);
  const floors = scaled.map((v) => Math.floor(v + 1e-9));
  let leftover = target - floors.reduce((sum, v) => sum + v, 0);

  const order = scaled
    .map((v, index) => ({ index, remainder: v - Math.floor(v + 1e-9) }))
    .sort((a, b) => b.remainder - a.remainder || a.index - b.index);
  const result = [...floors];
  for (const { index } of order) {
    if (leftover <= 0) {
      break;
    }
    result[index] = (result[index] ?? 0) + 1;
    leftover -= 1;
  }
  return result.map((v) => v / scale);
}

/**
 * The total value of a report row: price times quantity (a missing quantity counts as 1), or
 * null when the item has no price, which the CSV leaves blank as well.
 */
export function itemTotalValue(item: {
  purchase_price?: number;
  quantity?: number;
}): number | null {
  return typeof item.purchase_price === 'number'
    ? item.purchase_price * (item.quantity ?? 1)
    : null;
}

/**
 * What to say instead of the breakdown table, or null when the table should be shown. The legacy
 * breakdown (no organizer chosen) says nothing when every item is "Uncategorized".
 */
export function breakdownNotice(input: {
  groupCount: number;
  grouped: boolean;
  onlyUncategorized: boolean;
  selectOrganizerCount: number;
  hasActiveFilters: boolean;
}): string | null {
  if (input.groupCount === 0) {
    return input.hasActiveFilters
      ? 'No items match the current filters.'
      : 'This inventory has no items yet.';
  }
  if (!input.grouped && input.onlyUncategorized) {
    return input.selectOrganizerCount === 0
      ? 'Add a select organizer (for example "Category") to this inventory to see a breakdown of its items.'
      : 'Choose an organizer to group by.';
  }
  return null;
}

/** The `error` texts the report API answers with for an organizer or option the inventory lacks. */
const STALE_ORGANIZER_ERRORS = ['Unknown organizer or option', 'Not a select organizer'];

/** Whether a report failed because it named an organizer that has been deleted or changed. */
export function isStaleOrganizerError(error: string | undefined): boolean {
  return error !== undefined && STALE_ORGANIZER_ERRORS.includes(error);
}

/**
 * Whether to reload the organizers and ask again. Only once per failure: if the reload did not
 * cure it (`alreadyTried`), the error is shown instead, so the page cannot loop.
 */
export function shouldRecoverFromStaleOrganizers(
  error: string | undefined,
  alreadyTried: boolean
): boolean {
  return isStaleOrganizerError(error) && !alreadyTried;
}

/** The grouping to keep after the organizers changed: the same one if it still exists, else the default. */
export function keepGroupBy(
  groupBy: number | undefined,
  organizers: OrganizerTypeWithOptions[]
): number | undefined {
  const stillThere = organizers.some((o) => o.id === groupBy && o.input_type === 'select');
  return stillThere ? groupBy : defaultGroupBy(organizers);
}
