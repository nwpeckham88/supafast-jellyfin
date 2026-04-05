import { get } from 'svelte/store';
import { describe, expect, it } from 'vitest';

import { conflictsStore } from '$lib/stores/conflicts';
import type { ScanResult } from '$lib/models/normalize';

function sampleScanResult(): ScanResult {
  return {
    scan_id: 'scan-1',
    media_type: 'tv',
    base_path: '/media/tv',
    summary: {
      total_shows: 2,
      total_episodes: 0,
      merges: 1,
      conflicts: 1
    },
    groups: [
      {
        normalized_name: 'Animal Control (2023)',
        source_directories: [
          'Animal Control - 2023 [tvdbid-423022]',
          'Animal Control 2023 - [tmdbid-214162]'
        ]
      },
      {
        normalized_name: 'Single Source (2022)',
        source_directories: ['Single Source 2022']
      }
    ]
  };
}

describe('conflictsStore', () => {
  it('preserves existing valid selections when reseeding', () => {
    const scan = sampleScanResult();

    conflictsStore.seedFromScan(scan);
    conflictsStore.choose('Animal Control (2023)', 'Animal Control (2023)-1');

    conflictsStore.seedFromScan(scan);

    const selectionState = get(conflictsStore.selections);
    expect(selectionState['Animal Control (2023)']).toBe('Animal Control (2023)-1');
  });

  it('drops selections when the selected candidate no longer exists', () => {
    const initial = sampleScanResult();
    conflictsStore.seedFromScan(initial);
    conflictsStore.choose('Animal Control (2023)', 'Animal Control (2023)-1');

    const changed: ScanResult = {
      ...initial,
      groups: [
        {
          normalized_name: 'Animal Control (2023)',
          source_directories: ['Animal Control - 2023 [tvdbid-423022]']
        }
      ]
    };

    conflictsStore.seedFromScan(changed);

    const selectionState = get(conflictsStore.selections);
    expect(selectionState['Animal Control (2023)']).toBeUndefined();
  });
});
