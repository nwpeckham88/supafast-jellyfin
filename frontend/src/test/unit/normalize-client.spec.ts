import { afterEach, describe, expect, it, vi } from 'vitest';

import { request } from '$lib/api/client';

describe('request', () => {
  afterEach(() => {
    vi.restoreAllMocks();
  });

  it('returns data for successful envelopes', async () => {
    vi.stubGlobal(
      'fetch',
      vi.fn().mockResolvedValue({
        ok: true,
        status: 200,
        json: async () => ({
          success: true,
          data: { value: 42 },
          error: null
        })
      })
    );

    const result = await request<{ value: number }>('/api/test');
    expect(result.value).toBe(42);
  });

  it('throws when envelope marks failure', async () => {
    vi.stubGlobal(
      'fetch',
      vi.fn().mockResolvedValue({
        ok: false,
        status: 400,
        json: async () => ({
          success: false,
          data: null,
          error: { code: 'invalid_request', message: 'nope' }
        })
      })
    );

    await expect(request('/api/test')).rejects.toThrow('nope');
  });

  it('accepts valid falsy data payloads', async () => {
    vi.stubGlobal(
      'fetch',
      vi.fn().mockResolvedValue({
        ok: true,
        status: 200,
        json: async () => ({
          success: true,
          data: false,
          error: null
        })
      })
    );

    const result = await request<boolean>('/api/test');
    expect(result).toBe(false);
  });

  it('retries once for transient 5xx errors', async () => {
    const fetchMock = vi
      .fn()
      .mockResolvedValueOnce({
        ok: false,
        status: 503,
        json: async () => ({
          success: false,
          data: null,
          error: { code: 'service_unavailable', message: 'try again' }
        })
      })
      .mockResolvedValueOnce({
        ok: true,
        status: 200,
        json: async () => ({
          success: true,
          data: { value: 7 },
          error: null
        })
      });

    vi.stubGlobal('fetch', fetchMock);

    const result = await request<{ value: number }>('/api/test');
    expect(result.value).toBe(7);
    expect(fetchMock).toHaveBeenCalledTimes(2);
  });

  it('does not retry for 4xx errors', async () => {
    const fetchMock = vi.fn().mockResolvedValue({
      ok: false,
      status: 400,
      json: async () => ({
        success: false,
        data: null,
        error: { code: 'invalid_request', message: 'bad request' }
      })
    });
    vi.stubGlobal('fetch', fetchMock);

    await expect(request('/api/test')).rejects.toThrow('bad request');
    expect(fetchMock).toHaveBeenCalledTimes(1);
  });
});