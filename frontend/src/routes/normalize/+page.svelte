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
  let scanDefaultsLoaded = false;

  const hasScan = $derived(Boolean($scanStore.scanResult));
  const hasConflicts = $derived($conflictItems.length > 0);
  const hasExecution = $derived(Boolean($executeStore.jobId));
  const hasRollbackPreview = $derived(Boolean($rollbackStore.preview));

  const workflowSteps = $derived([
    {
      title: '1. Scan',
      detail: hasScan ? 'Session active' : 'Create preview session',
      state: hasScan ? 'done' : 'idle'
    },
    {
      title: '2. Resolve',
      detail: hasConflicts ? `${$unresolvedCount} unresolved conflict(s)` : 'No conflicts detected',
      state: !hasScan
        ? 'idle'
        : !hasConflicts
          ? 'done'
          : $unresolvedCount > 0
            ? 'active'
            : 'done'
    },
    {
      title: '3. Execute',
      detail: hasExecution
        ? `${$executeStore.status ?? 'pending'} (${ $executeStore.completedOps }/${ $executeStore.totalOps })`
        : 'Run dry-run or execute',
      state: hasExecution ? 'active' : 'idle'
    },
    {
      title: '4. Rollback',
      detail: hasRollbackPreview ? 'Preview ready' : 'Load a rollback session preview',
      state: hasRollbackPreview ? 'active' : 'idle'
    }
  ]);

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
    if (scanDefaultsLoaded) {
      return;
    }

    scanDefaultsLoaded = true;
    void scanStore.hydrateDefaults();
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

<section class="glass fade-up mb-6 rounded-2xl border border-teal-200/60 p-6 md:p-7">
  <div class="flex flex-wrap items-start justify-between gap-4">
    <div>
      <p class="text-xs font-semibold uppercase tracking-[0.16em] text-teal-700">Normalization Console</p>
      <h1 class="mt-2 text-3xl font-bold tracking-tight">Scan, Resolve, Execute, Recover</h1>
      <p class="mt-3 max-w-3xl text-sm text-slate-700">
        End-to-end workflow for safe media normalization. Every operation is previewed first,
        tracked with idempotency, and can be rolled back from recorded sessions.
      </p>
    </div>
    <div class="rounded-xl border border-slate-200 bg-white/80 px-3 py-2 text-xs text-slate-700">
      <p>Active media type: <span class="font-semibold text-slate-900">{$scanStore.mediaType.toUpperCase()}</span></p>
      <p class="mt-1">Configured base path: <span class="font-medium text-slate-900">{$scanStore.activeBasePath}</span></p>
    </div>
  </div>

  <div class="mt-5 grid gap-3 md:grid-cols-4">
    {#each workflowSteps as step}
      <article
        class="rounded-xl border bg-white/85 p-3"
        class:border-teal-300={step.state === 'done'}
        class:border-amber-300={step.state === 'active'}
        class:border-slate-200={step.state === 'idle'}
      >
        <div class="flex items-center justify-between gap-2">
          <h2 class="text-sm font-semibold text-slate-900">{step.title}</h2>
          <span
            class="rounded-full px-2 py-0.5 text-[10px] font-semibold uppercase tracking-wide"
            class:bg-teal-100={step.state === 'done'}
            class:text-teal-900={step.state === 'done'}
            class:bg-amber-100={step.state === 'active'}
            class:text-amber-900={step.state === 'active'}
            class:bg-slate-100={step.state === 'idle'}
            class:text-slate-700={step.state === 'idle'}
          >
            {step.state}
          </span>
        </div>
        <p class="mt-2 text-xs text-slate-700">{step.detail}</p>
      </article>
    {/each}
  </div>
</section>

{#if $scanStore.error}
  <aside class="mb-5 rounded-xl border border-red-300 bg-red-50 p-3 text-sm text-red-800">
    {$scanStore.error}
  </aside>
{/if}

<div class="grid gap-5">
  <section class="rounded-2xl border border-slate-200/80 bg-white/40 p-1">
    <ScanRequestForm
      mediaType={$scanStore.mediaType}
      busy={$scanStore.loading}
      onSubmit={() => scanStore.startScan()}
      onMediaTypeChange={(value) => scanStore.setMediaType(value)}
    />
  </section>

  <section class="flex flex-wrap items-center gap-3 rounded-xl border border-slate-200 bg-white/70 p-3">
    <p class="text-xs font-medium text-slate-700">Session Controls</p>
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
  </section>

  <div class="grid gap-5 xl:grid-cols-[1.4fr_1fr]">
    <div class="space-y-5">
      <ScanSummaryCard result={$scanStore.scanResult} loading={$scanStore.loading} />
      <PreviewTree result={$scanStore.scanResult} />
      <ConflictPanel
        conflicts={$conflictItems}
        selections={$conflictSelections}
        unresolvedCount={$unresolvedCount}
        onChoose={(conflictId, candidateId) => conflictsStore.choose(conflictId, candidateId)}
        onAutoSelectLikelyBest={() => conflictsStore.autoSelectLikelyBest()}
        onReset={() => conflictsStore.reset()}
      />
    </div>

    <div class="space-y-5">
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
        onGenerateIdempotencyKey={() => executeStore.generateIdempotencyKey()}
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
        onGenerateIdempotencyKey={() => rollbackStore.generateIdempotencyKey()}
        onRefreshSessions={() => rollbackStore.refreshSessions()}
        onPreviousSessionsPage={() => rollbackStore.previousSessionsPage()}
        onNextSessionsPage={() => rollbackStore.nextSessionsPage()}
        onLoadPreview={() => rollbackStore.loadPreview()}
        onDryRun={() => rollbackStore.runRollback(true)}
        onExecute={() => rollbackStore.runRollback(false)}
      />
    </div>
  </div>
</div>

{#if !$rollbackStore.sessions.length}
  <section class="mt-4 rounded-xl border border-amber-300 bg-amber-50 p-3 text-sm text-amber-900">
    No rollback sessions yet. Run normalize execute first, then refresh rollback sessions.
  </section>
{/if}