import { renderToStaticMarkup } from 'react-dom/server';
import { describe, expect, it } from 'vitest';
import type { OrganizerTypeWithOptions } from '@/types';
import { DEFAULT_FILTER, type ItemFilterState } from '@/utils/itemFilter';
import { ItemFilterBar } from './ItemFilterBar';

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
  {
    id: 3,
    inventory_id: 1,
    name: 'Photo',
    input_type: 'image',
    is_required: false,
    display_order: 3,
    options: [],
  },
];

function render(filter: ItemFilterState, shown = 3, total = 3): string {
  return renderToStaticMarkup(
    <ItemFilterBar
      filter={filter}
      organizers={organizers}
      shown={shown}
      total={total}
      onChange={() => undefined}
    />
  );
}

describe('the item filter bar', () => {
  it('labels every control', () => {
    const html = render(DEFAULT_FILTER);
    // Each label points at a control with that id
    for (const [id, text] of [
      ['item-filter-q', 'Search these items'],
      ['item-filter-sort', 'Sort by'],
      ['item-filter-organizer-1', 'Brand'],
    ] as const) {
      expect(html).toContain(`<label for="${id}"`);
      expect(html).toContain(`id="${id}"`);
      expect(html).toContain(text);
    }
    expect(html).toContain('role="search"');
    expect(html).toContain('maxLength="100"');
  });

  it('has a dropdown for select organizers only, with Any and No value', () => {
    const html = render(DEFAULT_FILTER);
    expect(html).toContain('item-filter-organizer-1');
    expect(html).not.toContain('item-filter-organizer-2');
    expect(html).not.toContain('item-filter-organizer-3');
    expect(html).toContain('>Any</option>');
    expect(html).toContain('>Bosch</option>');
    expect(html).toContain('>No value</option>');
  });

  it('lists every way to sort and says which way it sorts', () => {
    const html = render(DEFAULT_FILTER);
    for (const label of ['Recently added', 'Name', 'Price', 'Purchase date', 'Warranty expiry']) {
      expect(html).toContain(`>${label}</option>`);
    }
    expect(html).toContain('Sort order: descending. Switch to ascending');
    expect(render({ ...DEFAULT_FILTER, sort: 'name', dir: 'asc' })).toContain(
      'Sort order: ascending. Switch to descending'
    );
  });

  it('shows the plain count without filters and "Showing x of y" with them', () => {
    const plain = render(DEFAULT_FILTER, 3, 3);
    expect(plain).toContain('3 items');
    expect(plain).not.toContain('Showing');
    expect(plain).not.toContain('Clear all');
    expect(render(DEFAULT_FILTER, 1, 1)).toContain('1 item<');

    const filtered = render({ ...DEFAULT_FILTER, q: 'drill' }, 1, 3);
    expect(filtered).toContain('Showing 1 of 3 items');
    expect(filtered).toContain('Clear all');
  });

  it('shows a removable chip for each active filter, named for a screen reader', () => {
    const html = render({ ...DEFAULT_FILTER, q: 'drill', organizers: { 1: 11 } });
    expect(html).toContain('aria-label="Remove search filter: drill"');
    expect(html).toContain('aria-label="Remove filter Brand: Makita"');
    const none = render({ ...DEFAULT_FILTER, organizers: { 1: 'none' } });
    expect(none).toContain('aria-label="Remove filter Brand: No value"');
  });

  it('shows the search text typed so far and escapes it', () => {
    const html = render({ ...DEFAULT_FILTER, q: '<b>"x"</b>' });
    expect(html).not.toContain('<b>');
    expect(html).toContain('&lt;b&gt;');
  });
});
