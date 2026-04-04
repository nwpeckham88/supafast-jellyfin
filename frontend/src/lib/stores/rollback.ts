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

export interface RollbackStoreState {
  sessions: RollbackSessionSummary[];
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
    async refreshSessions() {
      update((state) => ({ ...state, loadingSessions: true, error: null }));

      try {
        const response = await listRollbackSessions();
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
            selectedSessionId: nextSelectedSessionId,
            preview: selectionChanged ? null : state.preview,
            result: selectionChanged ? null : state.result,
            loadingSessions: false
          };
        });
      } catch (error) {
        const message =
          error instanceof Error ? error.message : 'failed to list rollback sessions';
        update((state) => ({ ...state, loadingSessions: false, error: message }));
      }
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
      } catch (error) {
        const message =
          error instanceof Error ? error.message : 'failed to preview rollback';
        update((state) => ({ ...state, loadingPreview: false, error: message }));
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
      } catch (error) {
        const message = error instanceof Error ? error.message : 'rollback execute failed';
        update((state) => ({ ...state, submitting: false, error: message }));
      }
    },
    reset() {
      set({ ...initialState });
    }
  };
}

export const rollbackStore = createRollbackStore();
