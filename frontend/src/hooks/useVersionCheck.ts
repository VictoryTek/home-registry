import { useState, useEffect, useCallback } from 'react';
import { compareVersions, isValidVersion } from '../utils/versionCompare';

interface GitHubRelease {
  tag_name: string;
  name: string;
  published_at: string;
  html_url: string;
  body: string;
  prerelease: boolean;
  draft: boolean;
}

interface VersionCache {
  latestVersion: string;
  checkedAt: string;
  updateAvailable: boolean;
  releaseUrl: string;
  expiresAt: string;
}

export interface VersionCheckResult {
  currentVersion: string;
  latestVersion: string | null;
  updateAvailable: boolean;
  isChecking: boolean;
  error: string | null;
  releaseUrl: string | null;
  lastChecked: string | null;
  checkForUpdates: () => Promise<void>;
}

// Configuration
const CACHE_KEY = 'versionCheck';
const CACHE_DURATION_MS = 24 * 60 * 60 * 1000; // 24 hours
const GITHUB_API_BASE = 'https://api.github.com';

// GitHub repository configuration
// Repository: https://github.com/VictoryTek/home-registry
// These values can be overridden via environment variables:
//   VITE_GITHUB_OWNER - GitHub organization/user (default: VictoryTek)
//   VITE_GITHUB_REPO - Repository name (default: home-registry)
const REPO_OWNER = ((import.meta.env.VITE_GITHUB_OWNER as string | undefined) ?? '') || 'VictoryTek';
const REPO_NAME = ((import.meta.env.VITE_GITHUB_REPO as string | undefined) ?? '') || 'home-registry';

/**
 * Get cached version check data
 */
function getCachedVersionCheck(): VersionCache | null {
  try {
    const cached = localStorage.getItem(CACHE_KEY);
    if (!cached) {
      return null;
    }

    const data = JSON.parse(cached) as VersionCache;
    const expiresAt = new Date(data.expiresAt);

    if (expiresAt < new Date()) {
      // Cache expired
      localStorage.removeItem(CACHE_KEY);
      return null;
    }

    return data;
  } catch (error) {
    console.warn('Failed to read version check cache:', error);
    localStorage.removeItem(CACHE_KEY);
    return null;
  }
}

/**
 * Set cached version check data
 */
function setCachedVersionCheck(data: Omit<VersionCache, 'expiresAt' | 'checkedAt'>): void {
  try {
    const now = new Date();
    const expiresAt = new Date(now.getTime() + CACHE_DURATION_MS);

    const cacheData: VersionCache = {
      ...data,
      checkedAt: now.toISOString(),
      expiresAt: expiresAt.toISOString(),
    };

    localStorage.setItem(CACHE_KEY, JSON.stringify(cacheData));
  } catch (error) {
    console.warn('Failed to write version check cache:', error);
  }
}

/**
 * Fetch current version from backend API
 */
async function fetchCurrentVersion(): Promise<string> {
  try {
    const response = await fetch('/health');
    if (!response.ok) {
      throw new Error(`Health check failed: ${response.status}`);
    }

    const data = await response.json() as { version?: string };
    return (data.version ?? '') || '0.0.0';
  } catch (error) {
    console.warn('Failed to fetch current version from backend:', error);
    // Fallback to hardcoded version from package.json build
    return ((import.meta.env.VITE_APP_VERSION as string | undefined) ?? '') || '0.1.0-beta.3';
  }
}

/**
 * Fetch latest release from GitHub API
 */
async function fetchLatestRelease(): Promise<{ version: string; url: string } | null> {
  try {
    const url = `${GITHUB_API_BASE}/repos/${REPO_OWNER}/${REPO_NAME}/releases/latest`;
    const response = await fetch(url, {
      headers: {
        Accept: 'application/vnd.github.v3+json',
      },
    });

    if (response.status === 404) {
      console.warn('No releases found for repository');
      return null;
    }

    if (response.status === 403) {
      const rateLimitReset = response.headers.get('X-RateLimit-Reset');
      console.warn('GitHub API rate limit exceeded', { rateLimitReset });
      throw new Error('Rate limit exceeded');
    }

    if (!response.ok) {
      throw new Error(`GitHub API returned ${response.status}`);
    }

    const data = await response.json() as GitHubRelease;

    // Skip draft releases
    if (data.draft) {
      console.warn('Latest release is a draft, skipping');
      return null;
    }

    // Skip pre-releases (beta, rc, etc.) for stable update checks
    // Users running beta versions likely follow development closely
    if (data.prerelease) {
      console.warn('Latest release is a pre-release, skipping for stable update check');
      return null;
    }

    const version = data.tag_name;
    if (!isValidVersion(version)) {
      console.warn('Invalid version format from GitHub:', version);
      return null;
    }

    return {
      version,
      url: data.html_url,
    };
  } catch (error) {
    console.warn('Failed to fetch latest release from GitHub:', error);
    return null;
  }
}

/**
 * Custom hook for version checking
 */
export function useVersionCheck(): VersionCheckResult {
  const [currentVersion, setCurrentVersion] = useState<string>('0.0.0');
  const [latestVersion, setLatestVersion] = useState<string | null>(null);
  const [updateAvailable, setUpdateAvailable] = useState<boolean>(false);
  const [isChecking, setIsChecking] = useState<boolean>(false);
  const [error, setError] = useState<string | null>(null);
  const [releaseUrl, setReleaseUrl] = useState<string | null>(null);
  const [lastChecked, setLastChecked] = useState<string | null>(null);

  const checkForUpdates = useCallback(async () => {
    setIsChecking(true);
    setError(null);

    try {
      // Fetch current version from backend
      const current = await fetchCurrentVersion();
      setCurrentVersion(current);

      // Check cache first
      const cached = getCachedVersionCheck();
      if (cached) {
        setLatestVersion(cached.latestVersion);
        setUpdateAvailable(cached.updateAvailable);
        setReleaseUrl(cached.releaseUrl);
        setLastChecked(cached.checkedAt);
        setIsChecking(false);
        return;
      }

      // Fetch latest release from GitHub
      const latest = await fetchLatestRelease();

      if (!latest) {
        // No release found or error occurred
        setLatestVersion(null);
        setUpdateAvailable(false);
        setReleaseUrl(null);
        setIsChecking(false);
        return;
      }

      // Compare versions
      const isNewer = compareVersions(current, latest.version) < 0;

      // Update state
      setLatestVersion(latest.version);
      setUpdateAvailable(isNewer);
      setReleaseUrl(latest.url);
      setLastChecked(new Date().toISOString());

      // Cache the result
      setCachedVersionCheck({
        latestVersion: latest.version,
        updateAvailable: isNewer,
        releaseUrl: latest.url,
      });
    } catch (err) {
      const errorMessage = err instanceof Error ? err.message : 'Unknown error';
      setError(errorMessage);
      console.error('Version check failed:', err);
    } finally {
      setIsChecking(false);
    }
  }, []);

  // Check on mount
  useEffect(() => {
    void checkForUpdates();
  }, [checkForUpdates]);

  return {
    currentVersion,
    latestVersion,
    updateAvailable,
    isChecking,
    error,
    releaseUrl,
    lastChecked,
    checkForUpdates,
  };
}
