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

    scanStore.setBasePath('/tmp');
    await scanStore.startScan();

    const state = get(scanStore);
    expect(state?.scanResult).not.toBeNull();
    expect(state?.error).toBeNull();
  });
});