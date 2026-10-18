import { useMemo, useState, useEffect } from 'react';
import { useNavigate } from 'react-router-dom';
import { Header, EmptyState, LoadingState } from '@/components';
import { useApp } from '@/context/AppContext';
import { useAuth } from '@/context/AuthContext';
import { formatDateOnly, type DateFormatType } from '@/utils/dateFormat';
import { getNotificationMessage } from '@/utils/notifications';
import type { WarrantyNotification } from '@/utils/notifications';
import '@/styles/notifications.css';

export function NotificationsPage() {
  const navigate = useNavigate();
  const { warrantyNotifications, inventories } = useApp();
  const { settings, dismissNotification, clearAllDismissals } = useAuth();
  const { showToast } = useApp();
  const [loading, setLoading] = useState(true);

  // Track when initial data has been loaded from AppContext
  useEffect(() => {
    // Data is loaded when inventories have been fetched (even if empty)
    // We use a short timeout to allow AppContext to populate on initial mount
    const timer = setTimeout(() => {
      setLoading(false);
    }, 100);

    return () => clearTimeout(timer);
  }, []);

  // RECOMMENDED FIX: Removed redundant filtering - AppContext now filters at source
  // warrantyNotifications already excludes dismissed notifications
  const activeNotifications = warrantyNotifications;

  const getInventoryName = (inventoryId: number): string => {
    const inv = inventories.find((i) => i.id === inventoryId);
    return inv?.name ?? 'Unknown Inventory';
  };

  const { expired, expiringSoon, expiringThisMonth } = useMemo(
    () => ({
      expired: activeNotifications.filter((n) => n.status === 'expired'),
      expiringSoon: activeNotifications.filter((n) => n.status === 'expiring-soon'),
      expiringThisMonth: activeNotifications.filter((n) => n.status === 'expiring-this-month'),
    }),
    [activeNotifications]
  );

  const handleNotificationClick = (notification: WarrantyNotification) => {
    navigate(`/inventory/${notification.inventoryId}`, {
      state: { openItemId: notification.id },
    });
  };

  // Enhancement 3: Dismiss handlers
  const handleDismissNotification = async (
    notification: WarrantyNotification,
    e: React.MouseEvent
  ) => {
    e.stopPropagation(); // Prevent card click
    const success = await dismissNotification(notification.id, notification.warrantyExpiry);
    if (success) {
      showToast('Notification dismissed', 'success');
    } else {
      showToast('Failed to dismiss notification', 'error');
    }
  };

  const handleClearAll = async () => {
    if (activeNotifications.length === 0) {
      return;
    }

    // Confirm with user using native confirm (for simplicity in this context)
    // In production, replace with a custom modal component
    // eslint-disable-next-line no-alert
    if (!window.confirm(`Clear all ${activeNotifications.length} notifications?`)) {
      return;
    }

    const success = await clearAllDismissals();
    if (success) {
      showToast('All notifications cleared', 'success');
    } else {
      showToast('Failed to clear notifications', 'error');
    }
  };

  const dateFormat = (settings?.date_format ?? 'MM/DD/YYYY') as DateFormatType;

  const formatDate = (dateStr: string): string => {
    return formatDateOnly(dateStr, dateFormat);
  };

  const getStatusIcon = (status: WarrantyNotification['status']): string => {
    switch (status) {
      case 'expired':
        return 'fas fa-exclamation-circle';
      case 'expiring-soon':
        return 'fas fa-exclamation-triangle';
      case 'expiring-this-month':
        return 'fas fa-info-circle';
    }
  };

  const renderNotificationCard = (notification: WarrantyNotification) => {
    const statusClass = `status-${notification.status}`;

    return (
      <div
        key={notification.id}
        className={`notification-card ${statusClass}`}
        onClick={() => handleNotificationClick(notification)}
        onKeyDown={(e) => {
          if (e.key === 'Enter' || e.key === ' ') {
            e.preventDefault();
            handleNotificationClick(notification);
          }
        }}
        role="button"
        tabIndex={0}
        aria-label={`${notification.itemName} - ${getNotificationMessage(notification)}`}
      >
        {/* Dismiss button */}
        <button
          className="notification-dismiss"
          onClick={(e) => void handleDismissNotification(notification, e)}
          title="Dismiss notification"
          aria-label="Dismiss notification"
        >
          <i className="fas fa-times"></i>
        </button>

        {/* Icon */}
        <div className="notification-icon">
          <i className={getStatusIcon(notification.status)}></i>
        </div>

        {/* Content */}
        <div className="notification-content">
          <div className="notification-title">{notification.itemName}</div>
          <div className="notification-inventory">{getInventoryName(notification.inventoryId)}</div>
          <div className="notification-message">{getNotificationMessage(notification)}</div>
        </div>

        {/* Meta (date + chevron) */}
        <div className="notification-meta">
          <div className="notification-date">{formatDate(notification.warrantyExpiry)}</div>
          <i className="fas fa-chevron-right notification-chevron"></i>
        </div>
      </div>
    );
  };

  const renderSection = (
    title: string,
    icon: string,
    color: string,
    notifications: WarrantyNotification[]
  ) => {
    if (notifications.length === 0) {
      return null;
    }

    return (
      <div className="notification-section">
        <div className="notification-section-header" style={{ color }}>
          <i className={icon}></i>
          <h3>
            {title}
            <span className="notification-section-count">({notifications.length})</span>
          </h3>
        </div>
        {notifications.map(renderNotificationCard)}
      </div>
    );
  };

  // Disabled state
  if (!settings?.notifications_enabled) {
    return (
      <>
        <Header title="Notifications" subtitle="Warranty alerts and reminders" icon="fas fa-bell" />
        <div className="content">
          <EmptyState
            icon="fas fa-bell-slash"
            title="Notifications are disabled"
            text="Enable notifications in Settings to receive warranty alerts."
            action={
              <button className="btn btn-primary" onClick={() => navigate('/settings')}>
                <i className="fas fa-cog"></i>
                Go to Settings
              </button>
            }
          />
        </div>
      </>
    );
  }

  // Loading state
  if (loading) {
    return (
      <>
        <Header title="Notifications" subtitle="Warranty alerts and reminders" icon="fas fa-bell" />
        <div className="content">
          <LoadingState message="Loading notifications..." />
        </div>
      </>
    );
  }

  // Empty state - No notifications after loading is complete
  if (activeNotifications.length === 0) {
    return (
      <>
        <Header title="Notifications" subtitle="Warranty alerts and reminders" icon="fas fa-bell" />
        <div className="content">
          <EmptyState
            icon="fas fa-check-circle"
            title="No warranty alerts"
            text="All your warranties are up to date!"
          />
        </div>
      </>
    );
  }

  return (
    <>
      <Header title="Notifications" subtitle="Warranty alerts and reminders" icon="fas fa-bell" />

      <div className="content">
        {/* Enhancement 3: Header actions bar */}
        <div className="notifications-header">
          {activeNotifications.length > 0 && (
            <button className="btn btn-primary btn-sm" onClick={handleClearAll}>
              <i className="fas fa-check-double"></i>
              Clear All
            </button>
          )}
        </div>

        {/* Summary bar */}
        <div className="notifications-summary">
          <div className="notification-stat-card">
            <i className="fas fa-bell" style={{ color: 'var(--warning-color)' }}></i>
            <span className="stat-value">{activeNotifications.length}</span>
            <span className="stat-label">Total Alerts</span>
          </div>

          {expired.length > 0 && (
            <div className="notification-stat-card">
              <i className="fas fa-exclamation-circle" style={{ color: 'var(--danger-color)' }}></i>
              <span className="stat-value">{expired.length}</span>
              <span className="stat-label">Expired</span>
            </div>
          )}

          {expiringSoon.length > 0 && (
            <div className="notification-stat-card">
              <i
                className="fas fa-exclamation-triangle"
                style={{ color: 'var(--warning-color)' }}
              ></i>
              <span className="stat-value">{expiringSoon.length}</span>
              <span className="stat-label">Expiring Soon</span>
            </div>
          )}

          {expiringThisMonth.length > 0 && (
            <div className="notification-stat-card">
              <i className="fas fa-info-circle" style={{ color: 'var(--info-color)' }}></i>
              <span className="stat-value">{expiringThisMonth.length}</span>
              <span className="stat-label">This Month</span>
            </div>
          )}
        </div>

        {/* Notification sections */}
        {renderSection(
          'Expired Warranties',
          'fas fa-exclamation-circle',
          'var(--danger-color)',
          expired
        )}
        {renderSection(
          'Expiring Soon',
          'fas fa-exclamation-triangle',
          'var(--warning-color)',
          expiringSoon
        )}
        {renderSection(
          'Expiring This Month',
          'fas fa-info-circle',
          'var(--info-color)',
          expiringThisMonth
        )}
      </div>
    </>
  );
}
