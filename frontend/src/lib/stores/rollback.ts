import { writable } from 'svelte/store';

import {
  executeRollback,
  listRollbackSessions,
  previewRollback
} from '$lib/api/normalize';
import type {
  RollbackExecuteResponse,
  RollbackPreviewResponse,
  RollbackSessionSummary
} from '$lib/models/normalize';
import { notificationsStore } from '$lib/stores/notifications';
import { generateUuid } from '$lib/utils/idempotency';

export interface RollbackStoreState {
  sessions: RollbackSessionSummary[];
  sessionsPage: number;
  sessionsPageSize: number;
  sessionsTotal: number;
  sessionsTotalPages: number;
  selectedSessionId: string;
  preview: RollbackPreviewResponse | null;
  result: RollbackExecuteResponse | null;
  idempotencyKey: string;
  loadingSessions: boolean;
  loadingPreview: boolean;
  submitting: boolean;
  error: string | null;
}

const initialState: RollbackStoreState = {
  sessions: [],
  sessionsPage: 1,
  sessionsPageSize: 50,
  sessionsTotal: 0,
  sessionsTotalPages: 0,
  selectedSessionId: '',
  preview: null,
  result: null,
  idempotencyKey: '',
  loadingSessions: false,
  loadingPreview: false,
  submitting: false,
  error: null
};

function createRollbackStore() {
  const { subscribe, update, set } = writable<RollbackStoreState>(initialState);

  async function refreshSessionsInternal() {
    let page = 1;
    let pageSize = 50;
    update((state) => {
      page = state.sessionsPage;
      pageSize = state.sessionsPageSize;
      return { ...state, loadingSessions: true, error: null };
    });

    try {
      const response = await listRollbackSessions(page, pageSize);
      update((state) => {
        const selectedStillExists = response.sessions.some(
          (session) => session.session_id === state.selectedSessionId
        );
        const fallbackSelection = response.sessions[0]?.session_id ?? '';
        const nextSelectedSessionId = selectedStillExists
          ? state.selectedSessionId
          : fallbackSelection;
        const selectionChanged = nextSelectedSessionId !== state.selectedSessionId;
        return {
          ...state,
          sessions: response.sessions,
          sessionsPage: response.pagination.page,
          sessionsPageSize: response.pagination.page_size,
          sessionsTotal: response.pagination.total,
          sessionsTotalPages: response.pagination.total_pages,
          selectedSessionId: nextSelectedSessionId,
          preview: selectionChanged ? null : state.preview,
          result: selectionChanged ? null : state.result,
          loadingSessions: false
        };
      });
      notificationsStore.info('Rollback sessions refreshed.');
    } catch (error) {
      const message =
        error instanceof Error ? error.message : 'failed to list rollback sessions';
      update((state) => ({ ...state, loadingSessions: false, error: message }));
      notificationsStore.error(`Failed to refresh rollback sessions: ${message}`);
    }
  }

  return {
    subscribe,
    setSelectedSession(sessionId: string) {
      update((state) => ({
        ...state,
        selectedSessionId: sessionId,
        preview: null,
        result: null
      }));
    },
    setIdempotencyKey(idempotencyKey: string) {
      update((state) => ({ ...state, idempotencyKey }));
    },
    generateIdempotencyKey() {
      update((state) => ({ ...state, idempotencyKey: generateUuid() }));
    },
    async refreshSessions() {
      await refreshSessionsInternal();
    },
    async setSessionsPage(page: number) {
      update((state) => ({
        ...state,
        sessionsPage: page < 1 ? 1 : page
      }));
      await refreshSessionsInternal();
    },
    async nextSessionsPage() {
      let page = 1;
      let totalPages = 0;
      update((state) => {
        page = state.sessionsPage;
        totalPages = state.sessionsTotalPages;
        return state;
      });

      if (totalPages === 0 || page >= totalPages) {
        return;
      }

      update((state) => ({ ...state, sessionsPage: page + 1 }));
      await refreshSessionsInternal();
    },
    async previousSessionsPage() {
      let page = 1;
      update((state) => {
        page = state.sessionsPage;
        return state;
      });

      if (page <= 1) {
        return;
      }

      update((state) => ({ ...state, sessionsPage: page - 1 }));
      await refreshSessionsInternal();
    },
    async loadPreview() {
      let sessionId = '';
      update((state) => {
        sessionId = state.selectedSessionId;
        return { ...state, loadingPreview: true, error: null, preview: null, result: null };
      });

      if (!sessionId) {
        update((state) => ({
          ...state,
          loadingPreview: false,
          error: 'select a rollback session first'
        }));
        return;
      }

      try {
        const preview = await previewRollback(sessionId);
        update((state) => ({ ...state, preview, loadingPreview: false }));
        notificationsStore.success('Rollback preview loaded.');
      } catch (error) {
        const message =
          error instanceof Error ? error.message : 'failed to preview rollback';
        update((state) => ({ ...state, loadingPreview: false, error: message }));
        notificationsStore.error(`Rollback preview failed: ${message}`);
      }
    },
    async runRollback(dryRun: boolean) {
      let sessionId = '';
      let idempotencyKey = '';
      update((state) => {
        sessionId = state.selectedSessionId;
        idempotencyKey = state.idempotencyKey.trim();
        return {
          ...state,
          idempotencyKey,
          submitting: true,
          error: null,
          result: null
        };
      });

      if (!sessionId || !idempotencyKey) {
        update((state) => ({
          ...state,
          submitting: false,
          error: 'session and idempotency key are required'
        }));
        return;
      }

      try {
        const result = await executeRollback({
          session_id: sessionId,
          idempotency_key: idempotencyKey,
          dry_run: dryRun
        });

        update((state) => ({ ...state, submitting: false, result }));
        notificationsStore.success(
          dryRun ? 'Rollback dry-run completed.' : 'Rollback completed successfully.'
        );
      } catch (error) {
        const message = error instanceof Error ? error.message : 'rollback execute failed';
        update((state) => ({ ...state, submitting: false, error: message }));
        notificationsStore.error(`Rollback execution failed: ${message}`);
      }
    },
    reset() {
      set({ ...initialState });
    }
  };
}

export const rollbackStore = createRollbackStore();
