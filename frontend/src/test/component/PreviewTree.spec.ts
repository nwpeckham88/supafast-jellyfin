import { describe, expect, it } from 'vitest';

import { buildConflictsFromScan } from '$lib/stores/conflicts';

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
});