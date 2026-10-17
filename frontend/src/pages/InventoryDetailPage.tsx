import { useState, useEffect, useCallback, useMemo } from 'react';
import { useParams, useNavigate, useLocation } from 'react-router-dom';
import {
  Header,
  LoadingState,
  EmptyState,
  Modal,
  ConfirmModal,
  ShareInventoryModal,
  ImageLightbox,
  ImageOrganizerInput,
} from '@/components';
import { inventoryApi, itemApi, organizerApi, imageApi } from '@/services/api';
import { useApp } from '@/context/AppContext';
import { useAuth } from '@/context/AuthContext';
import { formatDate, type DateFormatType } from '@/utils/dateFormat';
import { formatCurrency, type CurrencyType } from '@/utils/currencyFormat';
import { getNotificationMessage } from '@/utils/notifications';
import type {
  Inventory,
  Item,
  CreateItemRequest,
  UpdateItemRequest,
  OrganizerTypeWithOptions,
  SetItemOrganizerValueRequest,
  ItemOrganizerValueWithDetails,
} from '@/types';

export function InventoryDetailPage() {
  const { id } = useParams<{ id: string }>();
  const navigate = useNavigate();
  const location = useLocation();
  const { showToast, setItems: setGlobalItems, warrantyNotifications } = useApp();
  const { settings } = useAuth();
  const [loading, setLoading] = useState(true);
  const [inventory, setInventory] = useState<Inventory | null>(null);
  const [items, setItems] = useState<Item[]>([]);
  const [organizers, setOrganizers] = useState<OrganizerTypeWithOptions[]>([]);
  const [showAddItemModal, setShowAddItemModal] = useState(false);
  const [showShareModal, setShowShareModal] = useState(false);

  // Issue 1: View Details modal state
  const [viewingItem, setViewingItem] = useState<Item | null>(null);
  const [viewingItemOrganizerValues, setViewingItemOrganizerValues] = useState<
    ItemOrganizerValueWithDetails[]
  >([]);

  // Issue 2: Edit Item modal state
  const [showEditItemModal, setShowEditItemModal] = useState(false);
  const [editingItem, setEditingItem] = useState<Item | null>(null);
  const [editItemData, setEditItemData] = useState<UpdateItemRequest>({});
  const [editOrganizerValues, setEditOrganizerValues] = useState<
    Record<string, { optionId?: number; textValue?: string }>
  >({});

  // Issue 3: Delete Confirmation modal state
  const [showDeleteItemModal, setShowDeleteItemModal] = useState(false);
  const [deletingItem, setDeletingItem] = useState<Item | null>(null);
  const [newItem, setNewItem] = useState<CreateItemRequest>({
    inventory_id: parseInt(id ?? '0', 10),
    name: '',
    description: '',
    purchase_date: undefined,
    purchase_price: undefined,
    warranty_expiry: undefined,
    quantity: 1,
  });
  const [organizerValues, setOrganizerValues] = useState<
    Record<string, { optionId?: number; textValue?: string }>
  >({});

  // Image organizer state
  const [itemImages, setItemImages] = useState<Record<string, string>>({});
  const [lightboxOpen, setLightboxOpen] = useState(false);
  const [lightboxUrl, setLightboxUrl] = useState('');
  const [lightboxAlt, setLightboxAlt] = useState('');

  // Determine if image section should be displayed on item cards
  // Show images only if BOTH conditions are true:
  // 1. Inventory has an Image Organizer (input_type === 'image')
  // 2. At least one item in the inventory has an image
  const shouldShowImageSection = useMemo(() => {
    // Check if inventory has an Image Organizer
    const hasImageOrganizer = organizers.some((org) => org.input_type === 'image');

    // Check if any item has an image
    const hasAnyImages = Object.keys(itemImages).length > 0;

    return hasImageOrganizer && hasAnyImages;
  }, [organizers, itemImages]);

  // Enhancement 2: Helper to get notification for an item
  const getItemNotification = (itemId: number | undefined) => {
    if (!itemId) {
      return null;
    }
    return warrantyNotifications.find((n) => n.id === itemId);
  };

  // Empty dependency array - all functions used are stable:
  // - navigate is stable (from react-router)
  // - setGlobalItems is a state setter (stable)
  // - showToast is now wrapped in useCallback (stable)
  const loadInventoryDetail = useCallback(async (inventoryId: number) => {
    setLoading(true);
    try {
      const [invResult, itemsResult, organizersResult] = await Promise.all([
        inventoryApi.getById(inventoryId),
        inventoryApi.getItems(inventoryId),
        organizerApi.getByInventory(inventoryId),
      ]);

      if (invResult.success && invResult.data) {
        setInventory(invResult.data);
      } else {
        showToast('Inventory not found', 'error');
        navigate('/');
        return;
      }

      if (itemsResult.success && itemsResult.data) {
        setItems(itemsResult.data);
        setGlobalItems(itemsResult.data); // Update global items state for notifications
      }

      if (organizersResult.success && organizersResult.data) {
        setOrganizers(organizersResult.data);

        // Fetch item images if any image-type organizers exist
        const hasImageOrg = organizersResult.data.some((o) => o.input_type === 'image');
        if (hasImageOrg) {
          try {
            const imgResult = await imageApi.getInventoryItemImages(inventoryId);
            if (imgResult.success && imgResult.data) {
              setItemImages(imgResult.data);
            }
          } catch {
            // Non-critical, continue without thumbnails
          }
        }
      }
    } catch {
      showToast('Failed to load inventory', 'error');
      navigate('/');
    } finally {
      setLoading(false);
    }
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, []); // Empty deps - all functions are stable

  useEffect(() => {
    if (id) {
      void loadInventoryDetail(parseInt(id, 10));
    }
  }, [id, loadInventoryDetail]);

  // Enhancement 1: Auto-open item details modal if navigated from notification
  // RECOMMENDED FIX: Extract primitive value to prevent unnecessary re-runs
  const openItemId = (location.state as { openItemId?: number } | null)?.openItemId;

  useEffect(() => {
    if (openItemId && items.length > 0) {
      const item = items.find((i) => i.id === openItemId);
      if (item) {
        void handleViewItem(item);
        // Clear navigation state to prevent re-opening on next visit
        window.history.replaceState({}, document.title);
      }
    }
  }, [items, openItemId]);

  const handleAddItem = async () => {
    // The server has no default inventory: never send an item without a valid one
    const inventoryId = parseInt(id ?? '', 10);
    if (!Number.isInteger(inventoryId) || inventoryId < 1) {
      showToast('This page has no valid inventory', 'error');
      return;
    }

    if (!newItem.name.trim()) {
      showToast('Please enter an item name', 'error');
      return;
    }

    // Check required organizers
    for (const org of organizers) {
      if (org.is_required && org.id) {
        const value = organizerValues[String(org.id)];
        if (
          !value ||
          (org.input_type === 'select' && !value.optionId) ||
          (org.input_type === 'text' && !value.textValue?.trim()) ||
          (org.input_type === 'image' && !value.textValue?.trim())
        ) {
          showToast(`Please fill in the required field: ${org.name}`, 'error');
          return;
        }
      }
    }

    try {
      const result = await itemApi.create({
        ...newItem,
        inventory_id: inventoryId,
      });

      if (result.success && result.data) {
        // Save organizer values if any are set
        const valuesToSave: SetItemOrganizerValueRequest[] = [];
        for (const [typeIdStr, value] of Object.entries(organizerValues)) {
          const typeId = parseInt(typeIdStr);
          if (value.optionId || value.textValue?.trim()) {
            valuesToSave.push({
              organizer_type_id: typeId,
              organizer_option_id: value.optionId,
              text_value: value.textValue?.trim(),
            });
          }
        }

        if (valuesToSave.length > 0 && result.data.id) {
          await itemApi.setOrganizerValues(result.data.id, { values: valuesToSave });
        }

        showToast('Item added successfully!', 'success');
        setShowAddItemModal(false);
        setNewItem({
          inventory_id: parseInt(id ?? '0', 10),
          name: '',
          description: '',
          purchase_date: undefined,
          purchase_price: undefined,
          warranty_expiry: undefined,
          quantity: 1,
        });
        setOrganizerValues({});
        void loadInventoryDetail(parseInt(id ?? '0', 10));
      } else {
        showToast(result.error ?? 'Failed to add item', 'error');
      }
    } catch {
      showToast('Failed to add item', 'error');
    }
  };

  // Issue 1: View Details handler
  const handleViewItem = async (item: Item) => {
    setViewingItem(item);
    setViewingItemOrganizerValues([]);
    if (item.id) {
      try {
        const result = await itemApi.getOrganizerValues(item.id);
        if (result.success && result.data) {
          setViewingItemOrganizerValues(result.data);
        }
      } catch {
        // Organizer values are optional, proceed without them
      }
    }
  };

  // Issue 2: Edit Item handlers
  const handleOpenEditItem = async (item: Item) => {
    setEditingItem(item);
    setEditItemData({
      name: item.name,
      description: item.description ?? '',
      purchase_date: item.purchase_date ?? '',
      purchase_price: item.purchase_price,
      warranty_expiry: item.warranty_expiry ?? '',
      notes: item.notes ?? '',
      quantity: item.quantity ?? 1,
    });
    setEditOrganizerValues({});
    if (item.id) {
      try {
        const result = await itemApi.getOrganizerValues(item.id);
        if (result.success && result.data) {
          const values: Record<string, { optionId?: number; textValue?: string }> = {};
          for (const val of result.data) {
            values[String(val.organizer_type_id)] = {
              optionId: val.organizer_option_id,
              textValue: val.text_value,
            };
          }
          setEditOrganizerValues(values);
        }
      } catch {
        // Continue without pre-filled organizer values
      }
    }
    setShowEditItemModal(true);
  };

  const handleEditItem = async () => {
    if (!editingItem?.id || !editItemData.name?.trim()) {
      showToast('Please enter an item name', 'error');
      return;
    }

    // Check required organizers
    for (const org of organizers) {
      if (org.is_required && org.id) {
        const value = editOrganizerValues[String(org.id)];
        if (
          !value ||
          (org.input_type === 'select' && !value.optionId) ||
          (org.input_type === 'text' && !value.textValue?.trim()) ||
          (org.input_type === 'image' && !value.textValue?.trim())
        ) {
          showToast(`Please fill in the required field: ${org.name}`, 'error');
          return;
        }
      }
    }

    try {
      const result = await itemApi.update(editingItem.id, editItemData);
      if (result.success) {
        // Save organizer values
        const valuesToSave: SetItemOrganizerValueRequest[] = [];
        for (const [typeIdStr, value] of Object.entries(editOrganizerValues)) {
          const typeId = parseInt(typeIdStr);
          if (value.optionId || value.textValue?.trim()) {
            valuesToSave.push({
              organizer_type_id: typeId,
              organizer_option_id: value.optionId,
              text_value: value.textValue?.trim(),
            });
          }
        }
        if (valuesToSave.length > 0) {
          await itemApi.setOrganizerValues(editingItem.id, { values: valuesToSave });
        }

        showToast('Item updated successfully!', 'success');
        setShowEditItemModal(false);
        setEditingItem(null);
        void loadInventoryDetail(parseInt(id ?? '0', 10));
      } else {
        showToast(result.error ?? 'Failed to update item', 'error');
      }
    } catch {
      showToast('Failed to update item', 'error');
    }
  };

  // Issue 3: Delete handlers (replacing native confirm())
  const openDeleteItemModal = (item: Item) => {
    setDeletingItem(item);
    setShowDeleteItemModal(true);
  };

  const handleDeleteItem = async () => {
    if (!deletingItem?.id) {
      return;
    }

    try {
      const result = await itemApi.delete(deletingItem.id);
      if (result.success) {
        showToast('Item deleted successfully!', 'success');
        void loadInventoryDetail(parseInt(id ?? '0', 10));
      } else {
        showToast(result.error ?? 'Failed to delete item', 'error');
      }
    } catch {
      showToast('Failed to delete item', 'error');
    }
  };

  const totalValue = items.reduce(
    (sum, item) => sum + (item.purchase_price ?? 0) * (item.quantity ?? 1),
    0
  );

  if (loading) {
    return (
      <>
        <Header title="Loading..." subtitle="" icon="fas fa-warehouse" />
        <div className="content">
          <LoadingState message="Loading inventory details..." />
        </div>
      </>
    );
  }

  if (!inventory) {
    return null;
  }

  return (
    <>
      <Header
        title={inventory.name}
        subtitle={inventory.description ?? 'Manage and organize your inventory collections'}
        icon="fas fa-warehouse"
      />

      <div className="content">
        <div className="inventory-detail">
          <div className="inventory-actions">
            <button
              className="btn btn-ghost"
              onClick={() => navigate('/')}
              aria-label="Back to Inventories"
            >
              <i className="fas fa-arrow-left"></i>
              <span className="btn-label">Back to Inventories</span>
            </button>
            <div className="inventory-actions-right">
              <button
                className="btn btn-secondary"
                onClick={() => navigate(`/inventory/${id}/report`)}
                title="Report"
                aria-label="Report"
              >
                <i className="fas fa-chart-bar"></i>
                <span className="btn-label">Report</span>
              </button>
              <button
                className="btn btn-secondary"
                onClick={() => setShowShareModal(true)}
                title="Share"
                aria-label="Share"
              >
                <i className="fas fa-share-nodes"></i>
                <span className="btn-label">Share</span>
              </button>
              <button
                className="btn btn-secondary"
                onClick={() => navigate(`/inventory/${id}/organizers`)}
                title="Organizers"
                aria-label="Organizers"
              >
                <i className="fas fa-folder-tree"></i>
                <span className="btn-label">Organizers</span>
              </button>
              <button
                className="btn btn-primary"
                onClick={() => setShowAddItemModal(true)}
                title="Add Item"
                aria-label="Add Item"
              >
                <i className="fas fa-plus"></i>
                <span className="btn-label">Add Item</span>
              </button>
            </div>
          </div>

          <div className="stats-row">
            <div className="stat-card">
              <div className="stat-icon" style={{ background: 'var(--primary-color)' }}>
                <i className="fas fa-boxes"></i>
              </div>
              <div className="stat-content">
                <div className="stat-label">Total Items</div>
                <div className="stat-value">{items.length}</div>
              </div>
            </div>
            <div className="stat-card">
              <div className="stat-icon" style={{ background: 'var(--success-color)' }}>
                <i className="fas fa-coins"></i>
              </div>
              <div className="stat-content">
                <div className="stat-label">Total Value</div>
                <div className="stat-value">
                  {formatCurrency(totalValue, (settings?.currency ?? 'USD') as CurrencyType)}
                </div>
              </div>
            </div>
          </div>

          <div className="inventory-detail-items">
            <h2 className="section-title">
              <i className="fas fa-boxes"></i>
              Items in this Inventory
            </h2>

            {items.length === 0 ? (
              <EmptyState
                icon="fas fa-box-open"
                title="No Items Yet"
                text="Start adding items to this inventory."
              />
            ) : (
              <div className="items-grid">
                {items.map((item) => {
                  const itemValue = (item.purchase_price ?? 0) * (item.quantity ?? 1);
                  const notification = getItemNotification(item.id);
                  const thumbnailUrl = item.id ? itemImages[String(item.id)] : undefined;
                  return (
                    <div key={item.id} className="item-card">
                      {/* Conditionally render thumbnail only if inventory has image organizer AND has images */}
                      {shouldShowImageSection && (
                        <div
                          className="item-card-thumbnail"
                          {...(thumbnailUrl
                            ? {
                                onClick: () => {
                                  setLightboxUrl(thumbnailUrl);
                                  setLightboxAlt(item.name);
                                  setLightboxOpen(true);
                                },
                                role: 'button',
                                tabIndex: 0,
                                'aria-label': `View photo of ${item.name}`,
                                onKeyDown: (e: React.KeyboardEvent) => {
                                  if (e.key === 'Enter' || e.key === ' ') {
                                    e.preventDefault();
                                    setLightboxUrl(thumbnailUrl);
                                    setLightboxAlt(item.name);
                                    setLightboxOpen(true);
                                  }
                                },
                              }
                            : {})}
                        >
                          {thumbnailUrl ? (
                            <img src={thumbnailUrl} alt={item.name} loading="lazy" />
                          ) : (
                            <div className="item-card-placeholder" aria-label="No image available">
                              <i className="fas fa-box-open" aria-hidden="true"></i>
                            </div>
                          )}
                        </div>
                      )}
                      <div className="item-card-header">
                        <h3 className="item-card-title">{item.name}</h3>
                        {item.category && (
                          <span className="item-card-category">{item.category}</span>
                        )}
                      </div>
                      <div className="item-card-body">
                        {item.description && (
                          <p className="item-card-description">{item.description}</p>
                        )}
                        <div className="item-card-details">
                          {item.location && (
                            <div className="detail-item">
                              <i className="fas fa-map-marker-alt"></i>
                              <span>{item.location}</span>
                            </div>
                          )}
                          {item.quantity && (
                            <div className="detail-item">
                              <i className="fas fa-boxes"></i>
                              <span>Qty: {item.quantity}</span>
                            </div>
                          )}
                          {item.purchase_date && (
                            <div className="detail-item">
                              <i className="fas fa-calendar-alt"></i>
                              <span>
                                Purchased:{' '}
                                {formatDate(
                                  item.purchase_date,
                                  (settings?.date_format ?? 'MM/DD/YYYY') as DateFormatType
                                )}
                              </span>
                            </div>
                          )}
                          {item.purchase_price && (
                            <div className="detail-item">
                              <i className="fas fa-tag"></i>
                              <span>
                                {formatCurrency(
                                  item.purchase_price,
                                  (settings?.currency ?? 'USD') as CurrencyType
                                )}{' '}
                                ea
                              </span>
                            </div>
                          )}
                          {itemValue > 0 && (
                            <div className="detail-item">
                              <i className="fas fa-coins"></i>
                              <span>
                                Total:{' '}
                                {formatCurrency(
                                  itemValue,
                                  (settings?.currency ?? 'USD') as CurrencyType
                                )}
                              </span>
                            </div>
                          )}
                          {item.warranty_expiry && (
                            <div className="detail-item">
                              <i className="fas fa-shield-alt"></i>
                              <span>
                                Warranty:{' '}
                                {formatDate(
                                  item.warranty_expiry,
                                  (settings?.date_format ?? 'MM/DD/YYYY') as DateFormatType
                                )}
                              </span>
                            </div>
                          )}
                        </div>
                      </div>
                      <div className="item-card-footer">
                        <button
                          className="btn btn-sm btn-ghost"
                          onClick={() => void handleViewItem(item)}
                          title="View Details"
                        >
                          <i className="fas fa-eye"></i>
                        </button>
                        <button
                          className="btn btn-sm btn-ghost"
                          onClick={() => void handleOpenEditItem(item)}
                          title="Edit Item"
                        >
                          <i className="fas fa-edit"></i>
                        </button>
                        <button
                          className="btn btn-sm btn-ghost text-danger"
                          onClick={() => openDeleteItemModal(item)}
                          title="Delete Item"
                        >
                          <i className="fas fa-trash"></i>
                        </button>
                        {/* Enhancement 2: Notification Badge - moved to footer */}
                        {notification &&
                          (() => {
                            const { status, daysUntilExpiry } = notification;
                            const statusClass = `status-${status}`;
                            const icon =
                              status === 'expired'
                                ? 'fa-exclamation-circle'
                                : status === 'expiring-soon'
                                  ? 'fa-exclamation-triangle'
                                  : 'fa-info-circle';

                            const text = status === 'expired' ? 'Expired' : `${daysUntilExpiry}d`;

                            return (
                              <span
                                className={`item-notification-badge ${statusClass}`}
                                title={getNotificationMessage(notification)}
                                aria-label={`Warranty notification: ${getNotificationMessage(notification)}`}
                              >
                                <i className={`fas ${icon}`}></i>
                                {text}
                              </span>
                            );
                          })()}
                      </div>
                    </div>
                  );
                })}
              </div>
            )}
          </div>
        </div>
      </div>

      <Modal
        isOpen={showAddItemModal}
        onClose={() => setShowAddItemModal(false)}
        title="Add New Item"
        subtitle="Quickly add an item to your inventory"
        footer={
          <>
            <button className="btn btn-secondary" onClick={() => setShowAddItemModal(false)}>
              Cancel
            </button>
            <button className="btn btn-primary" onClick={handleAddItem}>
              <i className="fas fa-plus"></i>
              Add Item
            </button>
          </>
        }
      >
        <div className="form-group">
          <label className="form-label" htmlFor="item-name">
            Item Name *
          </label>
          <input
            type="text"
            className="form-input"
            id="item-name"
            placeholder="Enter item name"
            value={newItem.name}
            onChange={(e) => setNewItem({ ...newItem, name: e.target.value })}
          />
        </div>

        {/* Dynamic Organizer Fields */}
        {organizers.length > 0 && (
          <div className="organizer-fields">
            {organizers.map(
              (org) =>
                org.id && (
                  <div className="form-group" key={org.id}>
                    <label className="form-label" htmlFor={`organizer-${org.id}`}>
                      {org.name}
                      {org.is_required ? ' *' : ''}
                    </label>
                    {org.input_type === 'select' ? (
                      <select
                        className="form-select"
                        id={`organizer-${org.id}`}
                        value={organizerValues[String(org.id)]?.optionId ?? ''}
                        onChange={(e) =>
                          setOrganizerValues({
                            ...organizerValues,
                            [String(org.id)]: {
                              optionId: e.target.value ? parseInt(e.target.value, 10) : undefined,
                            },
                          })
                        }
                      >
                        <option value="">Select {org.name.toLowerCase()}</option>
                        {org.options.map((opt) => (
                          <option key={opt.id} value={opt.id}>
                            {opt.name}
                          </option>
                        ))}
                      </select>
                    ) : org.input_type === 'image' ? (
                      <ImageOrganizerInput
                        value={organizerValues[String(org.id)]?.textValue ?? undefined}
                        onChange={(url) =>
                          setOrganizerValues({
                            ...organizerValues,
                            [String(org.id)]: { textValue: url },
                          })
                        }
                        itemName={newItem.name || 'item'}
                        required={org.is_required}
                      />
                    ) : (
                      <input
                        type="text"
                        className="form-input"
                        id={`organizer-${org.id}`}
                        placeholder={`Enter ${org.name.toLowerCase()}`}
                        value={organizerValues[String(org.id)]?.textValue ?? ''}
                        onChange={(e) =>
                          setOrganizerValues({
                            ...organizerValues,
                            [String(org.id)]: { textValue: e.target.value },
                          })
                        }
                      />
                    )}
                  </div>
                )
            )}
          </div>
        )}

        <div className="form-row">
          <div className="form-group">
            <label className="form-label" htmlFor="item-price">
              Purchase Price
            </label>
            <input
              type="number"
              className="form-input"
              id="item-price"
              placeholder="0.00"
              step="0.01"
              min="0"
              value={newItem.purchase_price ?? ''}
              onChange={(e) =>
                setNewItem({
                  ...newItem,
                  purchase_price: e.target.value ? parseFloat(e.target.value) : undefined,
                })
              }
            />
          </div>

          <div className="form-group">
            <label className="form-label" htmlFor="item-quantity">
              Quantity
            </label>
            <input
              type="number"
              className="form-input"
              id="item-quantity"
              placeholder="1"
              min="1"
              value={newItem.quantity ?? 1}
              onChange={(e) =>
                setNewItem({ ...newItem, quantity: parseInt(e.target.value, 10) || 1 })
              }
            />
          </div>
        </div>

        <div className="form-row">
          <div className="form-group">
            <label className="form-label" htmlFor="item-purchase-date">
              Purchase Date
            </label>
            <input
              type="date"
              className="form-input"
              id="item-purchase-date"
              value={newItem.purchase_date ?? ''}
              onChange={(e) => setNewItem({ ...newItem, purchase_date: e.target.value })}
            />
          </div>

          <div className="form-group">
            <label className="form-label" htmlFor="item-warranty">
              Warranty Expiry
            </label>
            <input
              type="date"
              className="form-input"
              id="item-warranty"
              value={newItem.warranty_expiry ?? ''}
              onChange={(e) => setNewItem({ ...newItem, warranty_expiry: e.target.value })}
            />
          </div>
        </div>

        <div className="form-group">
          <label className="form-label" htmlFor="item-description">
            Description
          </label>
          <textarea
            className="form-input"
            id="item-description"
            placeholder="Optional description"
            rows={3}
            value={newItem.description}
            onChange={(e) => setNewItem({ ...newItem, description: e.target.value })}
          />
        </div>
      </Modal>

      {/* View Details Modal (Issue 1) */}
      <Modal
        isOpen={viewingItem !== null}
        onClose={() => {
          setViewingItem(null);
          setViewingItemOrganizerValues([]);
        }}
        title={viewingItem?.name ?? 'Item Details'}
        subtitle={viewingItem?.category ? `Category: ${viewingItem.category}` : undefined}
        maxWidth="600px"
        footer={
          <button
            className="btn btn-secondary"
            onClick={() => {
              setViewingItem(null);
              setViewingItemOrganizerValues([]);
            }}
          >
            Close
          </button>
        }
      >
        {viewingItem && (
          <div className="item-details-view">
            {viewingItem.description && (
              <div className="form-group">
                <label className="form-label">Description</label>
                <p style={{ margin: 0, color: 'var(--text-secondary)' }}>
                  {viewingItem.description}
                </p>
              </div>
            )}
            {viewingItem.location && (
              <div className="form-group">
                <label className="form-label">
                  <i className="fas fa-map-marker-alt" style={{ marginRight: '0.5rem' }}></i>
                  Location
                </label>
                <p style={{ margin: 0, color: 'var(--text-secondary)' }}>{viewingItem.location}</p>
              </div>
            )}
            <div className="form-row">
              <div className="form-group">
                <label className="form-label">
                  <i className="fas fa-boxes" style={{ marginRight: '0.5rem' }}></i>
                  Quantity
                </label>
                <p style={{ margin: 0, color: 'var(--text-secondary)' }}>
                  {viewingItem.quantity ?? 1}
                </p>
              </div>
              {viewingItem.purchase_price !== undefined && (
                <div className="form-group">
                  <label className="form-label">
                    <i className="fas fa-tag" style={{ marginRight: '0.5rem' }}></i>
                    Purchase Price
                  </label>
                  <p style={{ margin: 0, color: 'var(--text-secondary)' }}>
                    {formatCurrency(
                      viewingItem.purchase_price,
                      (settings?.currency ?? 'USD') as CurrencyType
                    )}
                  </p>
                </div>
              )}
            </div>
            {viewingItem.purchase_price !== undefined && (viewingItem.quantity ?? 1) > 1 && (
              <div className="form-group">
                <label className="form-label">
                  <i className="fas fa-coins" style={{ marginRight: '0.5rem' }}></i>
                  Total Value
                </label>
                <p style={{ margin: 0, color: 'var(--text-secondary)' }}>
                  {formatCurrency(
                    viewingItem.purchase_price * (viewingItem.quantity ?? 1),
                    (settings?.currency ?? 'USD') as CurrencyType
                  )}
                </p>
              </div>
            )}
            <div className="form-row">
              {viewingItem.purchase_date && (
                <div className="form-group">
                  <label className="form-label">
                    <i className="fas fa-calendar-alt" style={{ marginRight: '0.5rem' }}></i>
                    Purchase Date
                  </label>
                  <p style={{ margin: 0, color: 'var(--text-secondary)' }}>
                    {formatDate(
                      viewingItem.purchase_date,
                      (settings?.date_format ?? 'MM/DD/YYYY') as DateFormatType
                    )}
                  </p>
                </div>
              )}
              {viewingItem.warranty_expiry && (
                <div className="form-group">
                  <label className="form-label">
                    <i className="fas fa-shield-alt" style={{ marginRight: '0.5rem' }}></i>
                    Warranty Expiry
                  </label>
                  <p style={{ margin: 0, color: 'var(--text-secondary)' }}>
                    {formatDate(
                      viewingItem.warranty_expiry,
                      (settings?.date_format ?? 'MM/DD/YYYY') as DateFormatType
                    )}
                    {new Date(viewingItem.warranty_expiry) < new Date()
                      ? ' (Expired)'
                      : ' (Active)'}
                  </p>
                </div>
              )}
            </div>
            {viewingItem.notes && (
              <div className="form-group">
                <label className="form-label">
                  <i className="fas fa-sticky-note" style={{ marginRight: '0.5rem' }}></i>
                  Notes
                </label>
                <p style={{ margin: 0, color: 'var(--text-secondary)', whiteSpace: 'pre-wrap' }}>
                  {viewingItem.notes}
                </p>
              </div>
            )}
            {viewingItemOrganizerValues.length > 0 && (
              <div className="form-group">
                <label className="form-label" style={{ marginBottom: '0.5rem' }}>
                  <i className="fas fa-folder-tree" style={{ marginRight: '0.5rem' }}></i>
                  Organizer Assignments
                </label>
                {viewingItemOrganizerValues.map((val) => (
                  <div
                    key={val.organizer_type_id}
                    style={{
                      display: 'flex',
                      flexDirection:
                        val.input_type === 'image' ? ('column' as const) : ('row' as const),
                      gap: '0.5rem',
                      marginBottom: '0.5rem',
                    }}
                  >
                    <span style={{ fontWeight: 500, color: 'var(--text-primary)' }}>
                      {val.organizer_type_name}:
                    </span>
                    {val.input_type === 'image' && val.text_value ? (
                      (() => {
                        const imageUrl = val.text_value;
                        return (
                          <img
                            src={imageUrl}
                            alt={`${val.organizer_type_name} for ${viewingItem.name}`}
                            className="organizer-image-preview"
                            onClick={() => {
                              setLightboxUrl(imageUrl);
                              setLightboxAlt(`${val.organizer_type_name} — ${viewingItem.name}`);
                              setLightboxOpen(true);
                            }}
                            role="button"
                            tabIndex={0}
                            onKeyDown={(e) => {
                              if (e.key === 'Enter' || e.key === ' ') {
                                e.preventDefault();
                                setLightboxUrl(imageUrl);
                                setLightboxAlt(`${val.organizer_type_name} — ${viewingItem.name}`);
                                setLightboxOpen(true);
                              }
                            }}
                          />
                        );
                      })()
                    ) : (
                      <span style={{ color: 'var(--text-secondary)' }}>
                        {val.value ?? val.text_value ?? '—'}
                      </span>
                    )}
                  </div>
                ))}
              </div>
            )}
            <div className="form-row">
              {viewingItem.created_at && (
                <div className="form-group">
                  <label
                    className="form-label"
                    style={{ fontSize: '0.8rem', color: 'var(--text-muted)' }}
                  >
                    Created
                  </label>
                  <p style={{ margin: 0, fontSize: '0.85rem', color: 'var(--text-muted)' }}>
                    {formatDate(
                      viewingItem.created_at,
                      (settings?.date_format ?? 'MM/DD/YYYY') as DateFormatType
                    )}
                  </p>
                </div>
              )}
              {viewingItem.updated_at && (
                <div className="form-group">
                  <label
                    className="form-label"
                    style={{ fontSize: '0.8rem', color: 'var(--text-muted)' }}
                  >
                    Updated
                  </label>
                  <p style={{ margin: 0, fontSize: '0.85rem', color: 'var(--text-muted)' }}>
                    {formatDate(
                      viewingItem.updated_at,
                      (settings?.date_format ?? 'MM/DD/YYYY') as DateFormatType
                    )}
                  </p>
                </div>
              )}
            </div>
          </div>
        )}
      </Modal>

      {/* Edit Item Modal (Issue 2) */}
      <Modal
        isOpen={showEditItemModal}
        onClose={() => {
          setShowEditItemModal(false);
          setEditingItem(null);
        }}
        title="Edit Item"
        subtitle="Update item details"
        footer={
          <>
            <button
              className="btn btn-secondary"
              onClick={() => {
                setShowEditItemModal(false);
                setEditingItem(null);
              }}
            >
              Cancel
            </button>
            <button className="btn btn-primary" onClick={handleEditItem}>
              <i className="fas fa-save"></i>
              Save Changes
            </button>
          </>
        }
      >
        <div className="form-group">
          <label className="form-label" htmlFor="edit-item-name">
            Item Name *
          </label>
          <input
            type="text"
            className="form-input"
            id="edit-item-name"
            placeholder="Enter item name"
            value={editItemData.name ?? ''}
            onChange={(e) => setEditItemData({ ...editItemData, name: e.target.value })}
          />
        </div>

        {/* Dynamic Organizer Fields */}
        {organizers.length > 0 && (
          <div className="organizer-fields">
            {organizers.map(
              (org) =>
                org.id && (
                  <div className="form-group" key={org.id}>
                    <label className="form-label" htmlFor={`edit-organizer-${org.id}`}>
                      {org.name}
                      {org.is_required ? ' *' : ''}
                    </label>
                    {org.input_type === 'select' ? (
                      <select
                        className="form-select"
                        id={`edit-organizer-${org.id}`}
                        value={editOrganizerValues[String(org.id)]?.optionId ?? ''}
                        onChange={(e) =>
                          setEditOrganizerValues({
                            ...editOrganizerValues,
                            [String(org.id)]: {
                              optionId: e.target.value ? parseInt(e.target.value, 10) : undefined,
                            },
                          })
                        }
                      >
                        <option value="">Select {org.name.toLowerCase()}</option>
                        {org.options.map((opt) => (
                          <option key={opt.id} value={opt.id}>
                            {opt.name}
                          </option>
                        ))}
                      </select>
                    ) : org.input_type === 'image' ? (
                      <ImageOrganizerInput
                        value={editOrganizerValues[String(org.id)]?.textValue ?? undefined}
                        onChange={(url) =>
                          setEditOrganizerValues({
                            ...editOrganizerValues,
                            [String(org.id)]: { textValue: url },
                          })
                        }
                        itemName={editItemData.name ?? 'item'}
                        required={org.is_required}
                      />
                    ) : (
                      <input
                        type="text"
                        className="form-input"
                        id={`edit-organizer-${org.id}`}
                        placeholder={`Enter ${org.name.toLowerCase()}`}
                        value={editOrganizerValues[String(org.id)]?.textValue ?? ''}
                        onChange={(e) =>
                          setEditOrganizerValues({
                            ...editOrganizerValues,
                            [String(org.id)]: { textValue: e.target.value },
                          })
                        }
                      />
                    )}
                  </div>
                )
            )}
          </div>
        )}

        <div className="form-row">
          <div className="form-group">
            <label className="form-label" htmlFor="edit-item-price">
              Purchase Price
            </label>
            <input
              type="number"
              className="form-input"
              id="edit-item-price"
              placeholder="0.00"
              step="0.01"
              min="0"
              value={editItemData.purchase_price ?? ''}
              onChange={(e) =>
                setEditItemData({
                  ...editItemData,
                  purchase_price: e.target.value ? parseFloat(e.target.value) : undefined,
                })
              }
            />
          </div>

          <div className="form-group">
            <label className="form-label" htmlFor="edit-item-quantity">
              Quantity
            </label>
            <input
              type="number"
              className="form-input"
              id="edit-item-quantity"
              placeholder="1"
              min="1"
              value={editItemData.quantity ?? 1}
              onChange={(e) =>
                setEditItemData({ ...editItemData, quantity: parseInt(e.target.value, 10) || 1 })
              }
            />
          </div>
        </div>

        <div className="form-row">
          <div className="form-group">
            <label className="form-label" htmlFor="edit-item-purchase-date">
              Purchase Date
            </label>
            <input
              type="date"
              className="form-input"
              id="edit-item-purchase-date"
              value={editItemData.purchase_date ?? ''}
              onChange={(e) => setEditItemData({ ...editItemData, purchase_date: e.target.value })}
            />
          </div>

          <div className="form-group">
            <label className="form-label" htmlFor="edit-item-warranty">
              Warranty Expiry
            </label>
            <input
              type="date"
              className="form-input"
              id="edit-item-warranty"
              value={editItemData.warranty_expiry ?? ''}
              onChange={(e) =>
                setEditItemData({ ...editItemData, warranty_expiry: e.target.value })
              }
            />
          </div>
        </div>

        <div className="form-group">
          <label className="form-label" htmlFor="edit-item-description">
            Description
          </label>
          <textarea
            className="form-input"
            id="edit-item-description"
            placeholder="Optional description"
            rows={3}
            value={editItemData.description ?? ''}
            onChange={(e) => setEditItemData({ ...editItemData, description: e.target.value })}
          />
        </div>

        <div className="form-group">
          <label className="form-label" htmlFor="edit-item-notes">
            Notes
          </label>
          <textarea
            className="form-input"
            id="edit-item-notes"
            placeholder="Optional notes"
            rows={3}
            value={editItemData.notes ?? ''}
            onChange={(e) => setEditItemData({ ...editItemData, notes: e.target.value })}
          />
        </div>
      </Modal>

      {/* Delete Confirmation Modal (Issue 3) */}
      <ConfirmModal
        isOpen={showDeleteItemModal}
        onClose={() => {
          setShowDeleteItemModal(false);
          setDeletingItem(null);
        }}
        onConfirm={handleDeleteItem}
        title="Delete Item"
        message={`Are you sure you want to delete "${deletingItem?.name}"? This action cannot be undone.`}
        confirmText="Delete"
        confirmButtonClass="btn-danger"
        icon="fas fa-trash"
      />

      <ShareInventoryModal
        isOpen={showShareModal}
        onClose={() => setShowShareModal(false)}
        inventoryId={parseInt(id ?? '0', 10)}
        inventoryName={inventory.name}
      />

      <ImageLightbox
        isOpen={lightboxOpen}
        onClose={() => setLightboxOpen(false)}
        imageUrl={lightboxUrl}
        altText={lightboxAlt}
      />
    </>
  );
}
