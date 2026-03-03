import { useState } from 'react';
import { Modal } from './Modal';
import { useApp } from '@/context/AppContext';

interface AboutDialogProps {
  isOpen: boolean;
  onClose: () => void;
  currentVersion: string;
  latestVersion: string | null;
  releaseUrl: string | null;
  onCheckForUpdates: () => Promise<void>;
  isChecking?: boolean;
  lastChecked?: string | null;
}

export function AboutDialog({
  isOpen,
  onClose,
  currentVersion,
  latestVersion,
  releaseUrl,
  onCheckForUpdates,
  isChecking = false,
  lastChecked,
}: AboutDialogProps) {
  const { theme } = useApp();
  const [checkMessage, setCheckMessage] = useState<string | null>(null);
  const [checkMessageType, setCheckMessageType] = useState<'success' | 'error' | null>(null);

  // GitHub repository URL - can be overridden via VITE_GITHUB_OWNER and VITE_GITHUB_REPO
  const githubRepoUrl = `https://github.com/${import.meta.env.VITE_GITHUB_OWNER || 'VictoryTek'}/${import.meta.env.VITE_GITHUB_REPO || 'home-registry'}`;

  // Format the last checked timestamp
  const formatLastChecked = (timestamp: string | null): string => {
    if (!timestamp) {
      return 'Never';
    }

    try {
      const date = new Date(timestamp);
      const now = new Date();
      const diffMs = now.getTime() - date.getTime();
      const diffMins = Math.floor(diffMs / 60000);
      const diffHours = Math.floor(diffMs / 3600000);
      const diffDays = Math.floor(diffMs / 86400000);

      if (diffMins < 1) {
        return 'Just now';
      }
      if (diffMins < 60) {
        return `${diffMins} ${diffMins === 1 ? 'minute' : 'minutes'} ago`;
      }
      if (diffHours < 24) {
        return `${diffHours} ${diffHours === 1 ? 'hour' : 'hours'} ago`;
      }
      if (diffDays < 7) {
        return `${diffDays} ${diffDays === 1 ? 'day' : 'days'} ago`;
      }

      return date.toLocaleDateString();
    } catch {
      return 'Unknown';
    }
  };

  const handleCheckForUpdates = async () => {
    setCheckMessage(null);
    setCheckMessageType(null);

    try {
      await onCheckForUpdates();

      // Check results after update completes
      setTimeout(() => {
        if (latestVersion && latestVersion !== currentVersion) {
          // Update is available - no message needed (visual display handles it)
          setCheckMessage(null);
        } else {
          // Up to date
          setCheckMessage("You're up to date! Running the latest version.");
          setCheckMessageType('success');

          // Auto-clear after 5 seconds
          setTimeout(() => {
            setCheckMessage(null);
            setCheckMessageType(null);
          }, 5000);
        }
      }, 100);
    } catch {
      setCheckMessage('Unable to check for updates. Please try again later.');
      setCheckMessageType('error');
    }
  };

  const footer = (
    <div className="modal-actions">
      <button className="btn btn-secondary" onClick={handleCheckForUpdates} disabled={isChecking}>
        {isChecking ? (
          <>
            <i className="fas fa-spinner fa-spin"></i>
            Checking...
          </>
        ) : (
          <>
            <i className="fas fa-sync-alt"></i>
            Check for Updates
          </>
        )}
      </button>

      {checkMessage && (
        <div
          style={{
            fontSize: '0.85rem',
            padding: '0.5rem 0.75rem',
            borderRadius: 'var(--radius-sm)',
            background:
              checkMessageType === 'success' ? 'rgba(34, 197, 94, 0.1)' : 'rgba(239, 68, 68, 0.1)',
            color: checkMessageType === 'success' ? 'rgb(34, 197, 94)' : 'rgb(239, 68, 68)',
            display: 'flex',
            alignItems: 'center',
            gap: '0.5rem',
          }}
        >
          <i
            className={`fas fa-${checkMessageType === 'success' ? 'check-circle' : 'exclamation-triangle'}`}
          ></i>
          {checkMessage}
        </div>
      )}

      <button className="btn btn-primary" onClick={onClose}>
        Close
      </button>
    </div>
  );

  return (
    <Modal
      isOpen={isOpen}
      onClose={onClose}
      title="About"
      subtitle="Application Information & Updates"
      footer={footer}
      maxWidth="550px"
    >
      <div style={{ padding: '0.5rem 0' }}>
        {/* Header Section */}
        <div style={{ textAlign: 'center', marginBottom: '2rem' }}>
          <div style={{ display: 'flex', justifyContent: 'center', marginBottom: '0.75rem' }}>
            <img
              src={theme === 'light' ? '/logo_full3.png' : '/logo_full.png'}
              alt="Home Registry"
              style={{
                height: '64px',
                width: 'auto',
                filter: 'drop-shadow(0 4px 12px rgba(59, 130, 246, 0.2))',
              }}
            />
          </div>
          <p
            style={{ margin: '0.5rem 0 0 0', color: 'var(--text-secondary)', fontSize: '0.95rem' }}
          >
            Universal Home Inventory Management System
          </p>
        </div>

        {/* Version Information */}
        <div style={{ marginBottom: '2rem' }}>
          <div
            style={{
              display: 'flex',
              justifyContent: 'space-between',
              alignItems: 'center',
              padding: '1rem',
              background: 'var(--bg-secondary)',
              borderRadius: 'var(--radius-md)',
              border: '1px solid var(--border-color)',
            }}
          >
            <div>
              <div
                style={{
                  fontSize: '0.875rem',
                  color: 'var(--text-secondary)',
                  marginBottom: '0.25rem',
                }}
              >
                Current Version
              </div>
              <div style={{ fontSize: '1.25rem', fontWeight: 600, fontFamily: 'monospace' }}>
                {currentVersion}
              </div>
              {lastChecked && (
                <div
                  style={{
                    fontSize: '0.75rem',
                    color: 'var(--text-tertiary)',
                    marginTop: '0.25rem',
                  }}
                >
                  Last checked: {formatLastChecked(lastChecked)}
                </div>
              )}
            </div>
            {latestVersion && latestVersion !== currentVersion && (
              <div style={{ textAlign: 'right' }}>
                <div
                  style={{
                    fontSize: '0.875rem',
                    color: 'var(--text-secondary)',
                    marginBottom: '0.25rem',
                  }}
                >
                  Latest Version
                </div>
                <div
                  style={{
                    fontSize: '1.25rem',
                    fontWeight: 600,
                    fontFamily: 'monospace',
                    color: 'var(--accent-color)',
                  }}
                >
                  {latestVersion}
                </div>
                {releaseUrl && (
                  <a
                    href={releaseUrl}
                    target="_blank"
                    rel="noopener noreferrer"
                    style={{
                      fontSize: '0.8rem',
                      color: 'var(--accent-color)',
                      textDecoration: 'none',
                      display: 'inline-flex',
                      alignItems: 'center',
                      gap: '0.25rem',
                      marginTop: '0.25rem',
                    }}
                  >
                    View Release{' '}
                    <i className="fas fa-external-link-alt" style={{ fontSize: '0.7rem' }}></i>
                  </a>
                )}
              </div>
            )}
          </div>
        </div>

        {/* Divider */}
        <div
          style={{
            height: '1px',
            background: 'var(--border-color)',
            margin: '1.5rem 0',
          }}
        />

        {/* Project Information */}
        <div style={{ marginBottom: '1.5rem' }}>
          <div
            style={{
              display: 'flex',
              alignItems: 'center',
              gap: '0.5rem',
              marginBottom: '0.75rem',
            }}
          >
            <i
              className="fas fa-balance-scale"
              style={{ color: 'var(--text-secondary)', width: '1.25rem' }}
            ></i>
            <span style={{ fontSize: '0.9rem' }}>
              <strong>License:</strong> MIT License
            </span>
          </div>
          <div style={{ display: 'flex', alignItems: 'center', gap: '0.5rem' }}>
            <i
              className="fab fa-github"
              style={{ color: 'var(--text-secondary)', width: '1.25rem' }}
            ></i>
            <span style={{ fontSize: '0.9rem' }}>
              <strong>GitHub:</strong>{' '}
              <a
                href={githubRepoUrl}
                target="_blank"
                rel="noopener noreferrer"
                style={{
                  color: 'var(--accent-color)',
                  textDecoration: 'none',
                }}
              >
                {githubRepoUrl.replace('https://github.com/', '')}
                <i
                  className="fas fa-external-link-alt"
                  style={{ fontSize: '0.7rem', marginLeft: '0.25rem' }}
                ></i>
              </a>
            </span>
          </div>
        </div>

        {/* Divider */}
        <div
          style={{
            height: '1px',
            background: 'var(--border-color)',
            margin: '1.5rem 0',
          }}
        />

        {/* Tech Stack */}
        <div style={{ marginBottom: '1.5rem' }}>
          <h4
            style={{
              fontSize: '0.95rem',
              fontWeight: 600,
              marginBottom: '0.75rem',
              color: 'var(--text-secondary)',
            }}
          >
            Built With:
          </h4>
          <ul style={{ margin: 0, paddingLeft: '1.25rem', fontSize: '0.9rem', lineHeight: '1.8' }}>
            <li>Rust + Actix-Web (Backend)</li>
            <li>React + TypeScript (Frontend)</li>
            <li>PostgreSQL (Database)</li>
          </ul>
        </div>
      </div>
    </Modal>
  );
}
