import { describe, expect, it } from 'vitest';

import { buildConflictsFromScan, prioritizeConflictGroups } from '$lib/stores/conflicts';

describe('buildConflictsFromScan', () => {
  it('creates a conflict item for duplicate source directories', () => {
    const conflicts = buildConflictsFromScan({
      scan_id: 'scan-1',
      media_type: 'tv',
      base_path: '/media/tv',
      summary: {
        total_shows: 1,
        total_episodes: 0,
        merges: 1,
        conflicts: 0
      },
      groups: [
        {
          normalized_name: 'Animal Control (2023)',
          source_directories: [
            'Animal Control - 2023 [tvdbid-423022]',
            'Animal Control 2023 - [tmdbid-214162]'
          ]
        }
      ]
    });

    expect(conflicts).toHaveLength(1);
    expect(conflicts[0].candidates).toHaveLength(2);
    expect(conflicts[0].normalizedName).toBe('Animal Control (2023)');
  });

  it('prioritizes conflict groups before non-conflict groups', () => {
    const sorted = prioritizeConflictGroups([
        {
          normalized_name: 'B Non Conflict Show (2022)',
          source_directories: ['B Non Conflict Show 2022 - [tmdbid-2]'],
          episode_count: 12,
          conflict_count: 0
        },
        {
          normalized_name: 'A Conflict Show (2021)',
          source_directories: ['A Conflict Show 2021 - [tmdbid-1]', 'A Conflict Show (2021)'],
          episode_count: 24,
          conflict_count: 2
        },
        {
          normalized_name: 'C Non Conflict Show (2023)',
          source_directories: ['C Non Conflict Show 2023 - [tmdbid-3]'],
          episode_count: 8,
          conflict_count: 0
        }
      ]);

    expect(sorted.map((group) => group.normalized_name)).toEqual([
      'A Conflict Show (2021)',
      'B Non Conflict Show (2022)',
      'C Non Conflict Show (2023)'
    ]);
  });

  it('builds conflict items even when groups include enriched episode metadata', () => {
    const conflicts = buildConflictsFromScan({
      scan_id: 'scan-episode-preview',
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
          normalized_name: 'Quality Show (2024)',
          source_directories: ['Quality Show 2024 - [tmdbid-1]', 'Quality Show - 2024 [tvdbid-2]'],
          episode_count: 1,
          conflict_count: 0,
          episodes: [
            {
              season_number: 1,
              episode_number: 1,
              auto_selected_source_directory: 'Quality Show - 2024 [tvdbid-2]',
              conflict: false,
              candidates: []
            }
          ]
        }
      ]
    });

    expect(conflicts).toHaveLength(1);
    expect(conflicts[0].normalizedName).toBe('Quality Show (2024)');
    expect(conflicts[0].candidates).toHaveLength(2);
  });
});
