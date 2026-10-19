import { useEffect, useState, type FormEvent } from 'react';
import { useLocation, useNavigate, useSearchParams } from 'react-router-dom';
import { SEARCH_TERM_MAX, normalizeTerm } from '@/utils/itemFilter';
import { VersionDisplay } from './VersionDisplay';

interface SidebarProps {
  currentPage: string;
  onNavigate: (page: string) => void;
  isOpen: boolean;
  onClose: () => void;
}

export function Sidebar({ currentPage, onNavigate, isOpen, onClose }: SidebarProps) {
  const navigate = useNavigate();
  const location = useLocation();
  const [params] = useSearchParams();
  const [searchText, setSearchText] = useState('');

  // On the search page the box shows the search that is being displayed
  const onSearchPage = location.pathname === '/search';
  const shownTerm = params.get('q') ?? '';
  useEffect(() => {
    if (onSearchPage) {
      setSearchText(shownTerm);
    }
  }, [onSearchPage, shownTerm]);

  const handleSearch = (event: FormEvent) => {
    event.preventDefault();
    const term = normalizeTerm(searchText);
    if (term === '') {
      return;
    }
    navigate(`/search?q=${encodeURIComponent(term)}`);
    onClose(); // Auto-close sidebar on navigation (mobile)
  };

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

        <form className="sidebar-search" role="search" onSubmit={handleSearch}>
          <label htmlFor="sidebar-search-input" className="visually-hidden">
            Search all items
          </label>
          <input
            id="sidebar-search-input"
            type="search"
            value={searchText}
            maxLength={SEARCH_TERM_MAX}
            placeholder="Search all items"
            autoComplete="off"
            onChange={(e) => setSearchText(e.target.value)}
          />
          <button type="submit" aria-label="Search">
            <i className="fas fa-search" aria-hidden="true"></i>
          </button>
        </form>

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
