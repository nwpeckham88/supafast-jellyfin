import { derived, writable } from 'svelte/store';

import type { ConflictItem, ScanGroup, ScanResult } from '$lib/models/normalize';

type Selections = Record<string, string>;

const conflictItems = writable<ConflictItem[]>([]);
const selections = writable<Selections>({});

export function buildConflictsFromScan(scanResult: ScanResult | null): ConflictItem[] {
  if (!scanResult) {
    return [];
  }

  return scanResult.groups
    .filter((group) => group.source_directories.length > 1)
    .map((group) => ({
      id: group.normalized_name,
      normalizedName: group.normalized_name,
      candidates: group.source_directories.map((sourceDirectory, index) => ({
        id: `${group.normalized_name}-${index}`,
        sourceDirectory
      }))
    }));
}

export function prioritizeConflictGroups(groups: ScanGroup[]): ScanGroup[] {
  const conflictGroups = groups
    .filter((group) => group.source_directories.length > 1)
    .sort((left, right) => left.normalized_name.localeCompare(right.normalized_name));

  const nonConflictGroups = groups
    .filter((group) => group.source_directories.length <= 1)
    .sort((left, right) => left.normalized_name.localeCompare(right.normalized_name));

  return [...conflictGroups, ...nonConflictGroups];
}

export const conflictsStore = {
  items: { subscribe: conflictItems.subscribe },
  selections: { subscribe: selections.subscribe },
  unresolvedCount: derived([conflictItems, selections], ([$items, $selections]) =>
    $items.filter((item) => !$selections[item.id]).length
  ),
  seedFromScan(scanResult: ScanResult | null) {
    const items = buildConflictsFromScan(scanResult);
    conflictItems.set(items);
    selections.update((state) => {
      const next: Selections = {};

      for (const item of items) {
        const selectedCandidateId = state[item.id];
        if (!selectedCandidateId) {
          continue;
        }

        const candidateStillExists = item.candidates.some(
          (candidate) => candidate.id === selectedCandidateId
        );

        if (candidateStillExists) {
          next[item.id] = selectedCandidateId;
        }
      }

      return next;
    });
  },
  choose(conflictId: string, candidateId: string) {
    selections.update((state) => ({ ...state, [conflictId]: candidateId }));
  },
  reset() {
    selections.set({});
  }
};