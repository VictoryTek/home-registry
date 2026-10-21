import { renderToStaticMarkup } from 'react-dom/server';
import { StaticRouter } from 'react-router-dom/server';
import { describe, expect, it, vi } from 'vitest';
import { Sidebar } from './Sidebar';

// The version box needs the app's context and has nothing to do with the search box
vi.mock('./VersionDisplay', () => ({ VersionDisplay: () => null }));

function render(location = '/'): string {
  return renderToStaticMarkup(
    <StaticRouter location={location}>
      <Sidebar
        currentPage="inventories"
        onNavigate={() => undefined}
        isOpen={false}
        onClose={() => undefined}
      />
    </StaticRouter>
  );
}

describe('the sidebar search box', () => {
  it('is a search landmark with a name, and its input has a label', () => {
    const html = render();
    expect(html).toMatch(/<form[^>]*role="search"[^>]*aria-label="Search all items"/);
    expect(html).toContain('<label for="sidebar-search-input"');
    expect(html).toContain('id="sidebar-search-input"');
    expect(html).toContain('aria-label="Search"');
  });
});
