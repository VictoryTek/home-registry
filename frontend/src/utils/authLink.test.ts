import { describe, expect, it, vi } from 'vitest';
import { buildInviteLink, buildResetLink, clearUrlFragment, readFragmentToken } from './authLink';

const TOKEN = 'abcdefghijklmnopqrstuvwxyzABCDEFGHIJKLMNOPQ'; // 43 characters

describe('reading a token from the URL fragment', () => {
  it('reads it with or without the leading #', () => {
    expect(TOKEN).toHaveLength(43);
    expect(readFragmentToken(`#invite=${TOKEN}`, 'invite')).toBe(TOKEN);
    expect(readFragmentToken(`invite=${TOKEN}`, 'invite')).toBe(TOKEN);
    expect(readFragmentToken(`#token=${TOKEN}`, 'token')).toBe(TOKEN);
  });

  it('accepts the whole URL-safe alphabet', () => {
    const odd = `${'-_'.repeat(10)}${'AZaz09'.repeat(3)}-_-_-`;
    expect(odd).toHaveLength(43);
    expect(readFragmentToken(`#invite=${odd}`, 'invite')).toBe(odd);
  });

  it('ignores other parameters and the wrong key', () => {
    expect(readFragmentToken(`#utm=1&invite=${TOKEN}&x=2`, 'invite')).toBe(TOKEN);
    expect(readFragmentToken(`#token=${TOKEN}`, 'invite')).toBeNull();
    expect(readFragmentToken(`#invite=${TOKEN}`, 'token')).toBeNull();
  });

  it('returns null for anything that is not shaped like a token', () => {
    for (const hash of [
      '',
      '#',
      '#invite=',
      '#invite',
      `#invite=${TOKEN.slice(1)}`,
      `#invite=${TOKEN}x`,
      `#invite=${TOKEN.slice(1)}+`,
      `#invite=${TOKEN.slice(1)}%20`,
      '#invite=<script>',
    ]) {
      expect(readFragmentToken(hash, 'invite'), hash).toBeNull();
    }
  });
});

describe('building links', () => {
  it('puts the token in the fragment, not the path or query string', () => {
    const invite = buildInviteLink('https://home.example', TOKEN);
    const reset = buildResetLink('https://home.example', TOKEN);
    expect(invite).toBe(`https://home.example/register#invite=${TOKEN}`);
    expect(reset).toBe(`https://home.example/reset#token=${TOKEN}`);
    for (const link of [invite, reset]) {
      const url = new URL(link);
      expect(url.search).toBe('');
      expect(url.pathname).not.toContain(TOKEN);
      expect(url.hash).toContain(TOKEN);
    }
  });

  it('builds links that read back', () => {
    const invite = new URL(buildInviteLink('https://h.example', TOKEN));
    const reset = new URL(buildResetLink('https://h.example', TOKEN));
    expect(readFragmentToken(invite.hash, 'invite')).toBe(TOKEN);
    expect(readFragmentToken(reset.hash, 'token')).toBe(TOKEN);
  });
});

describe('clearing the fragment', () => {
  it('replaces the history entry with the same path and query, without the fragment', () => {
    const replaceState = vi.fn();
    clearUrlFragment({
      history: { replaceState },
      location: { pathname: '/register', search: '?lang=en' },
    });
    expect(replaceState).toHaveBeenCalledTimes(1);
    expect(replaceState).toHaveBeenCalledWith(null, '', '/register?lang=en');
  });
});
