import { useState, useEffect, useCallback } from 'react';
import { useApp } from '@/context/AppContext';
import { useAuth } from '@/context/AuthContext';
import { authApi } from '@/services/api';
import { Modal, ConfirmModal } from '@/components';
import { buildInviteLink, buildResetLink } from '@/utils/authLink';
import type {
  User,
  CreateUserRequest,
  UpdateUserRequest,
  RegistrationMode,
  RegistrationSettings,
  InviteInfo,
} from '@/types';

const MODE_OPTIONS: { value: RegistrationMode; label: string; description: string }[] = [
  {
    value: 'open',
    label: 'Open',
    description: 'Anyone who can reach this page can create an account.',
  },
  {
    value: 'invite',
    label: 'Invite only',
    description: 'Only people with an invitation link from you can create an account.',
  },
  {
    value: 'closed',
    label: 'Closed',
    description:
      'Nobody can register. You can still add users yourself. Invitations you already made keep their expiry and work again if you switch back to invite only.',
  },
];

/** A link to hand over, shown once because the server keeps only a hash of its token. */
interface IssuedLink {
  title: string;
  intro: string;
  link: string;
  expiresAt: string;
}

export function UserManagement() {
  const { showToast } = useApp();
  const { user: currentUser } = useAuth();
  const [users, setUsers] = useState<User[]>([]);
  const [loading, setLoading] = useState(true);
  const [showCreateModal, setShowCreateModal] = useState(false);
  const [showEditModal, setShowEditModal] = useState(false);
  const [showDeleteModal, setShowDeleteModal] = useState(false);
  const [editingUser, setEditingUser] = useState<User | null>(null);
  const [deletingUser, setDeletingUser] = useState<User | null>(null);
  const [registration, setRegistration] = useState<RegistrationSettings | null>(null);
  const [invites, setInvites] = useState<InviteInfo[]>([]);
  const [issuedLink, setIssuedLink] = useState<IssuedLink | null>(null);
  const [formData, setFormData] = useState({
    username: '',
    full_name: '',
    password: '',
    is_admin: false,
    is_active: true,
  });

  const loadUsers = useCallback(async () => {
    setLoading(true);
    try {
      const result = await authApi.getAllUsers();
      if (result.success && result.data) {
        setUsers(result.data);
      } else {
        showToast(result.error ?? 'Failed to load users', 'error');
      }
    } catch {
      showToast('Failed to load users', 'error');
    } finally {
      setLoading(false);
    }
  }, [showToast]);

  useEffect(() => {
    void loadUsers();
  }, [loadUsers]);

  const loadRegistration = useCallback(async () => {
    try {
      const [settings, list] = await Promise.all([
        authApi.getRegistrationSettings(),
        authApi.listInvites(),
      ]);
      if (settings.success && settings.data) {
        setRegistration(settings.data);
      }
      if (list.success && list.data) {
        setInvites(list.data);
      }
    } catch {
      showToast('Failed to load registration settings', 'error');
    }
  }, [showToast]);

  useEffect(() => {
    void loadRegistration();
  }, [loadRegistration]);

  const changeMode = async (mode: RegistrationMode) => {
    try {
      const result = await authApi.updateRegistrationSettings({ registration_mode: mode });
      if (result.success && result.data) {
        setRegistration(result.data);
        showToast('Registration setting saved', 'success');
      } else {
        showToast(result.error ?? 'Failed to save registration setting', 'error');
      }
    } catch {
      showToast('Failed to save registration setting', 'error');
    }
  };

  const dismissNotice = async () => {
    try {
      const result = await authApi.updateRegistrationSettings({ dismiss_upgrade_notice: true });
      if (result.success && result.data) {
        setRegistration(result.data);
      }
    } catch {
      showToast('Failed to dismiss the notice', 'error');
    }
  };

  const createInvite = async () => {
    try {
      const result = await authApi.createInvite();
      if (result.success && result.data) {
        setIssuedLink({
          title: 'Invitation Link',
          intro: 'Send this link to the person you want to invite. It can be used once.',
          link: buildInviteLink(window.location.origin, result.data.token),
          expiresAt: result.data.expires_at,
        });
        void loadRegistration();
      } else {
        showToast(result.error ?? 'Failed to create invitation', 'error');
      }
    } catch {
      showToast('Failed to create invitation', 'error');
    }
  };

  const revokeInvite = async (inviteId: string) => {
    try {
      const result = await authApi.revokeInvite(inviteId);
      if (result.success) {
        showToast('Invitation revoked', 'success');
        void loadRegistration();
      } else {
        showToast(result.error ?? 'Failed to revoke invitation', 'error');
      }
    } catch {
      showToast('Failed to revoke invitation', 'error');
    }
  };

  const createResetLink = async (user: User) => {
    try {
      const result = await authApi.issueResetLink(user.id);
      if (result.success && result.data) {
        setIssuedLink({
          title: `Password Reset Link for @${user.username}`,
          intro:
            'Send this link to the user. When they use it they choose a new password and are signed out on every device. It can be used once, and a newer link replaces this one.',
          link: buildResetLink(window.location.origin, result.data.token),
          expiresAt: result.data.expires_at,
        });
      } else {
        showToast(result.error ?? 'Failed to create reset link', 'error');
      }
    } catch {
      showToast('Failed to create reset link', 'error');
    }
  };

  const copyIssuedLink = async () => {
    if (!issuedLink) {
      return;
    }
    try {
      await navigator.clipboard.writeText(issuedLink.link);
      showToast('Link copied to clipboard', 'success');
    } catch {
      showToast('Could not copy. Select the link and copy it by hand.', 'error');
    }
  };

  const openCreateModal = () => {
    setFormData({
      username: '',
      full_name: '',
      password: '',
      is_admin: false,
      is_active: true,
    });
    setShowCreateModal(true);
  };

  const openEditModal = (user: User) => {
    setEditingUser(user);
    setFormData({
      username: user.username,
      full_name: user.full_name,
      password: '',
      is_admin: user.is_admin,
      is_active: user.is_active,
    });
    setShowEditModal(true);
  };

  const openDeleteModal = (user: User) => {
    setDeletingUser(user);
    setShowDeleteModal(true);
  };

  const handleCreateUser = async () => {
    if (!formData.username.trim() || !formData.full_name.trim() || !formData.password) {
      showToast('Please fill in all required fields', 'error');
      return;
    }

    if (formData.password.length < 8) {
      showToast('Password must be at least 8 characters', 'error');
      return;
    }

    try {
      const createData: CreateUserRequest = {
        username: formData.username,
        full_name: formData.full_name,
        password: formData.password,
        is_admin: formData.is_admin,
        is_active: formData.is_active,
      };

      const result = await authApi.createUser(createData);
      if (result.success) {
        showToast('User created successfully!', 'success');
        setShowCreateModal(false);
        void loadUsers();
      } else {
        showToast(result.error ?? 'Failed to create user', 'error');
      }
    } catch {
      showToast('Failed to create user', 'error');
    }
  };

  const handleUpdateUser = async () => {
    if (!editingUser) {
      return;
    }

    if (!formData.full_name.trim()) {
      showToast('Please fill in all required fields', 'error');
      return;
    }

    try {
      const updateData: UpdateUserRequest = {
        full_name: formData.full_name,
        is_admin: formData.is_admin,
        is_active: formData.is_active,
      };

      const result = await authApi.updateUser(editingUser.id, updateData);
      if (result.success) {
        showToast('User updated successfully!', 'success');
        setShowEditModal(false);
        void loadUsers();
      } else {
        showToast(result.error ?? 'Failed to update user', 'error');
      }
    } catch {
      showToast('Failed to update user', 'error');
    }
  };

  const handleDeleteUser = async () => {
    if (!deletingUser) {
      return;
    }

    try {
      const result = await authApi.deleteUser(deletingUser.id);
      if (result.success) {
        showToast('User deleted successfully!', 'success');
        setShowDeleteModal(false);
        setDeletingUser(null);
        void loadUsers();
      } else {
        showToast(result.error ?? 'Failed to delete user', 'error');
      }
    } catch {
      showToast('Failed to delete user', 'error');
    }
  };

  if (loading) {
    return (
      <div style={{ padding: '2rem', textAlign: 'center' }}>
        <div className="spinner-small"></div>
        <p style={{ marginTop: '1rem', color: 'var(--text-secondary)' }}>Loading users...</p>
      </div>
    );
  }

  return (
    <div>
      {registration && (
        <div className="settings-group" style={{ marginBottom: '1.5rem' }}>
          <h3 style={{ marginBottom: '0.75rem' }}>Registration</h3>

          {registration.upgrade_notice && (
            <div className="auth-warning" style={{ marginBottom: '1rem' }}>
              <strong>Registration is now invite-only.</strong> This server used to let anyone
              register. Choose who may register below, or create invitation links for the people you
              want to add.{' '}
              <button className="btn btn-sm btn-ghost" onClick={dismissNotice}>
                Dismiss
              </button>
            </div>
          )}

          {MODE_OPTIONS.map((option) => (
            <label
              key={option.value}
              className="form-checkbox"
              style={{ display: 'flex', alignItems: 'flex-start', gap: '0.5rem' }}
            >
              <input
                type="radio"
                name="registration-mode"
                checked={registration.registration_mode === option.value}
                onChange={() => void changeMode(option.value)}
              />
              <span>
                <strong>{option.label}</strong>
                <span className="form-hint" style={{ display: 'block' }}>
                  {option.description}
                </span>
              </span>
            </label>
          ))}

          <div style={{ marginTop: '1rem' }}>
            <button
              className="btn btn-secondary btn-sm"
              onClick={createInvite}
              disabled={registration.registration_mode === 'closed'}
              title={
                registration.registration_mode === 'closed'
                  ? 'Invitations cannot be created while registration is closed'
                  : 'Create an invitation link'
              }
            >
              <i className="fas fa-envelope"></i>
              Invite Someone
            </button>
          </div>

          {invites.length > 0 && (
            <div style={{ marginTop: '0.75rem' }}>
              <div className="form-hint">Invitations waiting to be used</div>
              {invites.map((invite) => (
                <div
                  key={invite.id}
                  className="setting-item"
                  style={{ alignItems: 'center', justifyContent: 'space-between' }}
                >
                  <span style={{ fontSize: '0.875rem', color: 'var(--text-secondary)' }}>
                    Created {new Date(invite.created_at).toLocaleDateString()}
                    {invite.created_by_username ? ` by @${invite.created_by_username}` : ''},
                    expires {new Date(invite.expires_at).toLocaleDateString()}
                  </span>
                  <button
                    className="btn btn-sm btn-ghost btn-danger"
                    onClick={() => void revokeInvite(invite.id)}
                    title="Revoke this invitation"
                  >
                    <i className="fas fa-ban"></i> Revoke
                  </button>
                </div>
              ))}
            </div>
          )}
        </div>
      )}

      <div style={{ marginBottom: '1rem' }}>
        <button className="btn btn-primary btn-sm" onClick={openCreateModal}>
          <i className="fas fa-plus"></i>
          Add User
        </button>
      </div>

      <div className="settings-group">
        {users.map((user) => (
          <div key={user.id} className="setting-item" style={{ alignItems: 'flex-start' }}>
            <div className="setting-info" style={{ flex: 1 }}>
              <div
                style={{
                  display: 'flex',
                  alignItems: 'center',
                  gap: '0.75rem',
                  marginBottom: '0.5rem',
                }}
              >
                <div>
                  <div style={{ fontSize: '1rem', fontWeight: 600, color: 'var(--text-primary)' }}>
                    {user.full_name}
                  </div>
                  <div style={{ fontSize: '0.875rem', color: 'var(--text-secondary)' }}>
                    @{user.username}
                  </div>
                </div>
              </div>
              <div style={{ display: 'flex', gap: '0.5rem', marginTop: '0.5rem' }}>
                {user.is_admin && (
                  <span className="badge badge-primary">
                    <i className="fas fa-shield-alt"></i> Administrator
                  </span>
                )}
                <span className={`badge ${user.is_active ? 'badge-success' : 'badge-danger'}`}>
                  <i className={`fas fa-${user.is_active ? 'check-circle' : 'times-circle'}`}></i>
                  {user.is_active ? 'Active' : 'Inactive'}
                </span>
                {user.id === currentUser?.id && (
                  <span className="badge badge-info">
                    <i className="fas fa-user"></i> You
                  </span>
                )}
              </div>
            </div>
            <div style={{ display: 'flex', gap: '0.5rem' }}>
              <button
                className="btn btn-sm btn-ghost"
                onClick={() => void createResetLink(user)}
                title="Create a password reset link"
              >
                <i className="fas fa-key"></i>
              </button>
              <button
                className="btn btn-sm btn-ghost"
                onClick={() => openEditModal(user)}
                title="Edit User"
              >
                <i className="fas fa-edit"></i>
              </button>
              <button
                className="btn btn-sm btn-ghost btn-danger"
                onClick={() => openDeleteModal(user)}
                disabled={user.id === currentUser?.id}
                title={user.id === currentUser?.id ? 'Cannot delete yourself' : 'Delete User'}
              >
                <i className="fas fa-trash"></i>
              </button>
            </div>
          </div>
        ))}
      </div>

      {/* Create User Modal */}
      <Modal
        isOpen={showCreateModal}
        onClose={() => setShowCreateModal(false)}
        title="Create New User"
        subtitle="Add a new user to the system"
        footer={
          <>
            <button className="btn btn-secondary" onClick={() => setShowCreateModal(false)}>
              Cancel
            </button>
            <button className="btn btn-success" onClick={handleCreateUser}>
              <i className="fas fa-user-plus"></i>
              Create User
            </button>
          </>
        }
      >
        <div className="form-group">
          <label className="form-label" htmlFor="create-username">
            Username *
          </label>
          <input
            type="text"
            className="form-input"
            id="create-username"
            value={formData.username}
            onChange={(e) => setFormData({ ...formData, username: e.target.value })}
            autoFocus
          />
        </div>

        <div className="form-group">
          <label className="form-label" htmlFor="create-full-name">
            Full Name *
          </label>
          <input
            type="text"
            className="form-input"
            id="create-full-name"
            value={formData.full_name}
            onChange={(e) => setFormData({ ...formData, full_name: e.target.value })}
          />
        </div>

        <div className="form-group">
          <label className="form-label" htmlFor="create-password">
            Password *
          </label>
          <input
            type="password"
            className="form-input"
            id="create-password"
            value={formData.password}
            onChange={(e) => setFormData({ ...formData, password: e.target.value })}
            placeholder="Minimum 8 characters"
          />
        </div>

        <div className="form-group">
          <label className="form-checkbox">
            <input
              type="checkbox"
              checked={formData.is_admin}
              onChange={(e) => setFormData({ ...formData, is_admin: e.target.checked })}
            />
            <span className="form-checkbox-label">Administrator</span>
          </label>
          <p className="form-hint">
            Administrators have full access to all features, including user management
          </p>
        </div>

        <div className="form-group">
          <label className="form-checkbox">
            <input
              type="checkbox"
              checked={formData.is_active}
              onChange={(e) => setFormData({ ...formData, is_active: e.target.checked })}
            />
            <span className="form-checkbox-label">Account Active</span>
          </label>
          <p className="form-hint">Inactive users cannot log in</p>
        </div>
      </Modal>

      {/* Edit User Modal */}
      <Modal
        isOpen={showEditModal}
        onClose={() => setShowEditModal(false)}
        title="Edit User"
        subtitle={`Editing ${editingUser?.username}`}
        footer={
          <>
            <button className="btn btn-secondary" onClick={() => setShowEditModal(false)}>
              Cancel
            </button>
            <button className="btn btn-primary" onClick={handleUpdateUser}>
              <i className="fas fa-save"></i>
              Save Changes
            </button>
          </>
        }
      >
        <div className="form-group">
          <label className="form-label">Username</label>
          <input
            type="text"
            className="form-input"
            value={formData.username}
            disabled
            style={{
              background: 'var(--bg-secondary)',
              color: 'var(--text-tertiary)',
              cursor: 'not-allowed',
            }}
          />
          <p className="form-hint">Username cannot be changed</p>
        </div>

        <div className="form-group">
          <label className="form-label" htmlFor="edit-full-name">
            Full Name *
          </label>
          <input
            type="text"
            className="form-input"
            id="edit-full-name"
            value={formData.full_name}
            onChange={(e) => setFormData({ ...formData, full_name: e.target.value })}
          />
        </div>

        <div className="form-group">
          <p className="form-hint">
            To change this user&apos;s password, close this window and use the key button next to
            them to create a reset link.
          </p>
        </div>

        <div className="form-group">
          <label className="form-checkbox">
            <input
              type="checkbox"
              checked={formData.is_admin}
              onChange={(e) => setFormData({ ...formData, is_admin: e.target.checked })}
              disabled={editingUser?.id === currentUser?.id}
            />
            <span className="form-checkbox-label">Administrator</span>
          </label>
          {editingUser?.id === currentUser?.id && (
            <p className="form-hint" style={{ color: 'var(--warning-color)' }}>
              You cannot change your own admin status
            </p>
          )}
        </div>

        <div className="form-group">
          <label className="form-checkbox">
            <input
              type="checkbox"
              checked={formData.is_active}
              onChange={(e) => setFormData({ ...formData, is_active: e.target.checked })}
              disabled={editingUser?.id === currentUser?.id}
            />
            <span className="form-checkbox-label">Account Active</span>
          </label>
          {editingUser?.id === currentUser?.id && (
            <p className="form-hint" style={{ color: 'var(--warning-color)' }}>
              You cannot deactivate your own account
            </p>
          )}
        </div>
      </Modal>

      {/* Link to hand over; shown once */}
      <Modal
        isOpen={issuedLink !== null}
        onClose={() => setIssuedLink(null)}
        title={issuedLink?.title ?? ''}
        footer={
          <>
            <button className="btn btn-secondary" onClick={() => void copyIssuedLink()}>
              <i className="fas fa-copy"></i>
              Copy Link
            </button>
            <button className="btn btn-primary" onClick={() => setIssuedLink(null)}>
              Done
            </button>
          </>
        }
      >
        <p className="form-hint">{issuedLink?.intro}</p>
        <div className="form-group">
          <input
            type="text"
            className="form-input"
            readOnly
            value={issuedLink?.link ?? ''}
            onFocus={(e) => e.target.select()}
            aria-label="Link"
          />
        </div>
        <p className="form-hint">
          Expires {issuedLink ? new Date(issuedLink.expiresAt).toLocaleString() : ''}.
        </p>
        <div className="auth-warning">
          <strong>Copy it now.</strong> The server keeps only a fingerprint of the link, so it
          cannot be shown again. If it is lost, create a new one.
        </div>
      </Modal>

      {/* Delete Confirmation Modal */}
      <ConfirmModal
        isOpen={showDeleteModal}
        onClose={() => {
          setShowDeleteModal(false);
          setDeletingUser(null);
        }}
        onConfirm={handleDeleteUser}
        title="Delete User"
        message={`Are you sure you want to delete ${deletingUser?.full_name} (@${deletingUser?.username})? This action cannot be undone.`}
        confirmText="Delete User"
        confirmButtonClass="btn-danger"
      />
    </div>
  );
}
