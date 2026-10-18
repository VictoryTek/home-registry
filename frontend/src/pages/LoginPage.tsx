import { useEffect, useState } from 'react';
import { useNavigate, Link } from 'react-router-dom';
import { useAuth } from '@/context/AuthContext';
import { SESSION_NOTICE_KEY } from '@/services/api';
import '@/styles/auth.css';

export function LoginPage() {
  const navigate = useNavigate();
  const { login, completeTotpLogin, totpRequired, clearTotpRequired, registrationMode } = useAuth();
  const [isLoading, setIsLoading] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [showPassword, setShowPassword] = useState(false);
  const [totpCode, setTotpCode] = useState('');

  // Explain a forced sign-out: the server revoked the session (password changed, signed out of
  // other devices, role changed). Read once; removed after mount so a remount still shows it.
  const [sessionNotice] = useState<string | null>(() => {
    try {
      return sessionStorage.getItem(SESSION_NOTICE_KEY) === 'revoked'
        ? 'Your session ended because your account was changed or you signed out of all devices'
        : null;
    } catch {
      return null;
    }
  });
  useEffect(() => {
    try {
      sessionStorage.removeItem(SESSION_NOTICE_KEY);
    } catch {
      // sessionStorage unavailable; nothing to clear
    }
  }, []);

  const [formData, setFormData] = useState({
    username: '',
    password: '',
  });

  const handleInputChange = (e: React.ChangeEvent<HTMLInputElement>) => {
    const { name, value } = e.target;
    setFormData((prev) => ({ ...prev, [name]: value }));
    setError(null);
  };

  const handleTotpCodeChange = (e: React.ChangeEvent<HTMLInputElement>) => {
    const value = e.target.value.replace(/\D/g, '').slice(0, 6);
    setTotpCode(value);
    setError(null);
  };

  const handleSubmit = async (e: React.FormEvent) => {
    e.preventDefault();

    if (!formData.username.trim()) {
      setError('Please enter your username');
      return;
    }

    if (!formData.password) {
      setError('Please enter your password');
      return;
    }

    setIsLoading(true);
    setError(null);

    const result = await login(formData.username, formData.password);

    if (result.success && !result.totpRequired) {
      navigate('/');
    } else if (result.success && result.totpRequired) {
      // TOTP step will be shown automatically via totpRequired state
      setError(null);
    } else {
      setError(result.error ?? 'Login failed. Please check your credentials.');
    }

    setIsLoading(false);
  };

  const handleTotpSubmit = async (e: React.FormEvent) => {
    e.preventDefault();

    if (totpCode.length !== 6) {
      setError('Please enter a 6-digit code');
      return;
    }

    if (!totpRequired?.partial_token) {
      setError('Session expired. Please log in again.');
      clearTotpRequired();
      return;
    }

    setIsLoading(true);
    setError(null);

    const result = await completeTotpLogin(totpRequired.partial_token, totpCode);

    if (result.success) {
      navigate('/');
    } else {
      setError(result.error ?? 'Invalid code. Please try again.');
    }

    setIsLoading(false);
  };

  const handleBackToLogin = () => {
    clearTotpRequired();
    setTotpCode('');
    setError(null);
  };

  // TOTP verification step
  if (totpRequired) {
    return (
      <div className="auth-page">
        <div className="auth-background">
          <div className="auth-gradient-orb auth-gradient-orb-1"></div>
          <div className="auth-gradient-orb auth-gradient-orb-2"></div>
          <div className="auth-gradient-orb auth-gradient-orb-3"></div>
        </div>

        <div className="auth-container">
          <div className="auth-card">
            <div className="auth-header">
              <div className="auth-logo">
                <img src="/logo_full.png" alt="Home Registry" className="auth-logo-img" />
              </div>
              <h1 className="auth-title">Two-Factor Authentication</h1>
              <p className="auth-subtitle">Enter the 6-digit code from your authenticator app</p>
            </div>

            {error && (
              <div className="auth-error" role="alert">
                <i className="fas fa-exclamation-circle"></i>
                <span>{error}</span>
              </div>
            )}

            <form onSubmit={handleTotpSubmit} className="auth-form">
              <div className="form-group">
                <label htmlFor="totpCode">
                  <i className="fas fa-shield-alt"></i>
                  Authenticator Code
                </label>
                <input
                  type="text"
                  id="totpCode"
                  value={totpCode}
                  onChange={handleTotpCodeChange}
                  placeholder="000000"
                  style={{
                    fontFamily: 'monospace',
                    letterSpacing: '0.3em',
                    textAlign: 'center',
                    fontSize: '1.5rem',
                  }}
                  autoComplete="one-time-code"
                  inputMode="numeric"
                  maxLength={6}
                  autoFocus
                  aria-label="Enter 6-digit authenticator code"
                />
              </div>

              <button
                type="submit"
                className="auth-submit-btn"
                disabled={isLoading || totpCode.length !== 6}
              >
                {isLoading ? (
                  <>
                    <span className="btn-spinner"></span>
                    Verifying...
                  </>
                ) : (
                  <>
                    <i className="fas fa-check-circle"></i>
                    Verify
                  </>
                )}
              </button>
            </form>

            <div className="auth-footer">
              <p>
                <button
                  type="button"
                  className="auth-link"
                  onClick={handleBackToLogin}
                  style={{
                    background: 'none',
                    border: 'none',
                    cursor: 'pointer',
                    font: 'inherit',
                  }}
                >
                  Back to Login
                </button>
              </p>
            </div>
          </div>
        </div>
      </div>
    );
  }

  return (
    <div className="auth-page">
      <div className="auth-background">
        <div className="auth-gradient-orb auth-gradient-orb-1"></div>
        <div className="auth-gradient-orb auth-gradient-orb-2"></div>
        <div className="auth-gradient-orb auth-gradient-orb-3"></div>
      </div>

      <div className="auth-container">
        <div className="auth-card">
          <div className="auth-header">
            <div className="auth-logo">
              <img src="/logo_full.png" alt="Home Registry" className="auth-logo-img" />
            </div>
            <h1 className="auth-title">Welcome Back</h1>
            <p className="auth-subtitle">Sign in to manage your inventory</p>
          </div>

          {sessionNotice && !error && (
            <div className="alert alert-warning" role="status" style={{ marginBottom: '1rem' }}>
              <i className="fas fa-info-circle"></i> {sessionNotice}
            </div>
          )}

          {error && (
            <div className="auth-error" role="alert">
              <i className="fas fa-exclamation-circle"></i>
              <span>{error}</span>
            </div>
          )}

          <form onSubmit={handleSubmit} className="auth-form">
            <div className="form-group">
              <label htmlFor="username">
                <i className="fas fa-user"></i>
                Username
              </label>
              <input
                type="text"
                id="username"
                name="username"
                value={formData.username}
                onChange={handleInputChange}
                placeholder="Enter your username"
                autoFocus
                autoComplete="username"
              />
            </div>

            <div className="form-group">
              <label htmlFor="password">
                <i className="fas fa-lock"></i>
                Password
              </label>
              <div className="input-with-icon">
                <input
                  type={showPassword ? 'text' : 'password'}
                  id="password"
                  name="password"
                  value={formData.password}
                  onChange={handleInputChange}
                  placeholder="Enter your password"
                  autoComplete="current-password"
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

            <button type="submit" className="auth-submit-btn" disabled={isLoading}>
              {isLoading ? (
                <>
                  <span className="btn-spinner"></span>
                  Signing In...
                </>
              ) : (
                <>
                  <i className="fas fa-sign-in-alt"></i>
                  Sign In
                </>
              )}
            </button>
          </form>

          <div className="auth-footer">
            {registrationMode === 'open' && (
              <p>
                Don&apos;t have an account?{' '}
                <Link to="/register" className="auth-link">
                  Create one
                </Link>
              </p>
            )}
            <p style={registrationMode === 'open' ? { marginTop: '0.5rem' } : undefined}>
              <Link to="/recover" className="auth-link">
                Forgot your password?
              </Link>
            </p>
          </div>
        </div>
      </div>
    </div>
  );
}
