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
</script>

<section class="glass fade-up rounded-2xl p-5 md:p-6">
  <div class="mb-4 flex flex-wrap items-center justify-between gap-3">
    <h2 class="text-xl font-bold tracking-tight">Rollback Console</h2>
    <button
      class="rounded-lg border border-slate-300 bg-white px-3 py-2 text-xs font-semibold text-slate-800 hover:bg-slate-100"
      disabled={loadingSessions || submitting}
      onclick={onRefreshSessions}
      type="button"
    >
      {loadingSessions ? 'Loading Sessions...' : 'Refresh Sessions'}
    </button>
  </div>

  <div class="mb-3 flex items-center gap-2 text-xs text-slate-700">
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
    <span>Page {sessionsPage} of {sessionsTotalPages || 1} ({sessionsTotal} sessions)</span>
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
    <p class="mt-3 text-xs text-slate-600">
      Base path: <span class="font-medium text-slate-800">{selectedSession.base_path}</span>
    </p>
  {/if}

  <div class="mt-4 flex flex-wrap gap-2">
    <button
      class="rounded-lg border border-slate-300 bg-white px-3 py-2 text-xs font-semibold text-slate-800 hover:bg-slate-100"
      disabled={!selectedSessionId || loadingPreview || submitting}
      onclick={onLoadPreview}
      type="button"
    >
      {loadingPreview ? 'Building Preview...' : 'Load Preview'}
    </button>
    <button
      class="rounded-lg border border-amber-400 bg-amber-50 px-3 py-2 text-xs font-semibold text-amber-900 hover:bg-amber-100"
      disabled={!preview || submitting}
      onclick={onDryRun}
      type="button"
    >
      Dry Run Execute
    </button>
    <button
      class="rounded-lg bg-teal-700 px-3 py-2 text-xs font-semibold text-white hover:bg-teal-800"
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
    <article class="mt-4 rounded-xl border border-slate-200 bg-slate-50 p-4">
      <h3 class="text-sm font-semibold text-slate-900">Rollback Preview</h3>
      <p class="mt-1 text-xs text-slate-600">
        {preview.operation_count} operations, {preview.blocked_count} blocked.
      </p>

      <div class="mt-3 max-h-56 space-y-2 overflow-auto rounded-lg border border-slate-200 bg-white p-3">
        {#each preview.operations as operation}
          <div class="rounded-lg border border-slate-200 p-2 text-xs">
            <p class="font-semibold text-slate-900">
              #{operation.op_index} {operation.op_type}
            </p>
            <p class="mt-1 text-slate-700">{operation.source_path}</p>
            <p class="text-slate-500">to {operation.target_path}</p>
            {#if operation.blocked_reason}
              <p class="mt-1 text-red-700">Blocked: {operation.blocked_reason}</p>
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
