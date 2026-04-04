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
  const currentStatus = $derived(status ?? 'idle');
  const statusBadgeClass = $derived.by(() => {
    switch (currentStatus) {
      case 'completed':
        return 'bg-teal-100 text-teal-900 border-teal-300';
      case 'running':
        return 'bg-amber-100 text-amber-900 border-amber-300';
      case 'failed':
        return 'bg-red-100 text-red-900 border-red-300';
      case 'pending':
        return 'bg-indigo-100 text-indigo-900 border-indigo-300';
      default:
        return 'bg-slate-100 text-slate-700 border-slate-300';
    }
  });

  function entryStatusClass(status: NormalizeAuditEntry['status']): string {
    switch (status) {
      case 'succeeded':
        return 'bg-teal-100 text-teal-900 border-teal-300';
      case 'running':
        return 'bg-amber-100 text-amber-900 border-amber-300';
      case 'failed':
        return 'bg-red-100 text-red-900 border-red-300';
      default:
        return 'bg-slate-100 text-slate-700 border-slate-300';
    }
  }
</script>

<section class="glass fade-up rounded-2xl p-4 sm:p-5 md:p-6">
  <div class="mb-4 flex flex-wrap items-start justify-between gap-3">
    <div>
      <h2 class="text-xl font-bold tracking-tight">Normalize Execute</h2>
      <p class="mt-1 text-xs text-slate-700">
        Launch dry-runs or live execution with idempotent replay-safe requests.
      </p>
    </div>
    <div class="flex w-full flex-wrap items-center gap-2 text-xs text-slate-700 sm:w-auto sm:justify-end">
      <span class={`rounded-full border px-2 py-0.5 font-semibold uppercase tracking-wide ${statusBadgeClass}`}>
        {currentStatus}
      </span>
      <span>Polling: {polling ? 'on' : 'off'}</span>
      <button
        class="rounded border border-slate-300 px-2 py-1 sm:px-2"
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
  <p class="mt-2 text-xs text-slate-600">
    Reuse the same key to safely replay status without re-applying filesystem operations.
  </p>

  <div class="mt-4 flex flex-wrap gap-2">
    <button
      class="w-full rounded-lg border border-amber-400 bg-amber-50 px-3 py-2 text-xs font-semibold text-amber-900 hover:bg-amber-100 sm:w-auto"
      disabled={submitting}
      onclick={onDryRun}
      type="button"
    >
      Start Dry Run
    </button>
    <button
      class="w-full rounded-lg bg-teal-700 px-3 py-2 text-xs font-semibold text-white hover:bg-teal-800 sm:w-auto"
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

  <article class="mt-4 rounded-xl border border-slate-200 bg-slate-50 p-3 sm:p-4">
    <h3 class="text-sm font-semibold text-slate-900">Job Progress</h3>
    <div class="mt-2 grid gap-2 text-[11px] text-slate-700 sm:grid-cols-2 sm:text-xs">
      <p>Job ID: <span class="font-medium text-slate-900">{jobId || 'not started'}</span></p>
      <p>
        Completion: <span class="font-medium text-slate-900">{completedOps}/{totalOps}</span>
        ({progressPercent}%)
      </p>
      <p>Failed Ops: <span class="font-medium text-slate-900">{failedOps}</span></p>
      <p>
        Remaining: <span class="font-medium text-slate-900">{Math.max(totalOps - completedOps, 0)}</span>
      </p>
    </div>

    <div class="mt-3 h-2 w-full rounded bg-slate-200">
      <div
        class="h-2 rounded bg-gradient-to-r from-teal-500 to-cyan-500"
        style={`width: ${progressPercent}%`}
      ></div>
    </div>
  </article>

  <article class="mt-4 rounded-xl border border-slate-200 bg-slate-50 p-3 sm:p-4">
    <div class="flex flex-wrap items-center justify-between gap-3">
      <h3 class="text-sm font-semibold text-slate-900">Operations</h3>
      <div class="flex w-full items-center gap-2 text-xs text-slate-700 sm:w-auto sm:justify-end">
        <button
          class="w-full rounded border border-slate-300 px-2 py-1 sm:w-auto"
          disabled={!jobId || loadingEntries}
          onclick={onRefreshEntries}
          type="button"
        >
          {loadingEntries ? 'Loading...' : 'Refresh Entries'}
        </button>
      </div>
    </div>

    <div class="mt-2 flex flex-wrap items-center gap-2 text-xs text-slate-700">
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
      <span class="w-full text-[11px] sm:w-auto sm:text-xs">Page {entriesPage} of {entriesTotalPages || 1} ({entriesTotal} ops)</span>
    </div>

    <div class="mt-3 max-h-72 space-y-2 overflow-auto rounded-lg border border-slate-200 bg-white p-2 sm:max-h-64 sm:p-3">
      {#if !entries.length}
        <p class="text-xs text-slate-600">No operations loaded yet.</p>
      {:else}
        {#each entries as entry}
          <div class="rounded-xl border border-slate-200 p-2 text-[11px] sm:p-3 sm:text-xs">
            <div class="flex flex-wrap items-center justify-between gap-2">
              <p class="font-semibold text-slate-900">
                #{entry.op_index} {entry.op_type}
              </p>
              <span class={`rounded-full border px-2 py-0.5 text-[10px] font-semibold uppercase tracking-wide ${entryStatusClass(entry.status)}`}>
                {entry.status}
              </span>
            </div>
            <p class="mt-2 text-slate-700 break-all">{entry.source_path}</p>
            <p class="text-slate-500 break-all">to {entry.target_path}</p>
            {#if entry.error_message}
              <p class="mt-2 rounded-lg border border-red-200 bg-red-50 px-2 py-1 text-red-700">
                Error: {entry.error_message}
              </p>
            {/if}
          </div>
        {/each}
      {/if}
    </div>
  </article>
</section>
