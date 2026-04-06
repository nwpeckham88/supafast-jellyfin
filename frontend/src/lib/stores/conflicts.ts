import { derived, writable } from 'svelte/store';

import type { ConflictItem, ScanGroup, ScanResult } from '$lib/models/normalize';

type Selections = Record<string, string>;

const conflictItems = writable<ConflictItem[]>([]);
const selections = writable<Selections>({});

const QUALITY_TOKENS: Array<{ pattern: RegExp; score: number }> = [
  { pattern: /\b(4320p|8k)\b/i, score: 500 },
  { pattern: /\b(2160p|4k|uhd)\b/i, score: 400 },
  { pattern: /\b(1440p|2k)\b/i, score: 300 },
  { pattern: /\b1080p\b/i, score: 200 },
  { pattern: /\b720p\b/i, score: 120 },
  { pattern: /\b480p\b/i, score: 60 },
  { pattern: /\b(hevc|x265|h265|av1)\b/i, score: 45 },
  { pattern: /\b(h264|x264|avc)\b/i, score: 25 },
  { pattern: /\b(remux|bluray|blu-ray)\b/i, score: 35 },
  { pattern: /\b(web-?dl|webdl|webrip)\b/i, score: 20 }
];

export function buildConflictsFromScan(scanResult: ScanResult | null): ConflictItem[] {
  if (!scanResult) {
    return [];
  }

  return scanResult.groups
    .filter((group) => group.source_directories.length > 1)
    .map((group) => {
      const candidates = group.source_directories.map((sourceDirectory, index) => ({
        id: buildCandidateId(group.normalized_name, index),
        sourceDirectory
      }));

      const recommendedSourceDirectory = preferredSourceDirectoryForGroup(group);
      const recommendedCandidate = candidates.find(
        (candidate) => candidate.sourceDirectory === recommendedSourceDirectory
      );

      return {
        id: group.normalized_name,
        normalizedName: group.normalized_name,
        candidates,
        recommendedCandidateId: recommendedCandidate?.id ?? null
      };
    });
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

export function scoreCandidateForQuality(candidateLabel: string): number {
  const normalized = candidateLabel.toLowerCase();

  return QUALITY_TOKENS.reduce((sum, token) => {
    return sum + (token.pattern.test(normalized) ? token.score : 0);
  }, 0);
}

function pickLikelyBestCandidateId(conflict: ConflictItem): string | null {
  if (conflict.candidates.length === 0) {
    return null;
  }

  if (conflict.recommendedCandidateId) {
    return conflict.recommendedCandidateId;
  }

  let bestId: string | null = null;
  let bestScore = Number.NEGATIVE_INFINITY;

  for (const candidate of conflict.candidates) {
    const score = scoreCandidateForQuality(candidate.sourceDirectory);
    if (score > bestScore) {
      bestScore = score;
      bestId = candidate.id;
    }
  }

  return bestId;
}

function buildCandidateId(conflictId: string, index: number): string {
  return `${conflictId}-${index}`;
}

function preferredSourceDirectoryForGroup(group: ScanGroup): string | null {
  if (!group.episodes || group.episodes.length === 0) {
    return null;
  }

  const preferredSources = new Set<string>();
  let overlappingSlotCount = 0;

  for (const slot of group.episodes) {
    if (slot.candidates.length <= 1) {
      continue;
    }

    overlappingSlotCount += 1;
    if (slot.conflict || !slot.auto_selected_source_directory) {
      return null;
    }

    preferredSources.add(slot.auto_selected_source_directory);
  }

  if (overlappingSlotCount === 0 || preferredSources.size !== 1) {
    return null;
  }

  return Array.from(preferredSources)[0] ?? null;
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
        if (selectedCandidateId) {
          const candidateStillExists = item.candidates.some(
            (candidate) => candidate.id === selectedCandidateId
          );

          if (candidateStillExists) {
            next[item.id] = selectedCandidateId;
            continue;
          }
        }

        if (item.recommendedCandidateId) {
          next[item.id] = item.recommendedCandidateId;
        }
      }

      return next;
    });
  },
  choose(conflictId: string, candidateId: string) {
    selections.update((state) => ({ ...state, [conflictId]: candidateId }));
  },
  autoSelectLikelyBest() {
    let itemsSnapshot: ConflictItem[] = [];
    const unsubscribe = conflictItems.subscribe((items) => {
      itemsSnapshot = items;
    });
    unsubscribe();

    selections.update((state) => {
      const next = { ...state };

      for (const item of itemsSnapshot) {
        if (next[item.id]) {
          continue;
        }

        const likelyBest = pickLikelyBestCandidateId(item);
        if (likelyBest) {
          next[item.id] = likelyBest;
        }
      }

      return next;
    });
  },
  reset() {
    selections.set({});
  }
};
