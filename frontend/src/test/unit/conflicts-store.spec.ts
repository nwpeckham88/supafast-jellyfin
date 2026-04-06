import { get } from 'svelte/store';
import { describe, expect, it } from 'vitest';

import { conflictsStore, scoreCandidateForQuality } from '$lib/stores/conflicts';
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
  it('scores higher-quality labels above lower-quality labels', () => {
    const high = scoreCandidateForQuality('Show 2024 2160p HEVC REMUX');
    const medium = scoreCandidateForQuality('Show 2024 1080p x264 WEB-DL');
    const low = scoreCandidateForQuality('Show 2024 720p h264');

    expect(high).toBeGreaterThan(medium);
    expect(medium).toBeGreaterThan(low);
  });

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

  it('auto-selects likely best candidate for unresolved conflicts', () => {
    const scan: ScanResult = {
      scan_id: 'scan-2',
      media_type: 'tv',
      base_path: '/media/tv',
      summary: {
        total_shows: 1,
        total_episodes: 0,
        merges: 1,
        conflicts: 1
      },
      groups: [
        {
          normalized_name: 'Quality Show (2024)',
          source_directories: [
            'Quality Show 2024 1080p x264',
            'Quality Show 2024 2160p HEVC',
            'Quality Show 2024 720p h264'
          ]
        }
      ]
    };

    conflictsStore.seedFromScan(scan);
    conflictsStore.autoSelectLikelyBest();

    const selectionState = get(conflictsStore.selections);
    expect(selectionState['Quality Show (2024)']).toBe('Quality Show (2024)-1');
  });

  it('keeps first candidate when quality scores are equal', () => {
    const scan: ScanResult = {
      scan_id: 'scan-3',
      media_type: 'movie',
      base_path: '/media/movies',
      summary: {
        total_shows: 1,
        total_episodes: 0,
        merges: 1,
        conflicts: 1
      },
      groups: [
        {
          normalized_name: 'Equal Score Movie (2024)',
          source_directories: ['Equal Score Copy A', 'Equal Score Copy B']
        }
      ]
    };

    conflictsStore.seedFromScan(scan);
    conflictsStore.autoSelectLikelyBest();

    const selectionState = get(conflictsStore.selections);
    expect(selectionState['Equal Score Movie (2024)']).toBe('Equal Score Movie (2024)-0');
  });

  it('does not override an existing manual selection', () => {
    const scan: ScanResult = {
      scan_id: 'scan-4',
      media_type: 'tv',
      base_path: '/media/tv',
      summary: {
        total_shows: 1,
        total_episodes: 0,
        merges: 1,
        conflicts: 1
      },
      groups: [
        {
          normalized_name: 'Manual Pick Show (2025)',
          source_directories: ['Manual Pick Show 2025 1080p', 'Manual Pick Show 2025 2160p']
        }
      ]
    };

    conflictsStore.seedFromScan(scan);
    conflictsStore.choose('Manual Pick Show (2025)', 'Manual Pick Show (2025)-0');
    conflictsStore.autoSelectLikelyBest();

    const selectionState = get(conflictsStore.selections);
    expect(selectionState['Manual Pick Show (2025)']).toBe('Manual Pick Show (2025)-0');
  });
});
