import { useState } from 'react';
import { useApp } from '@/context/AppContext';
import { authApi } from '@/services/api';

export function SessionsSection() {
  const { showToast } = useApp();
  const [confirming, setConfirming] = useState(false);
  const [busy, setBusy] = useState(false);

  const signOutOtherDevices = async () => {
    setBusy(true);
    try {
      const result = await authApi.logoutAllDevices();
      if (result.success) {
        showToast('Signed out all other devices', 'success');
        setConfirming(false);
      } else {
        showToast(result.error ?? 'Failed to sign out other devices', 'error');
      }
    } catch {
      showToast('Failed to sign out other devices', 'error');
    } finally {
      setBusy(false);
    }
  };

  return (
    <div className="sessions-section">
      <p style={{ marginTop: 0, color: 'var(--text-secondary)', fontSize: '0.9rem' }}>
        The Sign out button only clears this browser. Your sign-in token itself stays valid until it
        expires. To end sign-ins on your other devices, use the button below. Changing your password
        or turning two-factor authentication on or off does this automatically.
      </p>

      {confirming ? (
        <div className="alert alert-warning" role="alert">
          <p style={{ marginTop: 0 }}>
            Every other device and browser will be signed out. This device stays signed in.
          </p>
          <div style={{ display: 'flex', gap: '0.5rem' }}>
            <button
              className="btn btn-secondary"
              onClick={() => setConfirming(false)}
              disabled={busy}
            >
              Cancel
            </button>
            <button
              className="btn btn-danger"
              onClick={() => void signOutOtherDevices()}
              disabled={busy}
            >
              {busy ? 'Signing out...' : 'Sign out all other devices'}
            </button>
          </div>
        </div>
      ) : (
        <button className="btn btn-secondary" onClick={() => setConfirming(true)}>
          <i className="fas fa-sign-out-alt"></i>
          Sign out all other devices
        </button>
      )}
    </div>
  );
}
