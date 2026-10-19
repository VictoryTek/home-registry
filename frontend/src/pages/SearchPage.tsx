import { useEffect, useMemo, useState } from 'react';
import { useNavigate, useSearchParams } from 'react-router-dom';
import { Header, EmptyState, LoadingState } from '@/components';
import { itemApi } from '@/services/api';
import { normalizeTerm } from '@/utils/itemFilter';
import type { ItemSearchHit, ItemSearchResults } from '@/types';

type SearchState =
  | { status: 'idle' }
  | { status: 'loading' }
  | { status: 'failed'; message: string }
  | { status: 'done'; found: ItemSearchResults };

/** The hits grouped by inventory, in the order of each inventory's best hit. */
function groupByInventory(
  hits: ItemSearchHit[]
): { inventoryId: number; name: string; hits: ItemSearchHit[] }[] {
  const groups = new Map<number, { inventoryId: number; name: string; hits: ItemSearchHit[] }>();
  for (const hit of hits) {
    const group = groups.get(hit.inventory_id);
    if (group) {
      group.hits.push(hit);
    } else {
      groups.set(hit.inventory_id, {
        inventoryId: hit.inventory_id,
        name: hit.inventory_name,
        hits: [hit],
      });
    }
  }
  return [...groups.values()];
}

export function SearchPage() {
  const navigate = useNavigate();
  const [params] = useSearchParams();
  const term = normalizeTerm(params.get('q') ?? '');
  const [state, setState] = useState<SearchState>({ status: 'idle' });

  useEffect(() => {
    if (term === '') {
      setState({ status: 'idle' });
      return;
    }
    // Only the newest search may fill the page: an older answer can arrive later
    let current = true;
    setState({ status: 'loading' });
    itemApi
      .search(term)
      .then((result) => {
        if (!current) {
          return;
        }
        if (result.success && result.data) {
          setState({ status: 'done', found: result.data });
        } else {
          setState({
            status: 'failed',
            message: result.message ?? result.error ?? 'The search failed',
          });
        }
      })
      .catch(() => {
        if (current) {
          setState({ status: 'failed', message: 'The search failed. Check your connection.' });
        }
      });
    return () => {
      current = false;
    };
  }, [term]);

  const groups = useMemo(
    () => (state.status === 'done' ? groupByInventory(state.found.results) : []),
    [state]
  );

  // Opens the item on its inventory's page, the way a warranty notification does
  const openHit = (hit: ItemSearchHit) => {
    navigate(`/inventory/${String(hit.inventory_id)}`, { state: { openItemId: hit.id } });
  };

  return (
    <>
      <Header
        title="Search"
        subtitle={term === '' ? 'Find items in all your inventories' : `Results for “${term}”`}
        icon="fas fa-search"
      />
      <div className="content">
        {state.status === 'idle' && (
          <EmptyState
            icon="fas fa-search"
            title="Search your items"
            text="Use the search box in the menu to look through the name, description, notes and organizer values of every item you can see."
          />
        )}
        {state.status === 'loading' && <LoadingState message="Searching..." />}
        {state.status === 'failed' && (
          <EmptyState
            icon="fas fa-exclamation-triangle"
            title="Search failed"
            text={state.message}
          />
        )}
        {state.status === 'done' && state.found.total === 0 && (
          <EmptyState
            icon="fas fa-search"
            title="No items found"
            text="Nothing matches that search. Try a shorter or different word."
          />
        )}
        {state.status === 'done' && state.found.total > 0 && (
          <div className="search-results">
            <p className="search-summary" role="status" aria-live="polite">
              {state.found.total > state.found.results.length
                ? `Showing the first ${String(state.found.results.length)} of ${String(state.found.total)} matches. Search for something more specific to narrow them down.`
                : `${String(state.found.total)} ${state.found.total === 1 ? 'match' : 'matches'}`}
            </p>
            {groups.map((group) => (
              <section
                key={group.inventoryId}
                className="search-group"
                aria-labelledby={`search-group-${String(group.inventoryId)}`}
              >
                <h2 className="section-title" id={`search-group-${String(group.inventoryId)}`}>
                  <i className="fas fa-warehouse" aria-hidden="true"></i>
                  {group.name}
                  <span className="search-group-count">{group.hits.length}</span>
                </h2>
                <ul className="search-hit-list">
                  {group.hits.map((hit) => (
                    <li key={hit.id}>
                      <button type="button" className="search-hit" onClick={() => openHit(hit)}>
                        <span className="search-hit-name">{hit.name}</span>
                        {hit.description && (
                          <span className="search-hit-description">{hit.description}</span>
                        )}
                        {hit.matches.length > 0 && (
                          <span className="item-chips">
                            {hit.matches.map((match) => (
                              <span
                                className="item-chip"
                                key={`${match.organizer}:${match.value}`}
                                title={`${match.organizer}: ${match.value}`}
                              >
                                <span className="item-chip-label">{match.organizer}</span>
                                <span className="item-chip-value">{match.value}</span>
                              </span>
                            ))}
                          </span>
                        )}
                      </button>
                    </li>
                  ))}
                </ul>
              </section>
            ))}
          </div>
        )}
      </div>
    </>
  );
}
