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
