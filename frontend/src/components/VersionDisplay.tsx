import { useState } from 'react';
import { useVersionCheck } from '../hooks/useVersionCheck';
import { AboutDialog } from './AboutDialog';

export function VersionDisplay() {
  const [showAboutDialog, setShowAboutDialog] = useState(false);
  const versionCheck = useVersionCheck();

  const handleVersionClick = () => {
    setShowAboutDialog(true);
  };

  const handleUpdateClick = (e: React.MouseEvent) => {
    e.stopPropagation(); // Prevent opening About dialog
    if (versionCheck.releaseUrl) {
      window.open(versionCheck.releaseUrl, '_blank', 'noopener,noreferrer');
    }
  };

  const handleCheckForUpdates = async () => {
    await versionCheck.checkForUpdates();
  };

  return (
    <>
      <div className="version-display">
        <div
          className="version-text"
          onClick={handleVersionClick}
          role="button"
          tabIndex={0}
          onKeyDown={(e) => {
            if (e.key === 'Enter' || e.key === ' ') {
              e.preventDefault();
              handleVersionClick();
            }
          }}
          aria-label={`Current version ${versionCheck.currentVersion}${versionCheck.updateAvailable ? '. Update available' : ''}. Click for more information.`}
        >
          v{versionCheck.currentVersion}
          {versionCheck.updateAvailable && (
            <span style={{ color: 'var(--accent-color)', fontWeight: 600 }}> (Update)</span>
          )}
        </div>
        {versionCheck.updateAvailable && versionCheck.releaseUrl && (
          <div
            className="update-indicator"
            onClick={handleUpdateClick}
            role="button"
            tabIndex={0}
            onKeyDown={(e) => {
              if (e.key === 'Enter' || e.key === ' ') {
                e.preventDefault();
                handleUpdateClick(e as unknown as React.MouseEvent);
              }
            }}
            aria-label={`Update available: version ${versionCheck.latestVersion}. Click to view release.`}
          >
            <span>Update Available</span>
            <i className="fas fa-external-link-alt icon"></i>
          </div>
        )}
      </div>

      <AboutDialog
        isOpen={showAboutDialog}
        onClose={() => setShowAboutDialog(false)}
        currentVersion={versionCheck.currentVersion}
        latestVersion={versionCheck.latestVersion}
        releaseUrl={versionCheck.releaseUrl}
        onCheckForUpdates={handleCheckForUpdates}
        isChecking={versionCheck.isChecking}
        lastChecked={versionCheck.lastChecked}
      />
    </>
  );
}
