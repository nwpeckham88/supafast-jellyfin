<script lang="ts">
  import type { ScanResult } from '$lib/models/normalize';

  interface Props {
    result?: ScanResult | null;
  }

  let { result = null }: Props = $props();

  const displayGroups = $derived.by(() => {
    if (!result) {
      return [];
    }

    const conflictGroups = result.groups
      .filter((group) => group.source_directories.length > 1)
      .sort((left, right) => left.normalized_name.localeCompare(right.normalized_name));

    const nonConflictGroups = result.groups
      .filter((group) => group.source_directories.length <= 1)
      .sort((left, right) => left.normalized_name.localeCompare(right.normalized_name));

    return [...conflictGroups, ...nonConflictGroups];
  });
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
          <h3 class="text-sm font-semibold">{group.normalized_name}</h3>
          <ul class="mt-2 space-y-1">
            {#each group.source_directories as sourceDirectory}
              <li class="rounded bg-slate-100 px-2 py-1 text-xs text-slate-700">{sourceDirectory}</li>
            {/each}
          </ul>
        </li>
      {/each}
    </ul>
  {/if}
</section>