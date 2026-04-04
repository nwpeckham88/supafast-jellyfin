<script lang="ts">
  import type { NormalizeAuditEntry } from '$lib/models/normalize';

  interface Props {
    idempotencyKey: string;
    jobId: string;
    status: 'pending' | 'running' | 'completed' | 'failed' | null;
    totalOps: number;
    completedOps: number;
    failedOps: number;
    entries: NormalizeAuditEntry[];
    entriesPage: number;
    entriesTotalPages: number;
    entriesTotal: number;
    submitting: boolean;
    polling: boolean;
    loadingEntries: boolean;
    error: string | null;
    onIdempotencyKeyChange: (value: string) => void;
    onDryRun: () => void;
    onExecute: () => void;
    onRefreshStatus: () => void;
    onRefreshEntries: () => void;
    onPreviousEntriesPage: () => void;
    onNextEntriesPage: () => void;
  }

  let {
    idempotencyKey,
    jobId,
    status,
    totalOps,
    completedOps,
    failedOps,
    entries,
    entriesPage,
    entriesTotalPages,
    entriesTotal,
    submitting,
    polling,
    loadingEntries,
    error,
    onIdempotencyKeyChange,
    onDryRun,
    onExecute,
    onRefreshStatus,
    onRefreshEntries,
    onPreviousEntriesPage,
    onNextEntriesPage
  }: Props = $props();

  const progressPercent = $derived(totalOps > 0 ? Math.round((completedOps / totalOps) * 100) : 0);
</script>

<section class="glass fade-up rounded-2xl p-5 md:p-6">
  <div class="mb-4 flex flex-wrap items-center justify-between gap-3">
    <h2 class="text-xl font-bold tracking-tight">Normalize Execute</h2>
    <div class="flex items-center gap-2 text-xs text-slate-700">
      <span>Polling: {polling ? 'on' : 'off'}</span>
      <button
        class="rounded border border-slate-300 px-2 py-1"
        disabled={!jobId || submitting}
        onclick={onRefreshStatus}
        type="button"
      >
        Refresh Status
      </button>
    </div>
  </div>

  <label class="text-sm font-medium text-slate-800" for="execute-idempotency-key">
    Idempotency Key
    <input
      class="mt-2 w-full rounded-lg border border-slate-300 bg-white px-3 py-2 text-sm text-slate-900"
      id="execute-idempotency-key"
      maxlength="128"
      oninput={(event) =>
        onIdempotencyKeyChange((event.currentTarget as HTMLInputElement).value)}
      placeholder="normalize-2026-04-04-001"
      type="text"
      value={idempotencyKey}
    />
  </label>

  <div class="mt-4 flex flex-wrap gap-2">
    <button
      class="rounded-lg border border-amber-400 bg-amber-50 px-3 py-2 text-xs font-semibold text-amber-900 hover:bg-amber-100"
      disabled={submitting}
      onclick={onDryRun}
      type="button"
    >
      Start Dry Run
    </button>
    <button
      class="rounded-lg bg-teal-700 px-3 py-2 text-xs font-semibold text-white hover:bg-teal-800"
      disabled={submitting}
      onclick={onExecute}
      type="button"
    >
      Start Execute
    </button>
  </div>

  {#if error}
    <aside class="mt-4 rounded-xl border border-red-300 bg-red-50 p-3 text-sm text-red-800">
      {error}
    </aside>
  {/if}

  <article class="mt-4 rounded-xl border border-slate-200 bg-slate-50 p-4">
    <h3 class="text-sm font-semibold text-slate-900">Job Progress</h3>
    <p class="mt-1 text-xs text-slate-700">
      Job: {jobId || 'not started'} | Status: {status ?? 'idle'}
    </p>
    <p class="mt-1 text-xs text-slate-700">
      Ops: {completedOps}/{totalOps} completed, {failedOps} failed ({progressPercent}%)
    </p>
    <div class="mt-2 h-2 w-full rounded bg-slate-200">
      <div class="h-2 rounded bg-teal-600" style={`width: ${progressPercent}%`}></div>
    </div>
  </article>

  <article class="mt-4 rounded-xl border border-slate-200 bg-slate-50 p-4">
    <div class="flex items-center justify-between gap-3">
      <h3 class="text-sm font-semibold text-slate-900">Operations</h3>
      <div class="flex items-center gap-2 text-xs text-slate-700">
        <button
          class="rounded border border-slate-300 px-2 py-1"
          disabled={!jobId || loadingEntries}
          onclick={onRefreshEntries}
          type="button"
        >
          {loadingEntries ? 'Loading...' : 'Refresh Entries'}
        </button>
      </div>
    </div>

    <div class="mt-2 flex items-center gap-2 text-xs text-slate-700">
      <button
        class="rounded border border-slate-300 px-2 py-1"
        disabled={loadingEntries || entriesPage <= 1}
        onclick={onPreviousEntriesPage}
        type="button"
      >
        Prev
      </button>
      <button
        class="rounded border border-slate-300 px-2 py-1"
        disabled={loadingEntries || entriesTotalPages === 0 || entriesPage >= entriesTotalPages}
        onclick={onNextEntriesPage}
        type="button"
      >
        Next
      </button>
      <span>Page {entriesPage} of {entriesTotalPages || 1} ({entriesTotal} ops)</span>
    </div>

    <div class="mt-3 max-h-64 space-y-2 overflow-auto rounded-lg border border-slate-200 bg-white p-3">
      {#if !entries.length}
        <p class="text-xs text-slate-600">No operations loaded yet.</p>
      {:else}
        {#each entries as entry}
          <div class="rounded border border-slate-200 p-2 text-xs">
            <p class="font-semibold text-slate-900">
              #{entry.op_index} {entry.op_type} [{entry.status}]
            </p>
            <p class="mt-1 text-slate-700">{entry.source_path}</p>
            <p class="text-slate-500">to {entry.target_path}</p>
            {#if entry.error_message}
              <p class="mt-1 text-red-700">Error: {entry.error_message}</p>
            {/if}
          </div>
        {/each}
      {/if}
    </div>
  </article>
</section>
