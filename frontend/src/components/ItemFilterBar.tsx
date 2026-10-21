import { useEffect, useRef, useState } from 'react';
import type { OrganizerTypeWithOptions } from '@/types';
import {
  SEARCH_TERM_MAX,
  SORT_KEYS,
  SORT_LABELS,
  defaultDir,
  isFiltered,
  normalizeTerm,
  type ItemFilterState,
  type SortKey,
} from '@/utils/itemFilter';
import type { OrganizerSelection } from '@/utils/reportQuery';

/** How long typing pauses before the search is written to the page address. */
const SEARCH_DEBOUNCE_MS = 300;

interface ItemFilterBarProps {
  filter: ItemFilterState;
  /** All organizers of the inventory; only select organizers with options get a dropdown */
  organizers: OrganizerTypeWithOptions[];
  shown: number;
  total: number;
  onChange: (next: ItemFilterState) => void;
}

export function ItemFilterBar({ filter, organizers, shown, total, onChange }: ItemFilterBarProps) {
  const selects = organizers.filter(
    (o) => o.input_type === 'select' && o.id !== undefined && o.options.length > 0
  );

  // The box keeps what was typed; the page address follows after a pause. Without the pause every
  // key would rewrite the address and re-sort the whole list.
  const [draft, setDraft] = useState(filter.q);
  const written = useRef(filter.q);
  const latest = useRef({ filter, onChange });
  useEffect(() => {
    latest.current = { filter, onChange };
  });

  // The address changed from outside (back button, "Clear all"): show it in the box
  useEffect(() => {
    if (filter.q !== written.current) {
      written.current = filter.q;
      setDraft(filter.q);
    }
  }, [filter.q]);

  useEffect(() => {
    const term = normalizeTerm(draft);
    if (term === written.current) {
      return;
    }
    const timer = window.setTimeout(() => {
      written.current = term;
      latest.current.onChange({ ...latest.current.filter, q: term });
    }, SEARCH_DEBOUNCE_MS);
    return () => window.clearTimeout(timer);
  }, [draft]);

  const optionName = (organizer: OrganizerTypeWithOptions, choice: number | 'none') =>
    choice === 'none'
      ? 'No value'
      : (organizer.options.find((o) => o.id === choice)?.name ?? 'Unknown');

  const activeOrganizers = selects.flatMap((organizer) => {
    const choice = organizer.id === undefined ? undefined : filter.organizers[organizer.id];
    return organizer.id === undefined || choice === undefined
      ? []
      : [{ id: organizer.id, label: `${organizer.name}: ${optionName(organizer, choice)}` }];
  });
  const term = normalizeTerm(filter.q);

  const setOrganizer = (id: number, value: string) => {
    const others: OrganizerSelection = Object.fromEntries(
      Object.entries(filter.organizers).filter(([type]) => Number(type) !== id)
    );
    onChange({
      ...filter,
      organizers:
        value === '' ? others : { ...others, [id]: value === 'none' ? 'none' : Number(value) },
    });
  };

  const clearAll = () => {
    // Also drop what is typed but not yet written: its pending timer would write it back
    written.current = '';
    setDraft('');
    onChange({ ...filter, q: '', organizers: {} });
  };

  return (
    <div className="item-filter" role="search" aria-label="Search and filter items">
      <div className="item-filter-row">
        <div className="item-filter-field item-filter-search">
          <label htmlFor="item-filter-q" className="visually-hidden">
            Search these items
          </label>
          <i className="fas fa-search item-filter-icon" aria-hidden="true"></i>
          <input
            id="item-filter-q"
            type="search"
            value={draft}
            maxLength={SEARCH_TERM_MAX}
            placeholder="Search name, notes or organizer values"
            autoComplete="off"
            onChange={(e) => setDraft(e.target.value)}
          />
        </div>

        <div className="item-filter-field">
          <label htmlFor="item-filter-sort">Sort by</label>
          <div className="item-filter-sort">
            <select
              id="item-filter-sort"
              value={filter.sort}
              onChange={(e) => {
                const sort = e.target.value as SortKey;
                onChange({ ...filter, sort, dir: defaultDir(sort) });
              }}
            >
              {SORT_KEYS.map((key) => (
                <option key={key} value={key}>
                  {SORT_LABELS[key]}
                </option>
              ))}
            </select>
            <button
              type="button"
              className="btn btn-secondary item-filter-dir"
              onClick={() => onChange({ ...filter, dir: filter.dir === 'asc' ? 'desc' : 'asc' })}
              aria-label={`Sort order: ${filter.dir === 'asc' ? 'ascending' : 'descending'}. Switch to ${filter.dir === 'asc' ? 'descending' : 'ascending'}`}
              title={filter.dir === 'asc' ? 'Ascending' : 'Descending'}
            >
              <i
                className={`fas ${filter.dir === 'asc' ? 'fa-arrow-up' : 'fa-arrow-down'}`}
                aria-hidden="true"
              ></i>
            </button>
          </div>
        </div>
      </div>

      {selects.length > 0 && (
        <div className="item-filter-row">
          {selects.map((organizer) => {
            const id = organizer.id ?? 0;
            const choice = filter.organizers[id];
            return (
              <div className="item-filter-field" key={id}>
                <label htmlFor={`item-filter-organizer-${String(id)}`}>{organizer.name}</label>
                <select
                  id={`item-filter-organizer-${String(id)}`}
                  value={choice === undefined ? '' : String(choice)}
                  onChange={(e) => setOrganizer(id, e.target.value)}
                >
                  <option value="">Any</option>
                  {organizer.options.map((option) => (
                    <option key={option.id} value={String(option.id)}>
                      {option.name}
                    </option>
                  ))}
                  <option value="none">No value</option>
                </select>
              </div>
            );
          })}
        </div>
      )}

      <div className="item-filter-summary">
        <p className="item-filter-count" role="status" aria-live="polite">
          {isFiltered(filter)
            ? `Showing ${String(shown)} of ${String(total)} items`
            : `${String(total)} ${total === 1 ? 'item' : 'items'}`}
        </p>
        {(term !== '' || activeOrganizers.length > 0) && (
          <ul className="item-filter-chips" aria-label="Active filters">
            {term !== '' && (
              <li>
                <button
                  type="button"
                  className="filter-chip"
                  onClick={() => onChange({ ...filter, q: '' })}
                  aria-label={`Remove search filter: ${term}`}
                >
                  <span>Search: {term}</span>
                  <i className="fas fa-times" aria-hidden="true"></i>
                </button>
              </li>
            )}
            {activeOrganizers.map((active) => (
              <li key={active.id}>
                <button
                  type="button"
                  className="filter-chip"
                  onClick={() => setOrganizer(active.id, '')}
                  aria-label={`Remove filter ${active.label}`}
                >
                  <span>{active.label}</span>
                  <i className="fas fa-times" aria-hidden="true"></i>
                </button>
              </li>
            ))}
            <li>
              <button type="button" className="btn btn-ghost btn-sm" onClick={clearAll}>
                Clear all
              </button>
            </li>
          </ul>
        )}
      </div>
    </div>
  );
}
