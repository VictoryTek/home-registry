import { useState, useEffect, useCallback } from 'react';
import { useApp } from '@/context/AppContext';
import { useAuth } from '@/context/AuthContext';
import { backupApi, SESSION_NOTICE_KEY } from '@/services/api';
import { ConfirmModal } from '@/components';
import type { BackupInfo, RestoreReport } from '@/types';

function formatBackupDate(dateString: string): string {
  const date = new Date(dateString);
  return date.toLocaleString('en-US', {
    year: 'numeric',
    month: 'short',
    day: 'numeric',
    hour: '2-digit',
    minute: '2-digit',
  });
}

export function BackupRestoreSection() {
  const { showToast } = useApp();
  const { user, logout } = useAuth();
  const [backups, setBackups] = useState<BackupInfo[]>([]);
  const [isLoading, setIsLoading] = useState(true);
  const [isCreating, setIsCreating] = useState(false);
  const [isUploading, setIsUploading] = useState(false);
  const [isRestoring, setIsRestoring] = useState(false);
  const [restoreTarget, setRestoreTarget] = useState<string | null>(null);
  const [deleteTarget, setDeleteTarget] = useState<string | null>(null);
  // Set when the backup's two-factor data cannot be decrypted with this server's key
  const [twoFaMismatch, setTwoFaMismatch] = useState<{ target: string; message: string } | null>(
    null
  );
  // Set after a restore: every session has ended, so show the result before signing out
  const [restoreResult, setRestoreResult] = useState<RestoreReport | null>(null);

  // Load backups on mount
  const loadBackups = useCallback(async () => {
    setIsLoading(true);
    try {
      const result = await backupApi.list();
      if (result.success && result.data) {
        setBackups(result.data);
      } else {
        console.error('Failed to load backups:', result.error);
      }
    } catch (error) {
      console.error('Error loading backups:', error);
    } finally {
      setIsLoading(false);
    }
  }, []);

  useEffect(() => {
    void loadBackups();
  }, [loadBackups]);

  // Create backup handler
  const handleCreateBackup = async () => {
    setIsCreating(true);
    try {
      const result = await backupApi.create();
      if (result.success && result.data) {
        showToast(`Backup created: ${result.data.name}`, 'success');
        await loadBackups();
      } else {
        showToast(result.error ?? 'Failed to create backup', 'error');
      }
    } catch (error) {
      console.error('Error creating backup:', error);
      showToast('An error occurred while creating backup', 'error');
    } finally {
      setIsCreating(false);
    }
  };

  // Upload backup handler
  const handleUploadBackup = async (e: React.ChangeEvent<HTMLInputElement>) => {
    const file = e.target.files?.[0];
    if (!file) {
      return;
    }

    // Reset the input so the same file can be uploaded again
    e.target.value = '';

    if (!file.name.endsWith('.json')) {
      showToast('Please select a .json backup file', 'error');
      return;
    }

    setIsUploading(true);
    try {
      const result = await backupApi.upload(file);
      if (result.success && result.data) {
        showToast(`Backup uploaded: ${result.data.name}`, 'success');
        await loadBackups();
      } else {
        showToast(result.error ?? 'Failed to upload backup', 'error');
      }
    } catch (error) {
      console.error('Error uploading backup:', error);
      showToast('An error occurred while uploading backup', 'error');
    } finally {
      setIsUploading(false);
    }
  };

  // Download backup handler
  const handleDownloadBackup = async (filename: string) => {
    try {
      await backupApi.download(filename);
      showToast('Backup download started', 'success');
    } catch (error) {
      console.error('Error downloading backup:', error);
      showToast('Failed to download backup', 'error');
    }
  };

  // Restore a backup. `withoutTwoFa` is only set after the admin confirmed the second dialog.
  const runRestore = async (target: string, withoutTwoFa: boolean) => {
    setIsRestoring(true);
    try {
      const result = await backupApi.restore(target, withoutTwoFa);
      if (result.success && result.data) {
        // The restore ended every session, including this one. Do not call the API again; show
        // the outcome and let the admin continue to the sign-in page.
        setRestoreResult(result.data);
      } else if (result.error === 'totp_key_mismatch') {
        setTwoFaMismatch({ target, message: result.message ?? '' });
      } else {
        showToast(result.message ?? result.error ?? 'Failed to restore backup', 'error');
      }
    } catch (error) {
      console.error('Error restoring backup:', error);
      showToast('An error occurred while restoring backup', 'error');
    } finally {
      setIsRestoring(false);
    }
  };

  const handleRestoreConfirm = async () => {
    if (!restoreTarget) {
      return;
    }
    const target = restoreTarget;
    setRestoreTarget(null);
    await runRestore(target, false);
  };

  const handleRestoreWithoutTwoFa = async () => {
    if (!twoFaMismatch) {
      return;
    }
    const target = twoFaMismatch.target;
    setTwoFaMismatch(null);
    await runRestore(target, true);
  };

  const continueToSignIn = () => {
    logout();
    sessionStorage.setItem(SESSION_NOTICE_KEY, 'revoked');
    window.location.assign('/login');
  };

  // Delete confirm handler
  const handleDeleteConfirm = async () => {
    if (!deleteTarget) {
      return;
    }

    const target = deleteTarget;
    setDeleteTarget(null);

    try {
      const result = await backupApi.delete(target);
      if (result.success) {
        showToast(`Backup deleted: ${target}`, 'success');
        await loadBackups();
      } else {
        showToast(result.error ?? 'Failed to delete backup', 'error');
      }
    } catch (error) {
      console.error('Error deleting backup:', error);
      showToast('An error occurred while deleting backup', 'error');
    }
  };

  // Only show for admin users
  if (!user?.is_admin) {
    return null;
  }

  return (
    <>
      {/* Backup Actions */}
      <div style={{ display: 'flex', gap: '0.75rem', marginBottom: '1rem' }}>
        <button
          className="btn btn-primary"
          onClick={handleCreateBackup}
          disabled={isCreating || isRestoring}
        >
          {isCreating ? (
            <>
              <span className="spinner-small"></span> Creating...
            </>
          ) : (
            <>
              <i className="fas fa-plus-circle"></i> Create Backup
            </>
          )}
        </button>
        <label
          className="btn btn-secondary"
          style={{ cursor: isUploading || isRestoring ? 'not-allowed' : 'pointer' }}
        >
          {isUploading ? (
            <>
              <span className="spinner-small"></span> Uploading...
            </>
          ) : (
            <>
              <i className="fas fa-upload"></i> Upload Backup
            </>
          )}
          <input
            type="file"
            accept=".json"
            style={{ display: 'none' }}
            onChange={handleUploadBackup}
            disabled={isUploading || isRestoring}
          />
        </label>
      </div>

      {/* Restoring indicator */}
      {isRestoring && (
        <div
          style={{
            padding: '0.75rem 1rem',
            marginBottom: '1rem',
            background: 'var(--warning-bg, #fff3cd)',
            border: '1px solid var(--warning-border, #ffc107)',
            borderRadius: '0.375rem',
            display: 'flex',
            alignItems: 'center',
            gap: '0.5rem',
          }}
        >
          <span className="spinner-small"></span>
          <span>Restoring backup... This may take a moment. Do not close this page.</span>
        </div>
      )}

      {/* Backups Table */}
      <div style={{ overflowX: 'auto' }}>
        <table className="settings-table" style={{ width: '100%' }}>
          <thead>
            <tr>
              <th>Backup Name</th>
              <th>Date Created</th>
              <th>Size</th>
              <th>Actions</th>
            </tr>
          </thead>
          <tbody>
            {isLoading ? (
              <tr>
                <td colSpan={4} style={{ textAlign: 'center', padding: '2rem' }}>
                  <span className="spinner-small"></span> Loading backups...
                </td>
              </tr>
            ) : backups.length === 0 ? (
              <tr>
                <td colSpan={4} style={{ textAlign: 'center', padding: '2rem', color: '#888' }}>
                  No backups found. Create your first backup above.
                </td>
              </tr>
            ) : (
              backups.map((backup) => (
                <tr key={backup.name}>
                  <td style={{ fontFamily: 'monospace', fontSize: '0.85em' }}>{backup.name}</td>
                  <td>{formatBackupDate(backup.date)}</td>
                  <td>{backup.size}</td>
                  <td>
                    <div style={{ display: 'flex', gap: '0.25rem' }}>
                      <button
                        className="btn btn-sm btn-secondary"
                        onClick={() => handleDownloadBackup(backup.name)}
                        title="Download"
                        disabled={isRestoring}
                      >
                        <i className="fas fa-download"></i>
                      </button>
                      <button
                        className="btn btn-sm btn-secondary"
                        onClick={() => setRestoreTarget(backup.name)}
                        title="Restore"
                        disabled={isRestoring}
                      >
                        <i className="fas fa-undo"></i>
                      </button>
                      <button
                        className="btn btn-sm btn-danger"
                        onClick={() => setDeleteTarget(backup.name)}
                        title="Delete"
                        disabled={isRestoring}
                      >
                        <i className="fas fa-trash"></i>
                      </button>
                    </div>
                  </td>
                </tr>
              ))
            )}
          </tbody>
        </table>
      </div>

      {/* Restore Confirmation Modal */}
      <ConfirmModal
        isOpen={restoreTarget !== null}
        onClose={() => setRestoreTarget(null)}
        onConfirm={handleRestoreConfirm}
        title="Confirm Backup Restore"
        message={`This will replace ALL current data with the backup "${restoreTarget ?? ''}". An automatic backup will be created first. Everyone, including you, will be signed out afterwards. Uploaded item photos are not part of backups. This action cannot be undone.`}
        confirmText="Restore Backup"
        confirmButtonClass="btn-danger"
        icon="fas fa-exclamation-triangle"
      />

      {/* Two-factor key mismatch: a separate confirmation for the override */}
      <ConfirmModal
        isOpen={twoFaMismatch !== null}
        onClose={() => setTwoFaMismatch(null)}
        onConfirm={handleRestoreWithoutTwoFa}
        title="Two-Factor Authentication Cannot Be Restored"
        message={`${twoFaMismatch?.message ?? ''} Restore everything except two-factor authentication? Affected users will have to set up two-factor authentication again, and until they do they can sign in with their password alone.`}
        confirmText="Restore Without 2FA"
        confirmButtonClass="btn-danger"
        icon="fas fa-shield-alt"
      />

      {/* Restore result: sessions have ended, so this is shown before signing out */}
      {restoreResult && (
        <div className="modal-overlay active">
          <div className="modal-content" style={{ maxWidth: '520px' }}>
            <div className="modal-header">
              <h2 className="modal-title">
                <i className="fas fa-check-circle" style={{ marginRight: '0.5rem' }}></i>
                Backup Restored
              </h2>
            </div>
            <div className="modal-body">
              <p>
                Restored {restoreResult.tables_restored} tables ({restoreResult.rows_restored}{' '}
                rows). A backup of the previous data was saved as{' '}
                <strong>{restoreResult.safety_backup}</strong>.
              </p>
              {restoreResult.two_factor_cleared_users !== undefined && (
                <div className="alert alert-warning" role="alert">
                  Two-factor authentication was cleared for {restoreResult.two_factor_cleared_users}{' '}
                  user(s); they must set it up again.
                </div>
              )}
              {restoreResult.warnings.length > 0 && (
                <ul>
                  {restoreResult.warnings.map((warning) => (
                    <li key={warning}>{warning}</li>
                  ))}
                </ul>
              )}
              <p>All sessions have ended. Everyone, including you, must sign in again.</p>
            </div>
            <div className="modal-footer">
              <button className="btn btn-primary" onClick={continueToSignIn}>
                Continue to sign in
              </button>
            </div>
          </div>
        </div>
      )}

      {/* Delete Confirmation Modal */}
      <ConfirmModal
        isOpen={deleteTarget !== null}
        onClose={() => setDeleteTarget(null)}
        onConfirm={handleDeleteConfirm}
        title="Delete Backup"
        message={`Are you sure you want to delete backup "${deleteTarget ?? ''}"? This cannot be undone.`}
        confirmText="Delete"
        confirmButtonClass="btn-danger"
        icon="fas fa-trash"
      />
    </>
  );
}
