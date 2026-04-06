import { afterEach, describe, expect, it, vi } from 'vitest';
import { get } from 'svelte/store';

import { scanStore } from '$lib/stores/scan';

describe('normalize page flow', () => {
  afterEach(() => {
    vi.restoreAllMocks();
  });

  it('creates and stores a scan preview from the API', async () => {
    vi.stubGlobal(
      'fetch',
      vi.fn().mockResolvedValue({
        ok: true,
        status: 201,
        json: async () => ({
          success: true,
          data: {
            scan_id: 'scan-1',
            media_type: 'tv',
            base_path: '/media/tv',
            groups: [
              {
                normalized_name: 'Animal Control (2023)',
                source_directories: [
                  'Animal Control - 2023 [tvdbid-423022]',
                  'Animal Control 2023 - [tmdbid-214162]'
                ]
              }
            ],
            summary: {
              total_shows: 1,
              total_episodes: 0,
              merges: 1,
              conflicts: 0
            }
          },
          error: null
        })
      })
    );

    await scanStore.startScan();

    const state = get(scanStore);
    expect(state?.scanResult).not.toBeNull();
    expect(state?.error).toBeNull();
    expect(fetch).toHaveBeenCalledWith(
      '/api/normalize/scan',
      expect.objectContaining({
        method: 'POST',
        body: JSON.stringify({ media_type: 'tv', base_path: '/media/tv' })
      })
    );
  });

  it('recovers after a failed scan when retried', async () => {
    const fetchMock = vi
      .fn()
      .mockResolvedValueOnce({
        ok: false,
        status: 500,
        json: async () => ({
          success: false,
          data: null,
          error: { code: 'internal_error', message: 'temporary failure' }
        })
      })
      .mockResolvedValueOnce({
        ok: true,
        status: 201,
        json: async () => ({
          success: true,
          data: {
            scan_id: 'scan-2',
            media_type: 'tv',
            base_path: '/media/tv',
            groups: [],
            summary: {
              total_shows: 0,
              total_episodes: 0,
              merges: 0,
              conflicts: 0
            }
          },
          error: null
        })
      });

    vi.stubGlobal('fetch', fetchMock);

    await scanStore.startScan();
    expect(get(scanStore)?.error).toBe('temporary failure');

    await scanStore.startScan();
    const state = get(scanStore);
    expect(state?.error).toBeNull();
    expect(state?.scanResult?.scan_id).toBe('scan-2');
    expect(fetchMock).toHaveBeenCalledTimes(2);
    expect(fetchMock).toHaveBeenNthCalledWith(
      1,
      '/api/normalize/scan',
      expect.objectContaining({
        method: 'POST',
        body: JSON.stringify({ media_type: 'tv', base_path: '/media/tv' })
      })
    );
  });
});