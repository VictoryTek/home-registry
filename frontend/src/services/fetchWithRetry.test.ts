import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import { fetchWithRetry, retriesAfterNetworkError } from './api';

const fetchMock = vi.fn<typeof fetch>();

function ok(): Response {
  return new Response('{}', { status: 200 });
}

/** Run a request to the end, letting the backoff timers fire. */
async function settle<T>(request: Promise<T>): Promise<PromiseSettledResult<T>> {
  const result = Promise.allSettled([request]).then(([r]) => r as PromiseSettledResult<T>);
  await vi.runAllTimersAsync();
  return result;
}

beforeEach(() => {
  vi.useFakeTimers();
  vi.stubGlobal('fetch', fetchMock);
  vi.spyOn(console, 'warn').mockImplementation(() => undefined);
  vi.spyOn(console, 'error').mockImplementation(() => undefined);
});

afterEach(() => {
  vi.useRealTimers();
  vi.unstubAllGlobals();
  vi.restoreAllMocks();
  fetchMock.mockReset();
});

describe('which requests are repeated after a network error', () => {
  it('only the ones that are safe to repeat', () => {
    for (const method of ['GET', 'HEAD', 'PUT', 'DELETE', 'OPTIONS', 'get', 'Delete']) {
      expect(retriesAfterNetworkError(method), method).toBe(true);
    }
    // No method means GET
    expect(retriesAfterNetworkError(undefined)).toBe(true);
    for (const method of ['POST', 'PATCH', 'post', 'CONNECT', 'TRACE', 'CUSTOM']) {
      expect(retriesAfterNetworkError(method), method).toBe(false);
    }
  });
});

describe('fetchWithRetry after a network error', () => {
  it('does not repeat a POST: the server may already have done it', async () => {
    fetchMock.mockRejectedValue(new TypeError('Failed to fetch'));
    const result = await settle(fetchWithRetry('/api/items', { method: 'POST', body: '{}' }));
    expect(result.status).toBe('rejected');
    expect(fetchMock).toHaveBeenCalledTimes(1);
  });

  it('does not repeat a PATCH either', async () => {
    fetchMock.mockRejectedValue(new TypeError('Failed to fetch'));
    const result = await settle(fetchWithRetry('/api/x', { method: 'PATCH' }));
    expect(result.status).toBe('rejected');
    expect(fetchMock).toHaveBeenCalledTimes(1);
  });

  it.each(['GET', 'HEAD', 'PUT', 'DELETE', 'OPTIONS'])('repeats a %s', async (method) => {
    fetchMock.mockRejectedValueOnce(new TypeError('Failed to fetch')).mockResolvedValueOnce(ok());
    const result = await settle(fetchWithRetry('/api/x', { method }));
    expect(result.status).toBe('fulfilled');
    expect(fetchMock).toHaveBeenCalledTimes(2);
  });

  it('repeats a request without a method (a GET)', async () => {
    fetchMock.mockRejectedValueOnce(new TypeError('Failed to fetch')).mockResolvedValueOnce(ok());
    const result = await settle(fetchWithRetry('/api/x', {}));
    expect(result.status).toBe('fulfilled');
    expect(fetchMock).toHaveBeenCalledTimes(2);
  });

  it('still gives up on a GET after the last retry', async () => {
    fetchMock.mockRejectedValue(new TypeError('Failed to fetch'));
    const result = await settle(fetchWithRetry('/api/x', {}, 2));
    expect(result.status).toBe('rejected');
    expect(fetchMock).toHaveBeenCalledTimes(3);
  });
});

describe('fetchWithRetry keeps its other retries for every method', () => {
  it('repeats a POST that was rate limited (the server did not run it)', async () => {
    fetchMock
      .mockResolvedValueOnce(new Response('{}', { status: 429, headers: { 'Retry-After': '1' } }))
      .mockResolvedValueOnce(ok());
    const result = await settle(fetchWithRetry('/api/items', { method: 'POST', body: '{}' }));
    expect(result.status).toBe('fulfilled');
    expect(fetchMock).toHaveBeenCalledTimes(2);
  });

  it('repeats a POST once with a newer token after a 401', async () => {
    vi.stubGlobal('localStorage', { getItem: () => 'new-token' });
    fetchMock
      .mockResolvedValueOnce(new Response('{}', { status: 401 }))
      .mockResolvedValueOnce(ok());
    const result = await settle(
      fetchWithRetry('/api/items', {
        method: 'POST',
        headers: { Authorization: 'Bearer old-token' },
      })
    );
    expect(result.status).toBe('fulfilled');
    expect(fetchMock).toHaveBeenCalledTimes(2);
    const retried = fetchMock.mock.calls[1]?.[1]?.headers as Record<string, string>;
    expect(retried.Authorization).toBe('Bearer new-token');
  });
});
