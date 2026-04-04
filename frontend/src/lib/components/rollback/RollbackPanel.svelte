<script lang="ts">
  import type {
    RollbackExecuteResponse,
    RollbackPreviewResponse,
    RollbackSessionSummary
  } from '$lib/models/normalize';

  interface Props {
    sessions: RollbackSessionSummary[];
    sessionsPage: number;
    sessionsTotalPages: number;
    sessionsTotal: number;
    selectedSessionId: string;
    idempotencyKey: string;
    preview: RollbackPreviewResponse | null;
    result: RollbackExecuteResponse | null;
    loadingSessions: boolean;
    loadingPreview: boolean;
    submitting: boolean;
    error: string | null;
    onSelectSession: (value: string) => void;
    onIdempotencyKeyChange: (value: string) => void;
    onRefreshSessions: () => void;
    onPreviousSessionsPage: () => void;
    onNextSessionsPage: () => void;
    onLoadPreview: () => void;
    onDryRun: () => void;
    onExecute: () => void;
  }

  let {
    sessions,
    sessionsPage,
    sessionsTotalPages,
    sessionsTotal,
    selectedSessionId,
    idempotencyKey,
    preview,
    result,
    loadingSessions,
    loadingPreview,
    submitting,
    error,
    onSelectSession,
    onIdempotencyKeyChange,
    onRefreshSessions,
    onPreviousSessionsPage,
    onNextSessionsPage,
    onLoadPreview,
    onDryRun,
    onExecute
  }: Props = $props();

  const selectedSession = $derived(
    sessions.find((session) => session.session_id === selectedSessionId) ?? null
  );
  const blockedPreviewOperations = $derived(
    preview ? preview.operations.filter((operation) => Boolean(operation.blocked_reason)).length : 0
  );

  function sessionStatusClass(status: string): string {
    switch (status) {
      case 'completed':
        return 'bg-teal-100 text-teal-900 border-teal-300';
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
      <h2 class="text-xl font-bold tracking-tight">Rollback Console</h2>
      <p class="mt-1 text-xs text-slate-700">
        Preview and revert previously audited operations in reverse order.
      </p>
    </div>
    <button
      class="w-full rounded-lg border border-slate-300 bg-white px-3 py-2 text-xs font-semibold text-slate-800 hover:bg-slate-100 sm:w-auto"
      disabled={loadingSessions || submitting}
      onclick={onRefreshSessions}
      type="button"
    >
      {loadingSessions ? 'Loading Sessions...' : 'Refresh Sessions'}
    </button>
  </div>

  <div class="mb-3 flex flex-wrap items-center gap-2 text-xs text-slate-700">
    <button
      class="rounded border border-slate-300 px-2 py-1"
      disabled={loadingSessions || sessionsPage <= 1}
      onclick={onPreviousSessionsPage}
      type="button"
    >
      Prev
    </button>
    <button
      class="rounded border border-slate-300 px-2 py-1"
      disabled={loadingSessions || sessionsTotalPages === 0 || sessionsPage >= sessionsTotalPages}
      onclick={onNextSessionsPage}
      type="button"
    >
      Next
    </button>
    <span class="w-full text-[11px] sm:w-auto sm:text-xs">Page {sessionsPage} of {sessionsTotalPages || 1} ({sessionsTotal} sessions)</span>
  </div>

  <div class="grid gap-4 md:grid-cols-2">
    <label class="text-sm font-medium text-slate-800" for="rollback-session-select">
      Rollback Session
      <select
        class="mt-2 w-full rounded-lg border border-slate-300 bg-white px-3 py-2 text-sm text-slate-900"
        id="rollback-session-select"
        onchange={(event) => onSelectSession((event.currentTarget as HTMLSelectElement).value)}
        value={selectedSessionId}
      >
        <option value="">Select a session</option>
        {#each sessions as session}
          <option value={session.session_id}>
            {session.media_type.toUpperCase()} | {session.session_id.slice(0, 8)}... | {session.status}
          </option>
        {/each}
      </select>
    </label>

    <label class="text-sm font-medium text-slate-800" for="rollback-idempotency-key">
      Idempotency Key
      <input
        class="mt-2 w-full rounded-lg border border-slate-300 bg-white px-3 py-2 text-sm text-slate-900"
        id="rollback-idempotency-key"
        maxlength="128"
        oninput={(event) =>
          onIdempotencyKeyChange((event.currentTarget as HTMLInputElement).value)}
        placeholder="rollback-2026-04-04"
        type="text"
        value={idempotencyKey}
      />
    </label>
  </div>

  {#if selectedSession}
    <article class="mt-3 rounded-lg border border-slate-200 bg-slate-50 px-3 py-2 text-[11px] text-slate-700 sm:text-xs">
      <div class="flex flex-wrap items-center gap-2">
        <span class={`rounded-full border px-2 py-0.5 font-semibold uppercase tracking-wide ${sessionStatusClass(selectedSession.status)}`}>
          {selectedSession.status}
        </span>
        <span>Ops: {selectedSession.completed_ops}/{selectedSession.total_ops}</span>
        <span>Failed: {selectedSession.failed_ops}</span>
      </div>
      <p class="mt-1 break-all">
        Base path: <span class="font-medium text-slate-800">{selectedSession.base_path}</span>
      </p>
    </article>
  {/if}

  <div class="mt-4 flex flex-wrap gap-2">
    <button
      class="w-full rounded-lg border border-slate-300 bg-white px-3 py-2 text-xs font-semibold text-slate-800 hover:bg-slate-100 sm:w-auto"
      disabled={!selectedSessionId || loadingPreview || submitting}
      onclick={onLoadPreview}
      type="button"
    >
      {loadingPreview ? 'Building Preview...' : 'Load Preview'}
    </button>
    <button
      class="w-full rounded-lg border border-amber-400 bg-amber-50 px-3 py-2 text-xs font-semibold text-amber-900 hover:bg-amber-100 sm:w-auto"
      disabled={!preview || submitting}
      onclick={onDryRun}
      type="button"
    >
      Dry Run Execute
    </button>
    <button
      class="w-full rounded-lg bg-teal-700 px-3 py-2 text-xs font-semibold text-white hover:bg-teal-800 sm:w-auto"
      disabled={!preview || submitting}
      onclick={onExecute}
      type="button"
    >
      Execute Rollback
    </button>
  </div>

  {#if error}
    <aside class="mt-4 rounded-xl border border-red-300 bg-red-50 p-3 text-sm text-red-800">
      {error}
    </aside>
  {/if}

  {#if preview}
    <article class="mt-4 rounded-xl border border-slate-200 bg-slate-50 p-3 sm:p-4">
      <h3 class="text-sm font-semibold text-slate-900">Rollback Preview</h3>
      <div class="mt-2 grid gap-2 text-[11px] text-slate-700 sm:grid-cols-3 sm:text-xs">
        <p>
          Total: <span class="font-semibold text-slate-900">{preview.operation_count}</span>
        </p>
        <p>
          Blocked: <span class="font-semibold text-red-700">{preview.blocked_count}</span>
        </p>
        <p>
          Ready: <span class="font-semibold text-teal-700">{preview.operation_count - blockedPreviewOperations}</span>
        </p>
      </div>

      <div class="mt-3 max-h-72 space-y-2 overflow-auto rounded-lg border border-slate-200 bg-white p-2 sm:max-h-56 sm:p-3">
        {#each preview.operations as operation}
          <div
            class="rounded-lg border p-2 text-[11px] sm:text-xs"
            class:border-red-200={Boolean(operation.blocked_reason)}
            class:bg-red-50={Boolean(operation.blocked_reason)}
            class:border-slate-200={!operation.blocked_reason}
            class:bg-white={!operation.blocked_reason}
          >
            <div class="flex flex-wrap items-center justify-between gap-2">
              <p class="font-semibold text-slate-900">
                #{operation.op_index} {operation.op_type}
              </p>
              {#if operation.blocked_reason}
                <span class="rounded-full border border-red-300 bg-red-100 px-2 py-0.5 text-[10px] font-semibold uppercase tracking-wide text-red-900">
                  blocked
                </span>
              {:else}
                <span class="rounded-full border border-teal-300 bg-teal-100 px-2 py-0.5 text-[10px] font-semibold uppercase tracking-wide text-teal-900">
                  ready
                </span>
              {/if}
            </div>
            <p class="mt-1 break-all text-slate-700">{operation.source_path}</p>
            <p class="break-all text-slate-500">to {operation.target_path}</p>
            {#if operation.blocked_reason}
              <p class="mt-1 rounded border border-red-200 bg-red-100 px-2 py-1 text-red-700">
                Blocked: {operation.blocked_reason}
              </p>
            {/if}
          </div>
        {/each}
      </div>
    </article>
  {/if}

  {#if result}
    <article class="mt-4 rounded-xl border border-teal-300 bg-teal-50 p-3 text-sm text-teal-900">
      <p class="font-semibold">
        Rollback {result.status}. Reverted: {result.reverted_operations}. Replay: {result.replayed ? 'yes' : 'no'}.
      </p>
      <p class="mt-1 text-xs">Rollback job: {result.rollback_job_id}</p>
    </article>
  {/if}
</section>
