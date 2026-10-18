import { useState, useEffect, useCallback } from 'react';
import { useParams, useNavigate } from 'react-router-dom';
import { Header, LoadingState, EmptyState } from '@/components';
import { reportApi, inventoryApi } from '@/services/api';
import { useApp } from '@/context/AppContext';
import { useAuth } from '@/context/AuthContext';
import { formatDateOnly, todayAsDateOnly, type DateFormatType } from '@/utils/dateFormat';
import { formatCurrency, type CurrencyType } from '@/utils/currencyFormat';
import type { InventoryReportData, InventoryReportParams, Inventory, Item } from '@/types';

export function InventoryReportPage() {
  const { id } = useParams<{ id: string }>();
  const navigate = useNavigate();
  const { showToast } = useApp();
  const { settings } = useAuth();
  const [loading, setLoading] = useState(true);
  const [inventory, setInventory] = useState<Inventory | null>(null);
  const [reportData, setReportData] = useState<InventoryReportData | null>(null);
  const [filters, setFilters] = useState<InventoryReportParams>({
    inventory_id: id ? parseInt(id, 10) : undefined,
    from_date: undefined,
    to_date: undefined,
    min_price: undefined,
    max_price: undefined,
    category: undefined,
  });
  const [showFilters, setShowFilters] = useState(false);
  const [downloading, setDownloading] = useState(false);

  const loadReport = useCallback(async () => {
    if (!id) {
      return;
    }

    setLoading(true);
    try {
      const [inventoryResult, reportResult] = await Promise.all([
        inventoryApi.getById(parseInt(id, 10)),
        reportApi.getInventoryReport(filters),
      ]);

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
      showToast('Failed to load report', 'error');
    } finally {
      setLoading(false);
    }
  }, [id, filters, navigate, showToast]);

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

  const handleApplyFilters = () => {
    void loadReport();
  };

  const handleClearFilters = () => {
    setFilters({
      inventory_id: id ? parseInt(id, 10) : undefined,
      from_date: undefined,
      to_date: undefined,
      min_price: undefined,
      max_price: undefined,
      category: undefined,
    });
  };

  const handleDownloadCSV = async () => {
    setDownloading(true);
    try {
      const blob = await reportApi.downloadReportCSV(filters);
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

  const { statistics, category_breakdown, items } = reportData;
  const dateFormat = (settings?.date_format ?? 'MM/DD/YYYY') as DateFormatType;
  const currencyFormat = (settings?.currency ?? 'USD') as CurrencyType;

  // Get unique categories for filter dropdown
  const uniqueCategories = Array.from(
    new Set(
      items.map((item) => item.category).filter((c): c is string => c !== undefined && c !== '')
    )
  ).sort();

  // Check if any filters are active
  const hasActiveFilters =
    filters.from_date !== undefined ||
    filters.to_date !== undefined ||
    filters.min_price !== undefined ||
    filters.max_price !== undefined ||
    filters.category !== undefined;

  return (
    <>
      <Header
        title={`${inventory.name} Report`}
        subtitle="View detailed inventory statistics and export data"
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
                <div>
                  <label
                    htmlFor="category"
                    style={{ display: 'block', marginBottom: '0.5rem', fontWeight: 500 }}
                  >
                    Category
                  </label>
                  <select
                    id="category"
                    className="input"
                    value={filters.category ?? ''}
                    onChange={(e) => handleFilterChange('category', e.target.value)}
                  >
                    <option value="">All Categories</option>
                    {uniqueCategories.map((cat) => (
                      <option key={cat} value={cat}>
                        {cat}
                      </option>
                    ))}
                  </select>
                </div>
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
                <div className="stat-label">Categories</div>
                <div className="stat-value">{statistics.category_count}</div>
              </div>
            </div>
            <div className="stat-card">
              <div className="stat-icon" style={{ background: 'var(--info-color)' }}>
                <i className="fas fa-dollar-sign"></i>
              </div>
              <div className="stat-content">
                <div className="stat-label">Average Price</div>
                <div className="stat-value">
                  {formatCurrency(statistics.average_price, currencyFormat)}
                </div>
              </div>
            </div>
          </div>

          {/* Category Breakdown */}
          {category_breakdown.length > 0 && (
            <div
              style={{
                marginBottom: '1.5rem',
                padding: '1rem',
                background: 'var(--card-bg)',
                borderRadius: '8px',
              }}
            >
              <h3 style={{ marginBottom: '1rem' }}>
                <i className="fas fa-chart-pie"></i>
                Category Breakdown
              </h3>
              <div className="table-container">
                <table className="table">
                  <thead>
                    <tr>
                      <th>Category</th>
                      <th style={{ textAlign: 'right' }}>Items</th>
                      <th style={{ textAlign: 'right' }}>Total Value</th>
                    </tr>
                  </thead>
                  <tbody>
                    {category_breakdown.map((cat) => (
                      <tr key={cat.category}>
                        <td>
                          <strong>{cat.category || 'Uncategorized'}</strong>
                        </td>
                        <td style={{ textAlign: 'right' }}>{cat.item_count}</td>
                        <td style={{ textAlign: 'right' }}>
                          {formatCurrency(cat.total_value, currencyFormat)}
                        </td>
                      </tr>
                    ))}
                  </tbody>
                </table>
              </div>
            </div>
          )}

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
                      <th>Category</th>
                      <th>Location</th>
                      <th style={{ textAlign: 'center' }}>Quantity</th>
                      <th style={{ textAlign: 'right' }}>Price</th>
                      <th style={{ textAlign: 'right' }}>Total Value</th>
                      <th>Purchase Date</th>
                    </tr>
                  </thead>
                  <tbody>
                    {items.map((item: Item) => {
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
                          <td>
                            {item.category ? (
                              <span className="item-card-category">{item.category}</span>
                            ) : (
                              <span style={{ color: 'var(--text-muted)' }}>-</span>
                            )}
                          </td>
                          <td>{item.location ?? '-'}</td>
                          <td style={{ textAlign: 'center' }}>{item.quantity ?? 1}</td>
                          <td style={{ textAlign: 'right' }}>
                            {item.purchase_price !== undefined
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
                <i className="fas fa-info-circle"></i> Filters active: This report shows a filtered
                subset of items.
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
