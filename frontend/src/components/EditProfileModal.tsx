import { useState, useEffect } from 'react';
import { Modal } from './Modal';
import { useApp } from '@/context/AppContext';
import { useAuth } from '@/context/AuthContext';
import { authApi } from '@/services/api';
import { formatTimestamp, type DateFormatType } from '@/utils/dateFormat';

interface EditProfileModalProps {
  onClose: () => void;
}

export function EditProfileModal({ onClose }: EditProfileModalProps) {
  const { showToast } = useApp();
  const { user, refreshUser, settings } = useAuth();
  const [form, setForm] = useState({
    full_name: '',
  });
  const [isLoading, setIsLoading] = useState(false);
  const [errors, setErrors] = useState<Record<string, string>>({});

  useEffect(() => {
    if (user) {
      setForm({
        full_name: user.full_name || '',
      });
    }
  }, [user]);

  const validateForm = (): boolean => {
    const newErrors: Record<string, string> = {};

    if (!form.full_name.trim()) {
      newErrors.full_name = 'Display name is required';
    } else if (form.full_name.trim().length < 2) {
      newErrors.full_name = 'Display name must be at least 2 characters';
    }

    setErrors(newErrors);
    return Object.keys(newErrors).length === 0;
  };

  const handleSubmit = async (e: React.FormEvent) => {
    e.preventDefault();

    if (!validateForm()) {
      return;
    }

    setIsLoading(true);
    try {
      const result = await authApi.updateProfile({
        full_name: form.full_name.trim(),
      });

      if (result.success) {
        await refreshUser();
        showToast('Profile updated successfully', 'success');
        onClose();
      } else {
        showToast(result.error ?? 'Failed to update profile', 'error');
      }
    } catch (error) {
      console.error('Error updating profile:', error);
      showToast('An unexpected error occurred', 'error');
    } finally {
      setIsLoading(false);
    }
  };

  const handleInputChange =
    (field: keyof typeof form) => (e: React.ChangeEvent<HTMLInputElement>) => {
      setForm((prev) => ({ ...prev, [field]: e.target.value }));
      if (errors[field]) {
        setErrors((prev) => {
          const { [field]: _removed, ...next } = prev;
          return next;
        });
      }
    };

  const hasChanges = user && form.full_name.trim() !== (user.full_name || '');

  return (
    <Modal
      isOpen={true}
      onClose={onClose}
      title="Edit Profile"
      subtitle="Update your account information"
      footer={
        <div className="modal-actions">
          <button
            type="button"
            className="btn btn-secondary"
            onClick={onClose}
            disabled={isLoading}
          >
            Cancel
          </button>
          <button
            type="submit"
            className="btn btn-primary"
            form="edit-profile-form"
            disabled={isLoading || !hasChanges}
          >
            {isLoading ? (
              <>
                <span className="spinner-small"></span>
                Saving...
              </>
            ) : (
              <>
                <i className="fas fa-check"></i>
                Save Changes
              </>
            )}
          </button>
        </div>
      }
    >
      <form id="edit-profile-form" onSubmit={handleSubmit} className="settings-form">
        <div className="form-group">
          <label htmlFor="username">Username</label>
          <input
            type="text"
            id="username"
            value={user?.username ?? ''}
            disabled
            className="input-disabled"
          />
          <span className="form-hint">Username cannot be changed</span>
        </div>

        <div className={`form-group ${errors.full_name ? 'has-error' : ''}`}>
          <label htmlFor="full_name">Display Name</label>
          <input
            type="text"
            id="full_name"
            value={form.full_name}
            onChange={handleInputChange('full_name')}
            placeholder="Enter your display name"
            disabled={isLoading}
          />
          {errors.full_name && <span className="form-error">{errors.full_name}</span>}
        </div>

        <div className="form-group">
          <label>Account Status</label>
          <div className="account-info">
            <span
              className={`status-badge ${user?.is_active ? 'status-active' : 'status-inactive'}`}
            >
              <i className={`fas fa-${user?.is_active ? 'check-circle' : 'times-circle'}`}></i>
              {user?.is_active ? 'Active' : 'Inactive'}
            </span>
            {user?.is_admin && (
              <span className="status-badge status-admin">
                <i className="fas fa-shield-alt"></i>
                Administrator
              </span>
            )}
          </div>
        </div>

        <div className="form-group">
          <label>Member Since</label>
          <input
            type="text"
            value={formatTimestamp(
              user?.created_at,
              (settings?.date_format ?? 'MM/DD/YYYY') as DateFormatType
            )}
            disabled
            className="input-disabled"
          />
        </div>
      </form>
    </Modal>
  );
}
