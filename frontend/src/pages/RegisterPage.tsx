import { useState, useRef, useEffect } from 'react';
import { useNavigate, Link } from 'react-router-dom';
import { authApi } from '@/services/api';
import { useApp } from '@/context/AppContext';
import { clearUrlFragment, readFragmentToken } from '@/utils/authLink';
import '@/styles/auth.css';

/** Whether this visitor may fill in the form: decided by the registration mode and invitation. */
type Access = 'checking' | 'allowed' | 'invite-needed' | 'closed';

export function RegisterPage() {
  const navigate = useNavigate();
  const { showToast } = useApp();
  // The invitation is in the URL fragment, so it never reaches a server log. It is read once,
  // before it is removed from the address bar.
  const [inviteToken] = useState(() => readFragmentToken(window.location.hash, 'invite'));
  const [access, setAccess] = useState<Access>('checking');
  const [isLoading, setIsLoading] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [showPassword, setShowPassword] = useState(false);
  const [showConfirmPassword, setShowConfirmPassword] = useState(false);
  const [showRecoveryCodes, setShowRecoveryCodes] = useState(false);
  const [recoveryCodes, setRecoveryCodes] = useState<string[] | null>(null);
  const [codesConfirmed, setCodesConfirmed] = useState(false);
  const codesRef = useRef<HTMLDivElement>(null);

  const [formData, setFormData] = useState({
    username: '',
    full_name: '',
    password: '',
    confirmPassword: '',
  });

  useEffect(() => {
    clearUrlFragment();
    let cancelled = false;

    const decide = async (): Promise<Access> => {
      const status = await authApi.checkSetupStatus();
      const mode = status.data?.registration_mode;
      if (mode === 'open') {
        return 'allowed';
      }
      if (mode === 'closed') {
        return 'closed';
      }
      if (inviteToken === null) {
        return 'invite-needed';
      }
      const check = await authApi.checkInvite(inviteToken);
      return check.data?.valid === true ? 'allowed' : 'invite-needed';
    };

    decide()
      .then((result) => {
        if (!cancelled) {
          setAccess(result);
        }
      })
      .catch(() => {
        // Could not ask; let the visitor try, the server decides
        if (!cancelled) {
          setAccess('allowed');
        }
      });

    return () => {
      cancelled = true;
    };
  }, [inviteToken]);

  const handleInputChange = (e: React.ChangeEvent<HTMLInputElement>) => {
    const { name, value } = e.target;
    setFormData((prev) => ({ ...prev, [name]: value }));
    setError(null);
  };

  const validateForm = () => {
    if (!formData.username.trim()) {
      setError('Username is required');
      return false;
    }
    if (formData.username.length < 3) {
      setError('Username must be at least 3 characters');
      return false;
    }
    if (!formData.full_name.trim()) {
      setError('Full name is required');
      return false;
    }
    if (!formData.password) {
      setError('Password is required');
      return false;
    }
    if (formData.password.length < 8) {
      setError('Password must be at least 8 characters');
      return false;
    }
    if (formData.password !== formData.confirmPassword) {
      setError('Passwords do not match');
      return false;
    }
    return true;
  };

  const handleSubmit = async (e: React.FormEvent) => {
    e.preventDefault();

    if (!validateForm()) {
      return;
    }

    setIsLoading(true);
    setError(null);

    try {
      const result = await authApi.register({
        username: formData.username,
        full_name: formData.full_name,
        password: formData.password,
        invite_token: inviteToken ?? undefined,
      });

      if (result.success && result.data) {
        // Store auth data
        localStorage.setItem('home_registry_token', result.data.token);
        localStorage.setItem('home_registry_user', JSON.stringify(result.data.user));

        // Generate recovery codes
        const codesResponse = await authApi.generateRecoveryCodes(formData.password);
        if (codesResponse.success && codesResponse.data) {
          setRecoveryCodes(codesResponse.data.codes);
          setShowRecoveryCodes(true);
        } else {
          setError(codesResponse.error ?? 'Failed to generate recovery codes');
        }
      } else {
        setError(result.error ?? 'Registration failed. Please try again.');
      }
    } catch {
      console.error('Registration error');
      setError('Network error. Please check your connection and try again.');
    } finally {
      setIsLoading(false);
    }
  };

  const handleCompleteRegistration = async () => {
    if (!codesConfirmed) {
      setError('Please confirm you have saved your recovery codes');
      return;
    }

    try {
      setIsLoading(true);
      await authApi.confirmRecoveryCodes();

      // Redirect to app
      navigate('/');
      window.location.reload();
    } catch {
      setError('Failed to complete registration');
      setIsLoading(false);
    }
  };

  const downloadCodes = () => {
    if (!recoveryCodes) {
      return;
    }

    const content = `Home Registry Recovery Codes\n\nUsername: ${formData.username}\nGenerated: ${new Date().toLocaleString()}\n\nSave these codes in a secure location. Each code can only be used once.\n\n${recoveryCodes.join('\n')}\n`;
    const blob = new Blob([content], { type: 'text/plain' });
    const url = URL.createObjectURL(blob);
    const a = document.createElement('a');
    a.href = url;
    a.download = `recovery-codes-${formData.username}.txt`;
    document.body.appendChild(a);
    a.click();
    document.body.removeChild(a);
    URL.revokeObjectURL(url);
    showToast('Recovery codes downloaded!', 'success');
  };

  const copyCodes = async () => {
    if (!recoveryCodes) {
      return;
    }

    try {
      console.warn('[RegisterPage] Attempting to copy recovery codes to clipboard');
      await navigator.clipboard.writeText(recoveryCodes.join('\n'));
      console.warn('[RegisterPage] Clipboard write successful');
      showToast('Codes copied to clipboard!', 'success');
    } catch (err) {
      // CRITICAL FIX: Enhanced error logging with detailed diagnostic information
      console.error('[RegisterPage] Failed to copy codes to clipboard:', err);

      // Provide detailed error information based on error type
      if (err instanceof Error) {
        console.error('[RegisterPage] Error name:', err.name);
        console.error('[RegisterPage] Error message:', err.message);
      }

      showToast('Failed to copy codes. Please try the download button instead.', 'error');
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
        <div
          className={`auth-card ${showRecoveryCodes ? 'setup-card wide' : 'auth-card-register'}`}
        >
          <div className="auth-header">
            <div className="auth-logo">
              <img src="/logo_full.png" alt="Home Registry" className="auth-logo-img" />
            </div>
            <h1 className="auth-title">Create Account</h1>
            <p className="auth-subtitle">Join Home Registry to start organizing</p>
          </div>

          {error && (
            <div className="auth-error">
              <i className="fas fa-exclamation-circle"></i>
              <span>{error}</span>
            </div>
          )}

          {access !== 'allowed' && !showRecoveryCodes ? (
            <>
              <div className="auth-step">
                <p className="step-description">
                  {access === 'checking' && 'Checking your invitation...'}
                  {access === 'closed' &&
                    'Registration is closed. Ask an administrator to create an account for you.'}
                  {access === 'invite-needed' &&
                    'Registration is by invitation only. Ask an administrator for an invitation link, then open it. If you reloaded this page, open the link again.'}
                </p>
              </div>
              <div className="auth-footer">
                <p>
                  <Link to="/login" className="auth-link">
                    Back to sign in
                  </Link>
                </p>
              </div>
            </>
          ) : !showRecoveryCodes ? (
            <>
              <form onSubmit={handleSubmit} className="auth-form">
                <div className="form-row">
                  <div className="form-group">
                    <label htmlFor="full_name">
                      <i className="fas fa-id-card"></i>
                      Full Name
                    </label>
                    <input
                      type="text"
                      id="full_name"
                      name="full_name"
                      value={formData.full_name}
                      onChange={handleInputChange}
                      placeholder="Enter your full name"
                      autoFocus
                    />
                  </div>

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
                      placeholder="Choose a username"
                    />
                  </div>
                </div>

                <div className="form-row">
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
                        placeholder="Min 8 characters"
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
                    <label htmlFor="confirmPassword">
                      <i className="fas fa-lock"></i>
                      Confirm Password
                    </label>
                    <div className="input-with-icon">
                      <input
                        type={showConfirmPassword ? 'text' : 'password'}
                        id="confirmPassword"
                        name="confirmPassword"
                        value={formData.confirmPassword}
                        onChange={handleInputChange}
                        placeholder="Confirm password"
                      />
                      <button
                        type="button"
                        className="password-toggle"
                        onClick={() => setShowConfirmPassword(!showConfirmPassword)}
                        tabIndex={-1}
                      >
                        <i className={`fas ${showConfirmPassword ? 'fa-eye-slash' : 'fa-eye'}`}></i>
                      </button>
                    </div>
                  </div>
                </div>

                <button type="submit" className="auth-submit-btn" disabled={isLoading}>
                  {isLoading ? (
                    <>
                      <span className="btn-spinner"></span>
                      Creating Account...
                    </>
                  ) : (
                    <>
                      <i className="fas fa-user-plus"></i>
                      Create Account
                    </>
                  )}
                </button>
              </form>

              <div className="auth-footer">
                <p>
                  Already have an account?{' '}
                  <Link to="/login" className="auth-link">
                    Sign in
                  </Link>
                </p>
              </div>
            </>
          ) : (
            <div className="auth-step">
              <h2 style={{ marginBottom: '0.5rem' }}>Save Your Recovery Codes</h2>
              <p
                className="step-description"
                style={{ marginBottom: '0.75rem', fontSize: '0.9rem' }}
              >
                These codes can be used to recover your account if you forget your password. Each
                code can only be used once.
              </p>

              <div className="recovery-codes-display" ref={codesRef}>
                <div className="codes-grid">
                  {recoveryCodes?.map((code, index) => (
                    <div key={index} className="code-item">
                      {code}
                    </div>
                  ))}
                </div>
              </div>

              <div className="recovery-actions">
                <button type="button" className="btn-secondary" onClick={downloadCodes}>
                  📥 Download
                </button>
                <button type="button" className="btn-secondary" onClick={copyCodes}>
                  📋 Copy All
                </button>
              </div>

              <div className="form-group" style={{ marginBottom: '0.75rem' }}>
                <label className="checkbox-label">
                  <input
                    type="checkbox"
                    checked={codesConfirmed}
                    onChange={(e) => setCodesConfirmed(e.target.checked)}
                  />
                  <span>I have saved these recovery codes in a secure location</span>
                </label>
              </div>

              <div className="auth-warning">
                <strong>⚠️ Important:</strong> You will not be able to see these codes again. Make
                sure to save them before continuing.
              </div>

              <button
                type="button"
                className="auth-submit-btn"
                onClick={handleCompleteRegistration}
                disabled={isLoading || !codesConfirmed}
                style={{ marginTop: '1rem' }}
              >
                {isLoading ? (
                  <>
                    <span className="btn-spinner"></span>
                    Finishing...
                  </>
                ) : (
                  <>
                    <i className="fas fa-check"></i>
                    Continue to Home Registry
                  </>
                )}
              </button>
            </div>
          )}
        </div>
      </div>
    </div>
  );
}
