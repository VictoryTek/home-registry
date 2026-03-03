import { useEffect } from 'react';
import { VersionDisplay } from './VersionDisplay';

interface SidebarProps {
  currentPage: string;
  onNavigate: (page: string) => void;
  isOpen: boolean;
  onClose: () => void;
}

export function Sidebar({ currentPage, onNavigate, isOpen, onClose }: SidebarProps) {
  // Close sidebar on window resize above mobile breakpoint
  useEffect(() => {
    const handleResize = () => {
      if (window.innerWidth > 768 && isOpen) {
        onClose();
      }
    };

    window.addEventListener('resize', handleResize);
    return () => window.removeEventListener('resize', handleResize);
  }, [isOpen, onClose]);

  // Close sidebar on Escape key press
  useEffect(() => {
    const handleKeyDown = (event: KeyboardEvent) => {
      if (event.key === 'Escape' && isOpen) {
        onClose();
      }
    };

    document.addEventListener('keydown', handleKeyDown);
    return () => document.removeEventListener('keydown', handleKeyDown);
  }, [isOpen, onClose]);

  // Lock body scroll when sidebar is open on mobile
  useEffect(() => {
    if (isOpen) {
      document.body.style.overflow = 'hidden';
    } else {
      document.body.style.overflow = '';
    }
    return () => {
      document.body.style.overflow = '';
    };
  }, [isOpen]);

  const handleNavigate = (page: string) => {
    onNavigate(page);
    onClose(); // Auto-close sidebar on navigation (mobile)
  };

  return (
    <>
      {/* Backdrop overlay */}
      <div
        className={`sidebar-backdrop ${isOpen ? 'visible' : ''}`}
        onClick={onClose}
        aria-hidden="true"
      />

      <aside className={`sidebar ${isOpen ? 'open' : ''}`}>
        <div className="sidebar-header">
          <a href="/" className="logo">
            <img src="/logo_full.png" alt="Home Registry" />
          </a>
        </div>

        <VersionDisplay />

        <nav className="nav-menu">
          <div className="nav-section">
            <div className="nav-section-title">Overview</div>
            <button
              className={`nav-item ${currentPage === 'inventories' ? 'active' : ''}`}
              onClick={() => handleNavigate('inventories')}
            >
              <i className="fas fa-warehouse"></i>
              <span>Inventories</span>
            </button>
            <button
              className={`nav-item ${currentPage === 'organizers' ? 'active' : ''}`}
              onClick={() => handleNavigate('organizers')}
            >
              <i className="fas fa-folder-tree"></i>
              <span>Organizers</span>
            </button>
          </div>

          <div className="sidebar-bottom">
            <button
              className={`nav-item nav-item-notifications ${currentPage === 'notifications' ? 'active' : ''}`}
              onClick={() => handleNavigate('notifications')}
            >
              <i className="fas fa-bell"></i>
              <span>Notifications</span>
            </button>

            <div className="nav-section system-section">
              <div className="nav-section-title">System</div>
              <button
                className={`nav-item ${currentPage === 'settings' ? 'active' : ''}`}
                onClick={() => handleNavigate('settings')}
              >
                <i className="fas fa-cog"></i>
                <span>Settings</span>
              </button>
            </div>
          </div>
        </nav>
      </aside>
    </>
  );
}
