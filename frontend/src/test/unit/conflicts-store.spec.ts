import { get } from 'svelte/store';
import { describe, expect, it } from 'vitest';

import { conflictsStore, scoreCandidateForQuality } from '$lib/stores/conflicts';
import type { ScanResult } from '$lib/models/normalize';

function episodeCandidatesFor(sourceDirectories: string[]) {
  return sourceDirectories.map((sourceDirectory, index) => ({
    source_directory: sourceDirectory,
    relative_path: `Season 1/file-${index}.mkv`,
    normalized_relative_path: 'Season 1/01x01 - Episode.mkv',
    season_number: 1,
    episode_number: 1,
    extension: 'mkv',
    file_size: 100 + index,
    quality: {
      resolution: '1080p',
      codec: 'h264',
      source: 'webdl',
      group: null,
      size_bytes: 100 + index
    }
  }));
}

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

  it('auto-seeds a recommended source when enriched episode data picks one consistently', () => {
    const scan: ScanResult = {
      scan_id: 'scan-recommended',
      media_type: 'tv',
      base_path: '/media/tv',
      summary: {
        total_shows: 1,
        total_episodes: 2,
        merges: 1,
        conflicts: 0
      },
      groups: [
        {
          normalized_name: 'Preferred Show (2025)',
          source_directories: ['Preferred Show 2025 - [tmdbid-1]', 'Preferred Show - 2025 [tvdbid-2]'],
          episode_count: 2,
          conflict_count: 0,
          episodes: [
            {
              season_number: 1,
              episode_number: 1,
              auto_selected_source_directory: 'Preferred Show - 2025 [tvdbid-2]',
              conflict: false,
              candidates: episodeCandidatesFor([
                'Preferred Show 2025 - [tmdbid-1]',
                'Preferred Show - 2025 [tvdbid-2]'
              ])
            },
            {
              season_number: 1,
              episode_number: 2,
              auto_selected_source_directory: 'Preferred Show - 2025 [tvdbid-2]',
              conflict: false,
              candidates: episodeCandidatesFor([
                'Preferred Show 2025 - [tmdbid-1]',
                'Preferred Show - 2025 [tvdbid-2]'
              ])
            }
          ]
        }
      ]
    };

    conflictsStore.seedFromScan(scan);

    const selectionState = get(conflictsStore.selections);
    expect(selectionState['Preferred Show (2025)']).toBe('Preferred Show (2025)-1');
  });

  it('does not auto-seed when enriched episode data still has unresolved conflicts', () => {
    const scan: ScanResult = {
      scan_id: 'scan-unresolved',
      media_type: 'tv',
      base_path: '/media/tv',
      summary: {
        total_shows: 1,
        total_episodes: 1,
        merges: 1,
        conflicts: 1
      },
      groups: [
        {
          normalized_name: 'Unresolved Show (2025)',
          source_directories: ['Unresolved Show Copy A', 'Unresolved Show Copy B'],
          episode_count: 1,
          conflict_count: 1,
          episodes: [
            {
              season_number: 1,
              episode_number: 1,
              auto_selected_source_directory: null,
              conflict: true,
              candidates: episodeCandidatesFor([
                'Unresolved Show Copy A',
                'Unresolved Show Copy B'
              ])
            }
          ]
        }
      ]
    };

    conflictsStore.seedFromScan(scan);

    const selectionState = get(conflictsStore.selections);
    expect(selectionState['Unresolved Show (2025)']).toBeUndefined();
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

  it('auto-selects enriched scan recommendation before filename quality heuristics', () => {
    const scan: ScanResult = {
      scan_id: 'scan-preferred-over-heuristics',
      media_type: 'tv',
      base_path: '/media/tv',
      summary: {
        total_shows: 1,
        total_episodes: 1,
        merges: 1,
        conflicts: 0
      },
      groups: [
        {
          normalized_name: 'Preference Beats Heuristic (2025)',
          source_directories: ['Preference Beats Heuristic 2025 2160p HEVC', 'Preference Beats Heuristic 2025 1080p x264'],
          episode_count: 1,
          conflict_count: 0,
          episodes: [
            {
              season_number: 1,
              episode_number: 1,
              auto_selected_source_directory: 'Preference Beats Heuristic 2025 1080p x264',
              conflict: false,
              candidates: episodeCandidatesFor([
                'Preference Beats Heuristic 2025 2160p HEVC',
                'Preference Beats Heuristic 2025 1080p x264'
              ])
            }
          ]
        }
      ]
    };

    conflictsStore.seedFromScan(scan);
    conflictsStore.reset();
    conflictsStore.autoSelectLikelyBest();

    const selectionState = get(conflictsStore.selections);
    expect(selectionState['Preference Beats Heuristic (2025)']).toBe(
      'Preference Beats Heuristic (2025)-1'
    );
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
