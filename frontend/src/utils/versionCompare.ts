/**
 * Version comparison utility for semantic versioning
 * Handles version formats like: 1.2.3, v1.2.3, 1.2.3-beta.4, etc.
 */

/**
 * Validates if a string is a valid semantic version
 * @param version - Version string to validate
 * @returns True if valid version format
 */
export function isValidVersion(version: string): boolean {
  return /^v?\d+\.\d+\.\d+/.test(version);
}

/**
 * Parses a version string into comparable parts
 * @param version - Version string (e.g., "v0.1.0-beta.3")
 * @returns Object with major, minor, patch, and prerelease parts
 */
function parseVersion(version: string): {
  major: number;
  minor: number;
  patch: number;
  prerelease: string;
} {
  // Remove 'v' prefix if present
  const clean = version.replace(/^v/, '');

  // Split by . and -
  const parts = clean.split(/[.-]/);

  return {
    major: parseInt((parts[0] ?? '') || '0', 10),
    minor: parseInt((parts[1] ?? '') || '0', 10),
    patch: parseInt((parts[2] ?? '') || '0', 10),
    prerelease: parts.slice(3).join('.'),
  };
}

/**
 * Compares two semantic version strings
 * @param v1 - First version string
 * @param v2 - Second version string
 * @returns -1 if v1 < v2, 0 if equal, 1 if v1 > v2
 *
 * @example
 * compareVersions("0.1.0", "0.2.0") // -1 (update available)
 * compareVersions("1.0.0", "1.0.0") // 0 (equal)
 * compareVersions("v0.1.0-beta.3", "v0.1.0-beta.4") // -1
 * compareVersions("v0.1.0-beta.4", "v0.1.0") // -1 (beta < release)
 * compareVersions("v0.1.0", "v0.1.0-beta.4") // 1 (release > beta)
 */
export function compareVersions(v1: string, v2: string): number {
  if (!isValidVersion(v1) || !isValidVersion(v2)) {
    console.warn('Invalid version format:', { v1, v2 });
    return 0; // Treat as equal if invalid
  }

  const parsed1 = parseVersion(v1);
  const parsed2 = parseVersion(v2);

  // Compare major version
  if (parsed1.major > parsed2.major) {
    return 1;
  }
  if (parsed1.major < parsed2.major) {
    return -1;
  }

  // Compare minor version
  if (parsed1.minor > parsed2.minor) {
    return 1;
  }
  if (parsed1.minor < parsed2.minor) {
    return -1;
  }

  // Compare patch version
  if (parsed1.patch > parsed2.patch) {
    return 1;
  }
  if (parsed1.patch < parsed2.patch) {
    return -1;
  }

  // Handle pre-release versions
  const pre1 = parsed1.prerelease;
  const pre2 = parsed2.prerelease;

  // Release version > Pre-release version
  if (!pre1 && pre2) {
    return 1;
  }
  if (pre1 && !pre2) {
    return -1;
  }

  // Both are pre-releases, compare lexicographically
  if (pre1 && pre2) {
    return pre1.localeCompare(pre2);
  }

  // Versions are equal
  return 0;
}

/**
 * Checks if an update is available (v2 is newer than v1)
 * @param currentVersion - Current version string
 * @param latestVersion - Latest available version string
 * @returns True if latest version is newer
 */
export function isUpdateAvailable(currentVersion: string, latestVersion: string): boolean {
  return compareVersions(currentVersion, latestVersion) < 0;
}
