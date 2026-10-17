import { useState } from 'react';
import { useNavigate } from 'react-router-dom';
import { authApi } from '@/services/api';
import { useApp } from '@/context/AppContext';
import '@/styles/auth.css';

export function SetupPage() {
  const navigate = useNavigate();
  const { showToast } = useApp();
  const [step, setStep] = useState(1);
  const [isLoading, setIsLoading] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [recoveryCodes, setRecoveryCodes] = useState<string[] | null>(null);
  const [codesConfirmed, setCodesConfirmed] = useState(false);

  const [formData, setFormData] = useState({
    username: '',
    full_name: '',
    password: '',
    confirmPassword: '',
  });

  const handleInputChange = (e: React.ChangeEvent<HTMLInputElement>) => {
    const { name, value } = e.target;
    setFormData((prev) => ({ ...prev, [name]: value }));
    setError(null);
  };

  const validateStep1 = () => {
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
    return true;
  };

  const validateStep2 = () => {
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

  const handleNext = () => {
    if (step === 1 && validateStep1()) {
      setStep(2);
    } else if (step === 2 && validateStep2()) {
      // Step 2 validation passes, now call setup and move to recovery codes
      void completeSetup();
    }
  };

  const handleBack = () => {
    setError(null);
    setStep((prev) => Math.max(1, prev - 1));
  };

  const handleSubmit = (e: React.FormEvent) => {
    e.preventDefault();
    // Prevent form submission - use button handlers instead
  };

  const completeSetup = async () => {
    setIsLoading(true);
    setError(null);

    try {
      const result = await authApi.setup({
        username: formData.username,
        full_name: formData.full_name,
        password: formData.password,
      });

      if (result.success && result.data) {
        // Store auth data
        localStorage.setItem('home_registry_token', result.data.token);
        localStorage.setItem('home_registry_user', JSON.stringify(result.data.user));

        // Generate recovery codes
        const codesResponse = await authApi.generateRecoveryCodes(formData.password);
        if (codesResponse.success && codesResponse.data) {
          setRecoveryCodes(codesResponse.data.codes);
          // Move to recovery codes step
          setStep(3);
        } else {
          setError(codesResponse.error ?? 'Failed to generate recovery codes');
        }
      } else {
        setError(result.error ?? 'Setup failed. Please try again.');
      }
    } catch (err) {
      console.error('Setup error:', err);
      setError('Network error. Please check your connection and try again.');
    } finally {
      setIsLoading(false);
    }
  };

  const handleCompleteSetup = async () => {
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
      setError('Failed to complete setup');
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
      console.warn('[SetupPage] Attempting to copy recovery codes to clipboard');
      await navigator.clipboard.writeText(recoveryCodes.join('\n'));
      console.warn('[SetupPage] Clipboard write successful');
      showToast('Codes copied to clipboard!', 'success');
    } catch (err) {
      // CRITICAL FIX: Capture and log the actual error object for debugging
      console.error('[SetupPage] Failed to copy codes to clipboard:', err);

      // Provide detailed error information based on error type
      if (err instanceof Error) {
        console.error('[SetupPage] Error name:', err.name);
        console.error('[SetupPage] Error message:', err.message);
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
        <div className={`auth-card setup-card ${step === 3 ? 'wide' : ''}`}>
          <div className="auth-header">
            <div className="auth-logo">
              <img src="/logo_full.png" alt="Home Registry" className="auth-logo-img" />
            </div>
            <p className="auth-subtitle">Welcome! Let's set up your account.</p>
          </div>

          {/* Progress indicator */}
          <div className="setup-progress">
            <div
              className={`progress-step ${step >= 1 ? 'active' : ''} ${step > 1 ? 'completed' : ''}`}
            >
              <div className="step-number">1</div>
              <span>Account</span>
            </div>
            <div className="progress-line"></div>
            <div
              className={`progress-step ${step >= 2 ? 'active' : ''} ${step > 2 ? 'completed' : ''}`}
            >
              <div className="step-number">2</div>
              <span>Security</span>
            </div>
            <div className="progress-line"></div>
            <div className={`progress-step ${step >= 3 ? 'active' : ''}`}>
              <div className="step-number">3</div>
              <span>Recovery</span>
            </div>
          </div>

          {error && (
            <div className="auth-error">
              <span className="error-icon">⚠️</span>
              {error}
            </div>
          )}

          <form onSubmit={handleSubmit}>
            {/* Step 1: Account Details */}
            {step === 1 && (
              <div className="auth-step">
                <h2>Create Admin Account</h2>
                <p className="step-description">
                  This will be the administrator account for your Home Registry.
                </p>

                <div className="form-group">
                  <label htmlFor="full_name">Full Name</label>
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
                  <label htmlFor="username">Username</label>
                  <input
                    type="text"
                    id="username"
                    name="username"
                    value={formData.username}
                    onChange={handleInputChange}
                    placeholder="Enter username"
                  />
                </div>
              </div>
            )}

            {/* Step 2: Password */}
            {step === 2 && (
              <div className="auth-step">
                <h2>Set Password</h2>
                <p className="step-description">Choose a strong password to secure your account.</p>

                <div className="form-group">
                  <label htmlFor="password">Password</label>
                  <input
                    type="password"
                    id="password"
                    name="password"
                    value={formData.password}
                    onChange={handleInputChange}
                    placeholder="Enter password (min 8 characters)"
                    autoFocus
                  />
                </div>

                <div className="form-group">
                  <label htmlFor="confirmPassword">Confirm Password</label>
                  <input
                    type="password"
                    id="confirmPassword"
                    name="confirmPassword"
                    value={formData.confirmPassword}
                    onChange={handleInputChange}
                    placeholder="Confirm your password"
                  />
                </div>
              </div>
            )}

            {/* Step 3: Recovery Codes */}
            {step === 3 && recoveryCodes && (
              <div className="auth-step">
                <h2 style={{ marginBottom: '0.5rem' }}>Save Your Recovery Codes</h2>
                <p
                  className="step-description"
                  style={{ marginBottom: '0.75rem', fontSize: '0.9rem' }}
                >
                  These codes can be used to recover your account if you forget your password. Each
                  code can only be used once.
                </p>

                <div className="recovery-codes-display">
                  <div className="codes-grid">
                    {recoveryCodes.map((code, index) => (
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
              </div>
            )}

            <div className="auth-actions">
              {step === 2 && (
                <button type="button" className="btn-secondary" onClick={handleBack}>
                  Back
                </button>
              )}

              {step === 1 ? (
                <button type="button" className="btn-primary" onClick={handleNext}>
                  Next
                </button>
              ) : step === 2 ? (
                <button
                  type="button"
                  className="btn-primary"
                  onClick={handleNext}
                  disabled={isLoading}
                >
                  {isLoading ? 'Creating Account...' : 'Complete Setup'}
                </button>
              ) : (
                <button
                  type="button"
                  className="btn-primary"
                  onClick={handleCompleteSetup}
                  disabled={isLoading || !codesConfirmed}
                >
                  {isLoading ? 'Finishing...' : 'Finish Setup'}
                </button>
              )}
            </div>
          </form>
        </div>
      </div>
    </div>
  );
}
