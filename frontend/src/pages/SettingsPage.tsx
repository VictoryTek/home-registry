import { useState, useEffect } from 'react';
import {
  Header,
  UserManagement,
  AllAccessManagement,
  RecoveryCodesSection,
  TotpSettings,
  BackupRestoreSection,
} from '@/components';
import { useApp } from '@/context/AppContext';
import { useAuth } from '@/context/AuthContext';
import type { Inventory } from '@/types';
import { inventoryApi } from '@/services/api';

const DATE_FORMAT_OPTIONS = [
  { value: 'MM/DD/YYYY', label: 'MM/DD/YYYY (US)' },
  { value: 'DD/MM/YYYY', label: 'DD/MM/YYYY (EU)' },
  { value: 'YYYY-MM-DD', label: 'YYYY-MM-DD (ISO)' },
  { value: 'DD.MM.YYYY', label: 'DD.MM.YYYY (German)' },
];

const CURRENCY_OPTIONS = [
  { value: 'USD', label: 'USD ($)', symbol: '$' },
  { value: 'EUR', label: 'EUR (€)', symbol: '€' },
  { value: 'GBP', label: 'GBP (£)', symbol: '£' },
  { value: 'CAD', label: 'CAD ($)', symbol: 'C$' },
  { value: 'AUD', label: 'AUD ($)', symbol: 'A$' },
  { value: 'JPY', label: 'JPY (¥)', symbol: '¥' },
];

export function SettingsPage() {
  const { showToast } = useApp();
  const { settings, updateSettings, token, user } = useAuth();
  const [inventories, setInventories] = useState<Inventory[]>([]);
  const [isLoading, setIsLoading] = useState(false);
  const [isSaving, setIsSaving] = useState(false);

  // BUG FIX: Use 0 as sentinel value for "no default inventory" instead of undefined.
  // undefined gets omitted from JSON serialization, preventing backend from clearing the setting.
  const [form, setForm] = useState({
    date_format: 'MM/DD/YYYY',
    currency: 'USD',
    default_inventory_id: 0 as number,
    notifications_enabled: true,
  });

  // Load inventories for default inventory selector
  useEffect(() => {
    const loadInventories = async () => {
      if (!token) {
        return;
      }
      setIsLoading(true);
      try {
        const result = await inventoryApi.getAll();
        if (result.success && result.data) {
          setInventories(result.data);
        }
      } catch (error) {
        console.error('Error loading inventories:', error);
      } finally {
        setIsLoading(false);
      }
    };
    void loadInventories();
  }, [token]);

  // Sync form with settings when loaded
  useEffect(() => {
    if (settings) {
      setForm({
        date_format: settings.date_format || 'MM/DD/YYYY',
        currency: settings.currency || 'USD',
        // Convert null/undefined from backend to 0 (our sentinel value)
        default_inventory_id: settings.default_inventory_id ?? 0,
        notifications_enabled: settings.notifications_enabled,
      });
    }
  }, [settings]);

  const handleSave = async () => {
    setIsSaving(true);
    try {
      // Send form.default_inventory_id directly - backend interprets 0 as "clear setting"
      const success = await updateSettings({
        date_format: form.date_format,
        currency: form.currency,
        default_inventory_id: form.default_inventory_id,
        notifications_enabled: form.notifications_enabled,
      });

      if (success) {
        showToast('Settings saved successfully', 'success');
      } else {
        showToast('Failed to save settings', 'error');
      }
    } catch (error) {
      console.error('Error saving settings:', error);
      showToast('An error occurred while saving', 'error');
    } finally {
      setIsSaving(false);
    }
  };

  const hasChanges =
    settings &&
    (form.date_format !== (settings.date_format || 'MM/DD/YYYY') ||
      form.currency !== (settings.currency || 'USD') ||
      // Compare with 0 as fallback to handle null/undefined from backend
      form.default_inventory_id !== (settings.default_inventory_id ?? 0) ||
      form.notifications_enabled !== settings.notifications_enabled);

  return (
    <>
      <Header
        title="Settings"
        subtitle="Configure your application preferences"
        icon="fas fa-cog"
      />

      <div className="content">
        <div className="settings-container">
          {/* Display Preferences */}
          <section className="settings-section">
            <div className="settings-section-header">
              <div className="settings-section-icon">
                <i className="fas fa-palette"></i>
              </div>
              <div>
                <h2 className="settings-section-title">Display Preferences</h2>
                <p className="settings-section-description">
                  Customize how information is displayed throughout the app
                </p>
              </div>
            </div>

            <div className="settings-group">
              <div className="setting-item">
                <div className="setting-info">
                  <label htmlFor="date_format" className="setting-label">
                    Date Format
                  </label>
                  <p className="setting-description">Choose how dates are displayed</p>
                </div>
                <select
                  id="date_format"
                  className="setting-select"
                  value={form.date_format}
                  onChange={(e) => setForm((prev) => ({ ...prev, date_format: e.target.value }))}
                >
                  {DATE_FORMAT_OPTIONS.map((opt) => (
                    <option key={opt.value} value={opt.value}>
                      {opt.label}
                    </option>
                  ))}
                </select>
              </div>

              <div className="setting-item">
                <div className="setting-info">
                  <label htmlFor="currency" className="setting-label">
                    Currency
                  </label>
                  <p className="setting-description">Default currency for prices</p>
                </div>
                <select
                  id="currency"
                  className="setting-select"
                  value={form.currency}
                  onChange={(e) => setForm((prev) => ({ ...prev, currency: e.target.value }))}
                >
                  {CURRENCY_OPTIONS.map((opt) => (
                    <option key={opt.value} value={opt.value}>
                      {opt.label}
                    </option>
                  ))}
                </select>
              </div>
            </div>
          </section>

          {/* Default Settings */}
          <section className="settings-section">
            <div className="settings-section-header">
              <div className="settings-section-icon">
                <i className="fas fa-home"></i>
              </div>
              <div>
                <h2 className="settings-section-title">Default Settings</h2>
                <p className="settings-section-description">
                  Set default values for the application
                </p>
              </div>
            </div>

            <div className="settings-group">
              <div className="setting-item">
                <div className="setting-info">
                  <label htmlFor="default_inventory" className="setting-label">
                    Default Inventory
                  </label>
                  <p className="setting-description">Inventory to show when opening the app</p>
                </div>
                <select
                  id="default_inventory"
                  className="setting-select"
                  value={form.default_inventory_id || ''}
                  onChange={(e) =>
                    setForm((prev) => ({
                      ...prev,
                      // Use 0 as sentinel value for "none" instead of undefined
                      default_inventory_id: e.target.value ? Number(e.target.value) : 0,
                    }))
                  }
                  disabled={isLoading}
                >
                  <option value="">None (show all inventories)</option>
                  {inventories.map((inv) => (
                    <option key={inv.id} value={inv.id}>
                      {inv.name}
                    </option>
                  ))}
                </select>
              </div>
            </div>
          </section>

          {/* Notifications */}
          <section className="settings-section">
            <div className="settings-section-header">
              <div className="settings-section-icon">
                <i className="fas fa-bell"></i>
              </div>
              <div>
                <h2 className="settings-section-title">Notifications</h2>
                <p className="settings-section-description">Manage notification preferences</p>
              </div>
            </div>

            <div className="settings-group">
              <div className="setting-item">
                <div className="setting-info">
                  <label htmlFor="notifications" className="setting-label">
                    Enable Notifications
                  </label>
                  <p className="setting-description">
                    Receive alerts for warranty expirations and reminders
                  </p>
                </div>
                <label className="toggle-switch">
                  <input
                    type="checkbox"
                    id="notifications"
                    checked={form.notifications_enabled}
                    onChange={(e) =>
                      setForm((prev) => ({ ...prev, notifications_enabled: e.target.checked }))
                    }
                  />
                  <span className="toggle-slider"></span>
                </label>
              </div>
            </div>
          </section>

          {/* Security - Recovery Codes */}
          <section className="settings-section">
            <div className="settings-section-header">
              <div className="settings-section-icon">
                <i className="fas fa-shield-alt"></i>
              </div>
              <div>
                <h2 className="settings-section-title">Account Recovery</h2>
                <p className="settings-section-description">
                  Set up recovery codes to regain access if you forget your password
                </p>
              </div>
            </div>

            <RecoveryCodesSection />
          </section>

          {/* Two-Factor Authentication */}
          <section className="settings-section">
            <div className="settings-section-header">
              <div className="settings-section-icon">
                <i className="fas fa-mobile-alt"></i>
              </div>
              <div>
                <h2 className="settings-section-title">Two-Factor Authentication</h2>
                <p className="settings-section-description">
                  Add an extra layer of security with an authenticator app (Google Authenticator,
                  Authy, etc.)
                </p>
              </div>
            </div>

            <TotpSettings />
          </section>

          {/* Backup & Restore (Admin Only) */}
          {user?.is_admin && (
            <section className="settings-section">
              <div className="settings-section-header">
                <div className="settings-section-icon">
                  <i className="fas fa-database"></i>
                </div>
                <div>
                  <h2 className="settings-section-title">Backup & Restore</h2>
                  <p className="settings-section-description">
                    Create backups of all data including inventories, items, and settings.
                    <strong> Warning:</strong> Restoring a backup will replace all current data.
                  </p>
                </div>
              </div>
              <BackupRestoreSection />
            </section>
          )}

          {/* User Management (Admin Only) */}
          {user?.is_admin && (
            <section className="settings-section">
              <div className="settings-section-header">
                <div className="settings-section-icon">
                  <i className="fas fa-users"></i>
                </div>
                <div>
                  <h2 className="settings-section-title">User Management</h2>
                  <p className="settings-section-description">
                    Manage user accounts and permissions
                  </p>
                </div>
              </div>

              <UserManagement />
            </section>
          )}

          {/* All Access Management */}
          <section className="settings-section">
            <div className="settings-section-header">
              <div className="settings-section-icon">
                <i className="fas fa-user-shield"></i>
              </div>
              <div>
                <h2 className="settings-section-title">All Access (Tier 4)</h2>
                <p className="settings-section-description">
                  Grant or receive full access to all inventories
                </p>
              </div>
            </div>

            <AllAccessManagement />
          </section>

          {/* Save Button */}
          <div className="settings-actions">
            <button
              className="btn btn-primary btn-lg"
              onClick={handleSave}
              disabled={isSaving || !hasChanges}
            >
              {isSaving ? (
                <>
                  <span className="spinner-small"></span>
                  Saving...
                </>
              ) : (
                <>
                  <i className="fas fa-save"></i>
                  Save Settings
                </>
              )}
            </button>
            {hasChanges && (
              <span className="settings-unsaved-hint">
                <i className="fas fa-exclamation-circle"></i>
                You have unsaved changes
              </span>
            )}
          </div>
        </div>
      </div>
    </>
  );
}
