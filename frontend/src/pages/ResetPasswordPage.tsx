import { useEffect, useState } from 'react';
import { Link } from 'react-router-dom';
import { authApi } from '@/services/api';
import { clearUrlFragment, readFragmentToken } from '@/utils/authLink';
import '@/styles/auth.css';

/**
 * Set a new password with a link from an administrator (`/reset#token=...`).
 *
 * The token is in the URL fragment, so it never reaches a server log. It is read once and then
 * removed from the address bar.
 */
export function ResetPasswordPage() {
  const [token] = useState(() => readFragmentToken(window.location.hash, 'token'));
  const [password, setPassword] = useState('');
  const [confirmPassword, setConfirmPassword] = useState('');
  const [showPassword, setShowPassword] = useState(false);
  const [isLoading, setIsLoading] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [done, setDone] = useState(false);

  useEffect(() => {
    clearUrlFragment();
  }, []);

  const handleSubmit = async (e: React.FormEvent) => {
    e.preventDefault();
    if (token === null) {
      return;
    }
    if (password.length < 8) {
      setError('Password must be at least 8 characters');
      return;
    }
    if (password !== confirmPassword) {
      setError('Passwords do not match');
      return;
    }

    setIsLoading(true);
    setError(null);
    try {
      const result = await authApi.resetPassword(token, password);
      if (result.success) {
        setDone(true);
      } else {
        setError(result.error ?? 'This link is invalid or has expired.');
      }
    } catch {
      setError('Network error. Please check your connection and try again.');
    } finally {
      setIsLoading(false);
    }
  };

  return (
    <div className="auth-page">
      <div className="auth-background">
        <div className="auth-gradient-orb auth-gradient-orb-1"></div>
        <div className="auth-gradient-orb auth-gradient-orb-2"></div>
        <div className="auth-gradient-orb auth-gradient-orb-3"></div>
      </div>

      <div className="auth-container">
        <div className="auth-card auth-card-register">
          <div className="auth-header">
            <div className="auth-logo">
              <img src="/logo_full.png" alt="Home Registry" className="auth-logo-img" />
            </div>
            <h1 className="auth-title">Reset Password</h1>
            <p className="auth-subtitle">Choose a new password for your account</p>
          </div>

          {token === null ? (
            <>
              <div className="auth-error">
                <i className="fas fa-exclamation-circle"></i>
                <span>
                  This password reset link is not valid. Ask an administrator for a new one. If you
                  reloaded this page, open the link from the administrator again.
                </span>
              </div>
              <div className="auth-footer">
                <p>
                  <Link to="/login" className="auth-link">
                    Back to sign in
                  </Link>
                </p>
              </div>
            </>
          ) : done ? (
            <>
              <div className="auth-step">
                <p className="step-description">
                  Your password has been changed and you have been signed out everywhere. Sign in
                  with your new password.
                </p>
              </div>
              <div className="auth-footer">
                <p>
                  <Link to="/login" className="auth-link">
                    Sign in
                  </Link>
                </p>
              </div>
            </>
          ) : (
            <>
              {error && (
                <div className="auth-error">
                  <i className="fas fa-exclamation-circle"></i>
                  <span>{error}</span>
                </div>
              )}
              <form onSubmit={handleSubmit} className="auth-form">
                <div className="form-group">
                  <label htmlFor="new-password">
                    <i className="fas fa-lock"></i>
                    New Password
                  </label>
                  <div className="input-with-icon">
                    <input
                      type={showPassword ? 'text' : 'password'}
                      id="new-password"
                      value={password}
                      onChange={(e) => {
                        setPassword(e.target.value);
                        setError(null);
                      }}
                      placeholder="Min 8 characters"
                      autoComplete="new-password"
                      autoFocus
                    />
                    <button
                      type="button"
                      className="password-toggle"
                      onClick={() => setShowPassword(!showPassword)}
                      tabIndex={-1}
                    >
                      <i className={`fas ${showPassword ? 'fa-eye-slash' : 'fa-eye'}`}></i>
                    </button>
                  </div>
                </div>

                <div className="form-group">
                  <label htmlFor="confirm-new-password">
                    <i className="fas fa-lock"></i>
                    Confirm New Password
                  </label>
                  <input
                    type={showPassword ? 'text' : 'password'}
                    id="confirm-new-password"
                    value={confirmPassword}
                    onChange={(e) => {
                      setConfirmPassword(e.target.value);
                      setError(null);
                    }}
                    placeholder="Confirm password"
                    autoComplete="new-password"
                  />
                </div>

                <button type="submit" className="auth-submit-btn" disabled={isLoading}>
                  {isLoading ? (
                    <>
                      <span className="btn-spinner"></span>
                      Saving...
                    </>
                  ) : (
                    <>
                      <i className="fas fa-key"></i>
                      Set New Password
                    </>
                  )}
                </button>
              </form>
            </>
          )}
        </div>
      </div>
    </div>
  );
}
