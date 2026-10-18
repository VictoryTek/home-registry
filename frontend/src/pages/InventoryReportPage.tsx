import { useState, useEffect, useCallback, useMemo, useRef } from 'react';
import { useParams, useNavigate } from 'react-router-dom';
import { Header, LoadingState, EmptyState } from '@/components';
import { reportApi, inventoryApi, organizerApi } from '@/services/api';
import { useApp } from '@/context/AppContext';
import { useAuth } from '@/context/AuthContext';
import { formatDateOnly, todayAsDateOnly, type DateFormatType } from '@/utils/dateFormat';
import { formatCurrency, type CurrencyType } from '@/utils/currencyFormat';
import {
  defaultGroupBy,
  encodeOrganizerFilters,
  roundShares,
  type OrganizerSelection,
} from '@/utils/reportQuery';
import type {
  InventoryReportData,
  InventoryReportParams,
  Inventory,
  OrganizerTypeWithOptions,
  ReportItem,
} from '@/types';

/** The filters a report was asked for: the form's values at the time of "Apply Filters". */
interface AppliedFilters {
  filters: InventoryReportParams;
  selection: OrganizerSelection;
}

export function InventoryReportPage() {
  const { id } = useParams<{ id: string }>();
  const navigate = useNavigate();
  const { showToast } = useApp();
  const { settings } = useAuth();
  const inventoryId = id ? parseInt(id, 10) : undefined;
  const [loading, setLoading] = useState(true);
  const [updating, setUpdating] = useState(false);
  const [inventory, setInventory] = useState<Inventory | null>(null);
  const [organizers, setOrganizers] = useState<OrganizerTypeWithOptions[] | null>(null);
  const [reportData, setReportData] = useState<InventoryReportData | null>(null);
  // What the form shows, and what the report was last asked for
  const noFilters: InventoryReportParams = { inventory_id: inventoryId };
  const [filters, setFilters] = useState<InventoryReportParams>(noFilters);
  const [selection, setSelection] = useState<OrganizerSelection>({});
  const [applied, setApplied] = useState<AppliedFilters>({ filters: noFilters, selection: {} });
  const [groupBy, setGroupBy] = useState<number | undefined>(undefined);
  const [showFilters, setShowFilters] = useState(false);
  const [downloading, setDownloading] = useState(false);
  const latestRequest = useRef(0);

  // The inventory's organizers decide how the report can be grouped and filtered
  useEffect(() => {
    if (inventoryId === undefined) {
      return;
    }
    let cancelled = false;
    const load = async () => {
      let list: OrganizerTypeWithOptions[] = [];
      try {
        const result = await organizerApi.getByInventory(inventoryId);
        if (result.success && result.data) {
          list = result.data;
        }
      } catch {
        // The report still works without organizers
      }
      if (!cancelled) {
        setOrganizers(list);
        setGroupBy(defaultGroupBy(list));
      }
    };
    void load();
    return () => {
      cancelled = true;
    };
  }, [inventoryId]);

  const reportParams = useMemo(
    (): InventoryReportParams => ({
      ...applied.filters,
      group_by: groupBy,
      organizers: encodeOrganizerFilters(applied.selection),
    }),
    [applied, groupBy]
  );

  const loadReport = useCallback(async () => {
    if (!id || organizers === null) {
      return;
    }

    // Answers that arrive after a newer request was sent are dropped
    latestRequest.current += 1;
    const request = latestRequest.current;
    setUpdating(true);
    try {
      const [inventoryResult, reportResult] = await Promise.all([
        inventoryApi.getById(parseInt(id, 10)),
        reportApi.getInventoryReport(reportParams),
      ]);
      if (request !== latestRequest.current) {
        return;
      }

      if (inventoryResult.success && inventoryResult.data) {
        setInventory(inventoryResult.data);
      } else {
        showToast('Inventory not found', 'error');
        navigate('/');
        return;
      }

      if (reportResult.success && reportResult.data) {
        setReportData(reportResult.data);
      } else {
        showToast(reportResult.error ?? 'Failed to load report', 'error');
      }
    } catch {
      if (request === latestRequest.current) {
        showToast('Failed to load report', 'error');
      }
    } finally {
      if (request === latestRequest.current) {
        setLoading(false);
        setUpdating(false);
      }
    }
  }, [id, organizers, reportParams, navigate, showToast]);

  useEffect(() => {
    void loadReport();
  }, [loadReport]);

  const handleFilterChange = (
    field: keyof InventoryReportParams,
    value: string | number | undefined
  ) => {
    setFilters((prev) => ({
      ...prev,
      [field]: value === '' ? undefined : value,
    }));
  };

  const handleOrganizerChange = (organizerId: number, value: string) => {
    setSelection((prev) => ({
      ...prev,
      [organizerId]: value === '' ? undefined : value === 'none' ? 'none' : Number(value),
    }));
  };

  const handleApplyFilters = () => {
    setApplied({ filters: { ...filters }, selection: { ...selection } });
  };

  const handleClearFilters = () => {
    setFilters(noFilters);
    setSelection({});
    setApplied({ filters: noFilters, selection: {} });
  };

  const handleDownloadCSV = async () => {
    setDownloading(true);
    try {
      const blob = await reportApi.downloadReportCSV(reportParams);
      const url = window.URL.createObjectURL(blob);
      const a = document.createElement('a');
      a.href = url;
      a.download = `inventory_report_${inventory?.name ?? 'all'}_${todayAsDateOnly()}.csv`;
      document.body.appendChild(a);
      a.click();
      document.body.removeChild(a);
      window.URL.revokeObjectURL(url);
      showToast('Report downloaded successfully', 'success');
    } catch {
      showToast('Failed to download CSV', 'error');
    } finally {
      setDownloading(false);
    }
  };

  const handlePrint = () => {
    window.print();
  };

  if (loading) {
    return <LoadingState message="Loading report..." />;
  }

  if (!reportData || !inventory) {
    return (
      <EmptyState
        icon="fas fa-chart-bar"
        title="No Report Data"
        text="Unable to load report data."
      />
    );
  }

  const { statistics, category_breakdown, items, group_by: grouping } = reportData;
  const dateFormat = (settings?.date_format ?? 'MM/DD/YYYY') as DateFormatType;
  const currencyFormat = (settings?.currency ?? 'USD') as CurrencyType;

  // Organizers by display order: select organizers can group and filter; select and text
  // organizers get a column in the items table (image values are file paths, not for a report)
  const ordered = [...(organizers ?? [])].sort(
    (a, b) => a.display_order - b.display_order || (a.id ?? 0) - (b.id ?? 0)
  );
  const selectOrganizers = ordered.filter((o) => o.input_type === 'select');
  const columnOrganizers = ordered.filter((o) => o.input_type !== 'image');

  // Values that only the old category/location columns still hold (no organizer carries them)
  const showLegacyCategory = items.some((item) => item.category);
  const showLegacyLocation = items.some((item) => item.location);

  // The legacy breakdown (no organizer chosen) says nothing when it is all "Uncategorized"
  const breakdownIsEmpty =
    !grouping && category_breakdown.every((group) => group.category === 'Uncategorized');
  const shares = roundShares(category_breakdown.map((group) => group.percentage_of_total));

  // Check if any filters are active
  const hasActiveFilters =
    applied.filters.from_date !== undefined ||
    applied.filters.to_date !== undefined ||
    applied.filters.min_price !== undefined ||
    applied.filters.max_price !== undefined ||
    encodeOrganizerFilters(applied.selection) !== undefined;

  return (
    <>
      <Header
        title={`${inventory.name} Report`}
        subtitle={updating ? 'Updating...' : 'View detailed inventory statistics and export data'}
        icon="fas fa-chart-bar"
      />

      <div className="content">
        <div className="inventory-report">
          <div style={{ display: 'flex', justifyContent: 'space-between', marginBottom: '1.5rem' }}>
            <button className="btn btn-ghost" onClick={() => navigate(`/inventory/${id}`)}>
              <i className="fas fa-arrow-left"></i>
              Back to Inventory
            </button>
            <div style={{ display: 'flex', gap: '0.5rem' }}>
              <button className="btn btn-secondary" onClick={() => setShowFilters(!showFilters)}>
                <i className="fas fa-filter"></i>
                {showFilters ? 'Hide Filters' : 'Show Filters'}
              </button>
              <button
                className="btn btn-secondary"
                onClick={handleDownloadCSV}
                disabled={downloading}
              >
                <i className="fas fa-download"></i>
                {downloading ? 'Downloading...' : 'Download CSV'}
              </button>
              <button className="btn btn-secondary" onClick={handlePrint}>
                <i className="fas fa-print"></i>
                Print
              </button>
            </div>
          </div>

          {/* Filters Section */}
          {showFilters && (
            <div
              className="filter-panel"
              style={{
                marginBottom: '1.5rem',
                padding: '1rem',
                background: 'var(--card-bg)',
                borderRadius: '8px',
              }}
            >
              <h3 style={{ marginBottom: '1rem' }}>
                <i className="fas fa-filter"></i>
                Filters
              </h3>
              <div
                style={{
                  display: 'grid',
                  gridTemplateColumns: 'repeat(auto-fit, minmax(200px, 1fr))',
                  gap: '1rem',
                }}
              >
                <div>
                  <label
                    htmlFor="from_date"
                    style={{ display: 'block', marginBottom: '0.5rem', fontWeight: 500 }}
                  >
                    From Date
                  </label>
                  <input
                    type="date"
                    id="from_date"
                    className="input"
                    value={filters.from_date ?? ''}
                    onChange={(e) => handleFilterChange('from_date', e.target.value)}
                  />
                </div>
                <div>
                  <label
                    htmlFor="to_date"
                    style={{ display: 'block', marginBottom: '0.5rem', fontWeight: 500 }}
                  >
                    To Date
                  </label>
                  <input
                    type="date"
                    id="to_date"
                    className="input"
                    value={filters.to_date ?? ''}
                    onChange={(e) => handleFilterChange('to_date', e.target.value)}
                  />
                </div>
                <div>
                  <label
                    htmlFor="min_price"
                    style={{ display: 'block', marginBottom: '0.5rem', fontWeight: 500 }}
                  >
                    Min Price
                  </label>
                  <input
                    type="number"
                    id="min_price"
                    className="input"
                    placeholder="0.00"
                    step="0.01"
                    value={filters.min_price ?? ''}
                    onChange={(e) =>
                      handleFilterChange(
                        'min_price',
                        e.target.value ? parseFloat(e.target.value) : undefined
                      )
                    }
                  />
                </div>
                <div>
                  <label
                    htmlFor="max_price"
                    style={{ display: 'block', marginBottom: '0.5rem', fontWeight: 500 }}
                  >
                    Max Price
                  </label>
                  <input
                    type="number"
                    id="max_price"
                    className="input"
                    placeholder="9999.99"
                    step="0.01"
                    value={filters.max_price ?? ''}
                    onChange={(e) =>
                      handleFilterChange(
                        'max_price',
                        e.target.value ? parseFloat(e.target.value) : undefined
                      )
                    }
                  />
                </div>
                {selectOrganizers.map((organizer) => (
                  <div key={organizer.id}>
                    <label
                      htmlFor={`organizer-${String(organizer.id)}`}
                      style={{ display: 'block', marginBottom: '0.5rem', fontWeight: 500 }}
                    >
                      {organizer.name}
                    </label>
                    <select
                      id={`organizer-${String(organizer.id)}`}
                      className="input"
                      value={String(selection[organizer.id ?? 0] ?? '')}
                      onChange={(e) => handleOrganizerChange(organizer.id ?? 0, e.target.value)}
                    >
                      <option value="">Any</option>
                      {[...organizer.options]
                        .sort(
                          (a, b) => a.display_order - b.display_order || (a.id ?? 0) - (b.id ?? 0)
                        )
                        .map((option) => (
                          <option key={option.id} value={option.id}>
                            {option.name}
                          </option>
                        ))}
                      <option value="none">No value</option>
                    </select>
                  </div>
                ))}
              </div>
              <div style={{ marginTop: '1rem', display: 'flex', gap: '0.5rem' }}>
                <button className="btn btn-primary" onClick={handleApplyFilters}>
                  <i className="fas fa-check"></i>
                  Apply Filters
                </button>
                {hasActiveFilters && (
                  <button className="btn btn-ghost" onClick={handleClearFilters}>
                    <i className="fas fa-times"></i>
                    Clear Filters
                  </button>
                )}
              </div>
            </div>
          )}

          {/* Statistics Cards */}
          <div
            className="stats-row"
            style={{
              display: 'grid',
              gridTemplateColumns: 'repeat(auto-fit, minmax(200px, 1fr))',
              gap: '1rem',
              marginBottom: '1.5rem',
            }}
          >
            <div className="stat-card">
              <div className="stat-icon" style={{ background: 'var(--primary-color)' }}>
                <i className="fas fa-boxes"></i>
              </div>
              <div className="stat-content">
                <div className="stat-label">Total Items</div>
                <div className="stat-value">{statistics.total_items}</div>
              </div>
            </div>
            <div className="stat-card">
              <div className="stat-icon" style={{ background: 'var(--success-color)' }}>
                <i className="fas fa-coins"></i>
              </div>
              <div className="stat-content">
                <div className="stat-label">Total Value</div>
                <div className="stat-value">
                  {formatCurrency(statistics.total_value, currencyFormat)}
                </div>
              </div>
            </div>
            <div className="stat-card">
              <div className="stat-icon" style={{ background: 'var(--warning-color)' }}>
                <i className="fas fa-chart-pie"></i>
              </div>
              <div className="stat-content">
                <div className="stat-label">
                  {grouping ? `${grouping.name} groups` : 'Categories'}
                </div>
                <div className="stat-value">{statistics.category_count}</div>
              </div>
            </div>
            <div className="stat-card">
              <div className="stat-icon" style={{ background: 'var(--info-color)' }}>
                <i className="fas fa-dollar-sign"></i>
              </div>
              <div className="stat-content">
                <div className="stat-label">Average Price</div>
                <div
                  className="stat-hint no-print"
                  style={{ fontSize: '0.75rem', color: 'var(--text-muted)' }}
                >
                  items with a price
                </div>
                <div className="stat-value">
                  {formatCurrency(statistics.average_item_value, currencyFormat)}
                </div>
              </div>
            </div>
          </div>

          {/* Breakdown */}
          <div
            style={{
              marginBottom: '1.5rem',
              padding: '1rem',
              background: 'var(--card-bg)',
              borderRadius: '8px',
            }}
          >
            <div
              style={{
                display: 'flex',
                justifyContent: 'space-between',
                alignItems: 'center',
                gap: '1rem',
                flexWrap: 'wrap',
                marginBottom: '1rem',
              }}
            >
              <h3 style={{ margin: 0 }}>
                <i className="fas fa-chart-pie"></i>
                {grouping ? `Breakdown by ${grouping.name}` : 'Category Breakdown (legacy)'}
              </h3>
              {selectOrganizers.length > 0 && (
                <div
                  className="no-print"
                  style={{ display: 'flex', alignItems: 'center', gap: '0.5rem' }}
                >
                  <label htmlFor="group_by" style={{ fontWeight: 500 }}>
                    Group by
                  </label>
                  <select
                    id="group_by"
                    className="input"
                    value={String(groupBy ?? '')}
                    onChange={(e) =>
                      setGroupBy(e.target.value === '' ? undefined : Number(e.target.value))
                    }
                  >
                    {selectOrganizers.map((organizer) => (
                      <option key={organizer.id} value={organizer.id}>
                        {organizer.name}
                      </option>
                    ))}
                  </select>
                </div>
              )}
            </div>

            {breakdownIsEmpty ? (
              <p style={{ color: 'var(--text-muted)' }}>
                {selectOrganizers.length === 0
                  ? 'Add a select organizer (for example "Category") to this inventory to see a breakdown of its items.'
                  : 'Choose an organizer to group by.'}
              </p>
            ) : category_breakdown.length === 0 ? (
              <p style={{ color: 'var(--text-muted)' }}>No items match the current filters.</p>
            ) : (
              <div className="table-container">
                <table className="table">
                  <thead>
                    <tr>
                      <th>{grouping ? grouping.name : 'Category'}</th>
                      <th style={{ textAlign: 'right' }}>Items</th>
                      <th style={{ textAlign: 'right' }}>Quantity</th>
                      <th style={{ textAlign: 'right' }}>Total Value</th>
                      <th style={{ textAlign: 'right' }}>Share</th>
                    </tr>
                  </thead>
                  <tbody>
                    {category_breakdown.map((group, index) => (
                      <tr key={group.organizer_option_id ?? `group-${group.category}`}>
                        <td>
                          <strong>{group.category}</strong>
                        </td>
                        <td style={{ textAlign: 'right' }}>{group.item_count}</td>
                        <td style={{ textAlign: 'right' }}>{group.total_quantity}</td>
                        <td style={{ textAlign: 'right' }}>
                          {formatCurrency(group.total_value, currencyFormat)}
                        </td>
                        <td style={{ textAlign: 'right' }}>{(shares[index] ?? 0).toFixed(1)}%</td>
                      </tr>
                    ))}
                  </tbody>
                </table>
              </div>
            )}
          </div>

          {/* Items Table */}
          <div style={{ padding: '1rem', background: 'var(--card-bg)', borderRadius: '8px' }}>
            <h3 style={{ marginBottom: '1rem' }}>
              <i className="fas fa-list"></i>
              Items ({items.length})
            </h3>
            {items.length === 0 ? (
              <EmptyState
                icon="fas fa-box-open"
                title="No Items Found"
                text="No items match the current filters."
              />
            ) : (
              <div className="table-container">
                <table className="table">
                  <thead>
                    <tr>
                      <th>Name</th>
                      {columnOrganizers.map((organizer) => (
                        <th key={organizer.id}>{organizer.name}</th>
                      ))}
                      {showLegacyCategory && <th>Legacy category</th>}
                      {showLegacyLocation && <th>Legacy location</th>}
                      <th style={{ textAlign: 'center' }}>Quantity</th>
                      <th style={{ textAlign: 'right' }}>Price</th>
                      <th style={{ textAlign: 'right' }}>Total Value</th>
                      <th>Purchase Date</th>
                    </tr>
                  </thead>
                  <tbody>
                    {items.map((item: ReportItem) => {
                      const itemValue = (item.purchase_price ?? 0) * (item.quantity ?? 1);
                      return (
                        <tr key={item.id}>
                          <td>
                            <strong>{item.name}</strong>
                            {item.description && (
                              <div
                                style={{
                                  fontSize: '0.85rem',
                                  color: 'var(--text-muted)',
                                  marginTop: '0.25rem',
                                }}
                              >
                                {item.description}
                              </div>
                            )}
                          </td>
                          {columnOrganizers.map((organizer) => {
                            const organizerValue = item.organizer_values?.[String(organizer.id)];
                            return (
                              <td key={organizer.id}>
                                {organizerValue ? (
                                  <span className="item-card-category">{organizerValue}</span>
                                ) : (
                                  <span style={{ color: 'var(--text-muted)' }}>-</span>
                                )}
                              </td>
                            );
                          })}
                          {showLegacyCategory && <td>{item.category ?? '-'}</td>}
                          {showLegacyLocation && <td>{item.location ?? '-'}</td>}
                          <td style={{ textAlign: 'center' }}>{item.quantity ?? 1}</td>
                          <td style={{ textAlign: 'right' }}>
                            {typeof item.purchase_price === 'number'
                              ? formatCurrency(item.purchase_price, currencyFormat)
                              : '-'}
                          </td>
                          <td style={{ textAlign: 'right' }}>
                            {formatCurrency(itemValue, currencyFormat)}
                          </td>
                          <td>
                            {item.purchase_date
                              ? formatDateOnly(item.purchase_date, dateFormat)
                              : '-'}
                          </td>
                        </tr>
                      );
                    })}
                  </tbody>
                </table>
              </div>
            )}
          </div>

          {/* Report Footer */}
          <div
            style={{
              marginTop: '1.5rem',
              padding: '1rem',
              background: 'var(--card-bg)',
              borderRadius: '8px',
              fontSize: '0.85rem',
              color: 'var(--text-muted)',
            }}
          >
            <p>
              <i className="fas fa-clock"></i> Report generated at:{' '}
              {new Date(reportData.generated_at).toLocaleString()}
            </p>
            {hasActiveFilters && (
              <p style={{ marginTop: '0.5rem' }}>
                <i className="fas fa-info-circle"></i> Filters active: the items, the statistics and
                the breakdown all describe the same filtered subset of items.
              </p>
            )}
          </div>
        </div>
      </div>

      {/* Print Styles */}
      <style>{`
        @media print {
          .sidebar,
          .btn,
          .filter-panel,
          .no-print {
            display: none !important;
          }
          .inventory-report {
            max-width: 100%;
          }
          .stat-card {
            break-inside: avoid;
          }
          table {
            font-size: 0.85rem;
          }
          .main-content {
            margin: 0;
            padding: 1rem;
          }
        }
      `}</style>
    </>
  );
}
