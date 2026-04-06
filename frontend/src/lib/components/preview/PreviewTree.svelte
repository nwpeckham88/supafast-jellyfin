<script lang="ts">
  import type { ScanEpisodeSlot, ScanResult } from '$lib/models/normalize';
  import { prioritizeConflictGroups } from '$lib/stores/conflicts';

  interface Props {
    result?: ScanResult | null;
  }

  let { result = null }: Props = $props();

  const displayGroups = $derived.by(() => {
    if (!result) {
      return [];
    }

    return prioritizeConflictGroups(result.groups);
  });

  function formatEpisodeSlot(slot: ScanEpisodeSlot): string {
    return `S${String(slot.season_number).padStart(2, '0')}E${String(slot.episode_number).padStart(2, '0')}`;
  }
</script>

<section class="glass fade-up rounded-2xl p-5 md:p-6">
  <h2 class="text-xl font-semibold">Hierarchical Preview</h2>
  <p class="mt-2 text-sm text-slate-700">
    Normalized groups with source directories beneath each target show name.
  </p>

  {#if !result || displayGroups.length === 0}
    <p class="mt-4 rounded-lg border border-dashed border-slate-300 bg-white p-4 text-sm text-slate-700">
      No grouped directories available for this scan yet.
    </p>
  {:else}
    <ul class="mt-4 space-y-3">
      {#each displayGroups as group (group.normalized_name)}
        <li class="rounded-xl border border-slate-200 bg-white p-3">
          <div class="flex flex-wrap items-start justify-between gap-2">
            <div>
              <h3 class="text-sm font-semibold">{group.normalized_name}</h3>
              <p class="mt-1 text-xs text-slate-600">
                {group.source_directories.length} source director{group.source_directories.length === 1 ? 'y' : 'ies'}
                {#if group.episode_count}
                  <span class="mx-1 text-slate-400">•</span>
                  {group.episode_count} episode slot{group.episode_count === 1 ? '' : 's'}
                {/if}
                {#if group.conflict_count}
                  <span class="mx-1 text-slate-400">•</span>
                  <span class="font-medium text-orange-700">{group.conflict_count} unresolved overlap{group.conflict_count === 1 ? '' : 's'}</span>
                {/if}
              </p>
            </div>
            {#if group.conflict_count}
              <span class="rounded-full bg-orange-100 px-2 py-1 text-[10px] font-semibold uppercase tracking-wide text-orange-900">
                conflict
              </span>
            {:else if group.episode_count && group.episode_count > 0}
              <span class="rounded-full bg-teal-100 px-2 py-1 text-[10px] font-semibold uppercase tracking-wide text-teal-900">
                scanned
              </span>
            {/if}
          </div>

          <ul class="mt-2 space-y-1">
            {#each group.source_directories as sourceDirectory}
              <li class="rounded bg-slate-100 px-2 py-1 text-xs text-slate-700">{sourceDirectory}</li>
            {/each}
          </ul>

          {#if group.episodes && group.episodes.length > 0}
            <div class="mt-3 rounded-lg border border-slate-200 bg-slate-50/80 p-3">
              <div class="flex items-center justify-between gap-2">
                <h4 class="text-xs font-semibold uppercase tracking-wide text-slate-700">Episode Slots</h4>
                <span class="text-[11px] text-slate-500">quality-aware overlap preview</span>
              </div>

              <ul class="mt-3 space-y-2">
                {#each group.episodes as slot (`${group.normalized_name}-${slot.season_number}-${slot.episode_number}`)}
                  <li class="rounded-lg border border-slate-200 bg-white p-2">
                    <div class="flex flex-wrap items-center justify-between gap-2">
                      <div class="flex items-center gap-2">
                        <span class="font-mono text-xs text-slate-800">{formatEpisodeSlot(slot)}</span>
                        <span class="text-xs text-slate-500">{slot.candidates.length} candidate{slot.candidates.length === 1 ? '' : 's'}</span>
                      </div>

                      {#if slot.conflict}
                        <span class="rounded-full bg-orange-100 px-2 py-0.5 text-[10px] font-semibold uppercase tracking-wide text-orange-900">
                          unresolved
                        </span>
                      {:else if slot.auto_selected_source_directory}
                        <span class="rounded-full bg-teal-100 px-2 py-0.5 text-[10px] font-semibold uppercase tracking-wide text-teal-900">
                          auto-selected
                        </span>
                      {/if}
                    </div>

                    {#if slot.auto_selected_source_directory}
                      <p class="mt-1 text-[11px] text-slate-600">
                        Selected source: <span class="font-medium text-slate-800">{slot.auto_selected_source_directory}</span>
                      </p>
                    {/if}

                    <ul class="mt-2 space-y-1">
                      {#each slot.candidates as candidate (`${slot.season_number}-${slot.episode_number}-${candidate.source_directory}-${candidate.relative_path}`)}
                        <li class="rounded bg-slate-100 px-2 py-1 text-[11px] text-slate-700">
                          <div class="flex flex-wrap items-center justify-between gap-2">
                            <span>{candidate.source_directory}: {candidate.relative_path}</span>
                            <span class="text-slate-500">{candidate.normalized_relative_path}</span>
                          </div>
                        </li>
                      {/each}
                    </ul>
                  </li>
                {/each}
              </ul>
            </div>
          {/if}
        </li>
      {/each}
    </ul>
  {/if}
</section>
