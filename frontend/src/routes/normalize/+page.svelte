<script lang="ts">
  import type { ScanResult } from '$lib/models/normalize';

  import ConflictPanel from '$lib/components/conflicts/ConflictPanel.svelte';
  import ExecutePanel from '$lib/components/execute/ExecutePanel.svelte';
  import PreviewTree from '$lib/components/preview/PreviewTree.svelte';
  import RollbackPanel from '$lib/components/rollback/RollbackPanel.svelte';
  import ScanRequestForm from '$lib/components/scan/ScanRequestForm.svelte';
  import ScanSummaryCard from '$lib/components/scan/ScanSummaryCard.svelte';
  import { conflictsStore } from '$lib/stores/conflicts';
  import { executeStore } from '$lib/stores/execute';
  import { rollbackStore } from '$lib/stores/rollback';
  import { scanStore } from '$lib/stores/scan';

  const conflictItems = conflictsStore.items;
  const conflictSelections = conflictsStore.selections;
  const unresolvedCount = conflictsStore.unresolvedCount;
  let lastSeededResult: ScanResult | null | undefined = undefined;
  let rollbackSessionsLoaded = false;

  $effect(() => {
    const result = $scanStore.scanResult;
    if (result === lastSeededResult) {
      return;
    }

    lastSeededResult = result;
    conflictsStore.seedFromScan(result);
    executeStore.syncFromScan(result);
  });

  $effect(() => {
    if (rollbackSessionsLoaded) {
      return;
    }

    rollbackSessionsLoaded = true;
    void rollbackStore.refreshSessions();
  });
</script>

<svelte:head>
  <title>Normalize Preview | supafast-jellyfin</title>
</svelte:head>

<section class="mb-6">
  <h1 class="text-3xl font-bold tracking-tight">Normalize Scan Preview</h1>
  <p class="mt-2 text-sm text-slate-700">
    Phase 4 includes scan preview, execute, and rollback session tooling in one workflow.
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
          executeStore.reset();
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

  <ExecutePanel
    idempotencyKey={$executeStore.idempotencyKey}
    jobId={$executeStore.jobId}
    status={$executeStore.status}
    totalOps={$executeStore.totalOps}
    completedOps={$executeStore.completedOps}
    failedOps={$executeStore.failedOps}
    entries={$executeStore.entries}
    entriesPage={$executeStore.entriesPage}
    entriesTotalPages={$executeStore.entriesTotalPages}
    entriesTotal={$executeStore.entriesTotal}
    submitting={$executeStore.submitting}
    polling={$executeStore.polling}
    loadingEntries={$executeStore.loadingEntries}
    error={$executeStore.error}
    onIdempotencyKeyChange={(value) => executeStore.setIdempotencyKey(value)}
    onDryRun={() =>
      executeStore.startExecute({
        scanResult: $scanStore.scanResult,
        idempotencyKey: $executeStore.idempotencyKey,
        dryRun: true,
        conflictItems: $conflictItems,
        conflictSelections: $conflictSelections
      })}
    onExecute={() =>
      executeStore.startExecute({
        scanResult: $scanStore.scanResult,
        idempotencyKey: $executeStore.idempotencyKey,
        dryRun: false,
        conflictItems: $conflictItems,
        conflictSelections: $conflictSelections
      })}
    onRefreshStatus={() => executeStore.refreshStatus()}
    onRefreshEntries={() => executeStore.refreshEntries()}
    onPreviousEntriesPage={() => executeStore.previousEntriesPage()}
    onNextEntriesPage={() => executeStore.nextEntriesPage()}
  />

  <RollbackPanel
    sessions={$rollbackStore.sessions}
    sessionsPage={$rollbackStore.sessionsPage}
    sessionsTotalPages={$rollbackStore.sessionsTotalPages}
    sessionsTotal={$rollbackStore.sessionsTotal}
    selectedSessionId={$rollbackStore.selectedSessionId}
    idempotencyKey={$rollbackStore.idempotencyKey}
    preview={$rollbackStore.preview}
    result={$rollbackStore.result}
    loadingSessions={$rollbackStore.loadingSessions}
    loadingPreview={$rollbackStore.loadingPreview}
    submitting={$rollbackStore.submitting}
    error={$rollbackStore.error}
    onSelectSession={(value) => rollbackStore.setSelectedSession(value)}
    onIdempotencyKeyChange={(value) => rollbackStore.setIdempotencyKey(value)}
    onRefreshSessions={() => rollbackStore.refreshSessions()}
    onPreviousSessionsPage={() => rollbackStore.previousSessionsPage()}
    onNextSessionsPage={() => rollbackStore.nextSessionsPage()}
    onLoadPreview={() => rollbackStore.loadPreview()}
    onDryRun={() => rollbackStore.runRollback(true)}
    onExecute={() => rollbackStore.runRollback(false)}
  />
</div>

{#if !$rollbackStore.sessions.length}
  <section class="mt-4 rounded-xl border border-amber-300 bg-amber-50 p-3 text-sm text-amber-900">
    No rollback sessions yet. Run normalize execute first, then refresh rollback sessions.
  </section>
{/if}