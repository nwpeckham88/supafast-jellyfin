<script lang="ts">
  import type { ScanResult } from '$lib/models/normalize';

  import ConflictPanel from '$lib/components/conflicts/ConflictPanel.svelte';
  import PreviewTree from '$lib/components/preview/PreviewTree.svelte';
  import ScanRequestForm from '$lib/components/scan/ScanRequestForm.svelte';
  import ScanSummaryCard from '$lib/components/scan/ScanSummaryCard.svelte';
  import { conflictsStore } from '$lib/stores/conflicts';
  import { scanStore } from '$lib/stores/scan';

  const conflictItems = conflictsStore.items;
  const conflictSelections = conflictsStore.selections;
  const unresolvedCount = conflictsStore.unresolvedCount;
  let lastSeededResult: ScanResult | null | undefined = undefined;

  $effect(() => {
    const result = $scanStore.scanResult;
    if (result === lastSeededResult) {
      return;
    }

    lastSeededResult = result;
    conflictsStore.seedFromScan(result);
  });
</script>

<svelte:head>
  <title>Normalize Preview | supafast-jellyfin</title>
</svelte:head>

<section class="mb-6">
  <h1 class="text-3xl font-bold tracking-tight">Normalize Scan Preview</h1>
  <p class="mt-2 text-sm text-slate-700">
    Review normalized groupings and mock conflict choices before execute endpoints land in Phase 3.
  </p>
</section>

{#if $scanStore.error}
  <aside class="mb-5 rounded-xl border border-red-300 bg-red-50 p-3 text-sm text-red-800">
    {$scanStore.error}
  </aside>
{/if}

<div class="grid gap-5">
  <ScanRequestForm
    mediaType={$scanStore.mediaType}
    basePath={$scanStore.basePath}
    busy={$scanStore.loading}
    onSubmit={() => scanStore.startScan()}
    onMediaTypeChange={(value) => scanStore.setMediaType(value)}
    onBasePathChange={(value) => scanStore.setBasePath(value)}
  />

  <div class="flex flex-wrap items-center gap-3">
    <button
      class="rounded-lg border border-slate-300 bg-white px-3 py-2 text-xs font-semibold text-slate-800 hover:bg-slate-100"
      disabled={$scanStore.loading || !$scanStore.scanResult}
      onclick={() => scanStore.refreshScan()}
      type="button"
    >
      Refresh Scan
    </button>
    <button
      class="rounded-lg border border-slate-300 bg-white px-3 py-2 text-xs font-semibold text-slate-800 hover:bg-slate-100"
      disabled={$scanStore.loading}
      onclick={async () => {
        const cleared = await scanStore.clearScanSession();
        if (cleared) {
          conflictsStore.reset();
        }
      }}
      type="button"
    >
      Expire And Reset
    </button>
  </div>

  <ScanSummaryCard result={$scanStore.scanResult} loading={$scanStore.loading} />
  <PreviewTree result={$scanStore.scanResult} />
  <ConflictPanel
    conflicts={$conflictItems}
    selections={$conflictSelections}
    unresolvedCount={$unresolvedCount}
    onChoose={(conflictId, candidateId) => conflictsStore.choose(conflictId, candidateId)}
    onReset={() => conflictsStore.reset()}
  />
</div>