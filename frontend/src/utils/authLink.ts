/**
 * Invitation and password-reset links.
 *
 * The token travels in the URL fragment (`/register#invite=...`, `/reset#token=...`), never in the
 * path or query string. A browser does not send the fragment to the server, so it stays out of the
 * server's and any reverse proxy's access log and out of the Referer header. The page reads it,
 * then removes it from the address bar so it does not linger in the history, a bookmark or a
 * screenshot.
 */

/** A token as the server issues it: 32 random bytes, 43 URL-safe characters. */
const TOKEN_PATTERN = /^[A-Za-z0-9_-]{43}$/;

/**
 * The token stored under `key` in a URL fragment, or null when it is missing or not shaped like a
 * token.
 * @param hash - `window.location.hash`, with or without the leading `#`
 */
export function readFragmentToken(hash: string, key: 'invite' | 'token'): string | null {
  const params = new URLSearchParams(hash.startsWith('#') ? hash.slice(1) : hash);
  const value = params.get(key);
  return value !== null && TOKEN_PATTERN.test(value) ? value : null;
}

/** The link an admin hands to someone they invite. */
export function buildInviteLink(origin: string, token: string): string {
  return `${origin}/register#invite=${token}`;
}

/** The link an admin hands to a user who needs a new password. */
export function buildResetLink(origin: string, token: string): string {
  return `${origin}/reset#token=${token}`;
}

/** The parts of `window` that `clearUrlFragment` uses. */
interface FragmentWindow {
  history: Pick<History, 'replaceState'>;
  location: Pick<Location, 'pathname' | 'search'>;
}

/** Remove the fragment from the address bar without navigating. */
export function clearUrlFragment(win: FragmentWindow = window): void {
  win.history.replaceState(null, '', win.location.pathname + win.location.search);
}
