<script lang="ts">
  import type { ConflictItem } from '$lib/models/normalize';

  interface Props {
    conflicts?: ConflictItem[];
    selections?: Record<string, string>;
    unresolvedCount?: number;
    onChoose: (conflictId: string, candidateId: string) => void;
    onAutoSelectLikelyBest: () => void;
    onReset: () => void;
  }

  let {
    conflicts = [],
    selections = {},
    unresolvedCount = 0,
    onChoose,
    onAutoSelectLikelyBest,
    onReset
  }: Props = $props();
</script>

<section class="glass fade-up rounded-2xl p-5 md:p-6">
  <div class="flex flex-wrap items-center justify-between gap-3">
    <h2 class="text-xl font-semibold">Conflict Selection</h2>
    <div class="flex items-center gap-2">
      <span class="rounded-full bg-amber-100 px-3 py-1 text-xs font-semibold text-amber-900">
        Unresolved: {unresolvedCount}
      </span>
      <button
        class="rounded border border-teal-300 bg-teal-50 px-3 py-1 text-xs font-semibold text-teal-900 hover:bg-teal-100"
        type="button"
        disabled={conflicts.length === 0}
        onclick={onAutoSelectLikelyBest}
      >
        Auto-Select Likely Best
      </button>
      <button
        class="rounded border border-slate-300 bg-white px-3 py-1 text-xs font-semibold text-slate-800 hover:bg-slate-100"
        type="button"
        onclick={onReset}
      >
        Reset Choices
      </button>
    </div>
  </div>

  {#if conflicts.length === 0}
    <p class="mt-4 rounded-lg border border-dashed border-slate-300 bg-white p-4 text-sm text-slate-700">
      No duplicate groups detected. Conflict resolution is ready when duplicates appear.
    </p>
  {:else}
    <div class="mt-4 space-y-4">
      {#each conflicts as conflict (conflict.id)}
        <article class="rounded-xl border border-amber-200 bg-amber-50 p-3">
          <h3 class="text-sm font-semibold text-amber-900">{conflict.normalizedName}</h3>
          <p class="mt-1 text-xs text-amber-900">
            Choose the preferred source directory.
            {#if conflict.recommendedCandidateId}
              Episode-slot scan found a consistent preferred source and preselected it for you.
            {/if}
          </p>
          <div class="mt-3 grid gap-2 md:grid-cols-2">
            {#each conflict.candidates as candidate}
              <label class="flex cursor-pointer items-center gap-2 rounded-lg border border-amber-200 bg-white px-3 py-2 text-xs">
                <input
                  type="radio"
                  name={conflict.id}
                  checked={selections[conflict.id] === candidate.id}
                  onchange={() => onChoose(conflict.id, candidate.id)}
                />
                <span>{candidate.sourceDirectory}</span>
                {#if conflict.recommendedCandidateId === candidate.id}
                  <span class="rounded-full bg-teal-100 px-2 py-0.5 text-[10px] font-semibold uppercase tracking-wide text-teal-900">
                    recommended
                  </span>
                {/if}
              </label>
            {/each}
          </div>
        </article>
      {/each}
    </div>
  {/if}
</section>
