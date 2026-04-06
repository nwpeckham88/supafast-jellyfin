import { writable } from 'svelte/store';

import {
  executeNormalize,
  getNormalizeJobEntries,
  getNormalizeJobStatus
} from '$lib/api/normalize';
import type {
  ConflictItem,
  ExecuteNormalizeResponse,
  MediaType,
  NormalizeAuditEntry,
  NormalizeJobStatusResponse,
  ScanResult
} from '$lib/models/normalize';
import { notificationsStore } from '$lib/stores/notifications';
import { generateUuid } from '$lib/utils/idempotency';

interface ExecuteStartInput {
  scanResult: ScanResult | null;
  idempotencyKey: string;
  dryRun: boolean;
  conflictItems: ConflictItem[];
  conflictSelections: Record<string, string>;
}

export interface ExecuteStoreState {
  scanId: string;
  mediaType: MediaType;
  basePath: string;
  idempotencyKey: string;
  jobId: string;
  status: 'pending' | 'running' | 'completed' | 'failed' | null;
  totalOps: number;
  completedOps: number;
  failedOps: number;
  error: string | null;
  submitting: boolean;
  polling: boolean;
  loadingEntries: boolean;
  entries: NormalizeAuditEntry[];
  entriesPage: number;
  entriesPageSize: number;
  entriesTotal: number;
  entriesTotalPages: number;
  replayed: boolean;
}

const initialState: ExecuteStoreState = {
  scanId: '',
  mediaType: 'tv',
  basePath: '',
  idempotencyKey: '',
  jobId: '',
  status: null,
  totalOps: 0,
  completedOps: 0,
  failedOps: 0,
  error: null,
  submitting: false,
  polling: false,
  loadingEntries: false,
  entries: [],
  entriesPage: 1,
  entriesPageSize: 50,
  entriesTotal: 0,
  entriesTotalPages: 0,
  replayed: false
};

function createExecuteStore() {
  const { subscribe, update, set } = writable<ExecuteStoreState>(initialState);
  let pollingTimer: ReturnType<typeof setInterval> | null = null;
  let entriesRefreshInFlight = false;

  function stopPolling() {
    if (pollingTimer) {
      clearInterval(pollingTimer);
      pollingTimer = null;
    }

    update((state) => ({ ...state, polling: false }));
  }

  async function refreshStatusInternal() {
    let jobId = '';
    let currentEntriesPage = 1;
    update((state) => {
      jobId = state.jobId;
      currentEntriesPage = state.entriesPage;
      return state;
    });

    if (!jobId) {
      return;
    }

    try {
      const status = await getNormalizeJobStatus(jobId);
      applyStatus(status);

      if (status.status === 'pending' || status.status === 'running') {
        if (!entriesRefreshInFlight) {
          entriesRefreshInFlight = true;
          try {
            await loadEntriesInternal(currentEntriesPage);
          } finally {
            entriesRefreshInFlight = false;
          }
        }
      }

      if (status.status === 'completed' || status.status === 'failed') {
        if (!entriesRefreshInFlight) {
          entriesRefreshInFlight = true;
          try {
            await loadEntriesInternal(currentEntriesPage);
          } finally {
            entriesRefreshInFlight = false;
          }
        }
        stopPolling();
      }
    } catch (error) {
      const message = error instanceof Error ? error.message : 'failed to refresh job status';
      update((state) => ({ ...state, error: message }));
      stopPolling();
      notificationsStore.error(`Failed to refresh normalize job: ${message}`);
    }
  }

  function applyStatus(status: NormalizeJobStatusResponse) {
    update((state) => ({
      ...state,
      status: status.status,
      totalOps: status.total_ops,
      completedOps: status.completed_ops,
      failedOps: status.failed_ops,
      error: status.error
    }));
  }

  async function loadEntriesInternal(page: number) {
    let jobId = '';
    let pageSize = 50;
    update((state) => {
      jobId = state.jobId;
      pageSize = state.entriesPageSize;
      return { ...state, loadingEntries: true, error: null };
    });

    if (!jobId) {
      update((state) => ({ ...state, loadingEntries: false }));
      return;
    }

    try {
      const response = await getNormalizeJobEntries(jobId, page, pageSize);
      update((state) => ({
        ...state,
        entries: response.entries,
        entriesPage: response.pagination.page,
        entriesPageSize: response.pagination.page_size,
        entriesTotal: response.pagination.total,
        entriesTotalPages: response.pagination.total_pages,
        loadingEntries: false
      }));
    } catch (error) {
      const message = error instanceof Error ? error.message : 'failed to load job entries';
      update((state) => ({ ...state, loadingEntries: false, error: message }));
      notificationsStore.error(`Failed to load normalize entries: ${message}`);
    }
  }

  function startPolling() {
    stopPolling();
    update((state) => ({ ...state, polling: true }));
    pollingTimer = setInterval(() => {
      void refreshStatusInternal();
    }, 1500);
  }

  return {
    subscribe,
    setIdempotencyKey(idempotencyKey: string) {
      update((state) => ({ ...state, idempotencyKey }));
    },
    generateIdempotencyKey() {
      update((state) => ({ ...state, idempotencyKey: generateUuid() }));
    },
    syncFromScan(scanResult: ScanResult | null) {
      if (!scanResult) {
        stopPolling();
        set({ ...initialState });
        return;
      }

      update((state) => ({
        ...state,
        scanId: scanResult.scan_id,
        mediaType: scanResult.media_type,
        basePath: scanResult.base_path,
        jobId: state.scanId === scanResult.scan_id ? state.jobId : '',
        status: state.scanId === scanResult.scan_id ? state.status : null,
        totalOps: state.scanId === scanResult.scan_id ? state.totalOps : 0,
        completedOps: state.scanId === scanResult.scan_id ? state.completedOps : 0,
        failedOps: state.scanId === scanResult.scan_id ? state.failedOps : 0,
        entries: state.scanId === scanResult.scan_id ? state.entries : [],
        entriesPage: 1,
        entriesTotal: state.scanId === scanResult.scan_id ? state.entriesTotal : 0,
        entriesTotalPages: state.scanId === scanResult.scan_id ? state.entriesTotalPages : 0,
        error: null
      }));
    },
    async startExecute(input: ExecuteStartInput) {
      if (!input.scanResult) {
        update((state) => ({ ...state, error: 'create a scan before execute' }));
        return;
      }

      const normalizedKey = input.idempotencyKey.trim();
      if (!normalizedKey) {
        update((state) => ({ ...state, error: 'idempotency key is required' }));
        return;
      }

      const conflictResolutions = input.conflictItems
        .map((item) => {
          const candidateId = input.conflictSelections[item.id];
          if (!candidateId) {
            return null;
          }

          const candidate = item.candidates.find((entry) => entry.id === candidateId);
          if (!candidate) {
            return null;
          }

          return {
            normalized_name: item.normalizedName,
            selected_source_directory: candidate.sourceDirectory
          };
        })
        .filter((value): value is { normalized_name: string; selected_source_directory: string } =>
          value !== null
        );

      if (conflictResolutions.length !== input.conflictItems.length) {
        update((state) => ({
          ...state,
          error: 'resolve all conflicts before execute'
        }));
        return;
      }

      update((state) => ({
        ...state,
        submitting: true,
        error: null,
        idempotencyKey: normalizedKey
      }));

      try {
        const response = await executeNormalize({
          scan_id: input.scanResult.scan_id,
          idempotency_key: normalizedKey,
          media_type: input.scanResult.media_type,
          base_path: input.scanResult.base_path,
          name_overrides: [],
          conflict_resolutions: conflictResolutions,
          dry_run: input.dryRun
        });

        applyExecuteResponse(response);
        await refreshStatusInternal();
        await loadEntriesInternal(1);

        if (response.replayed) {
          notificationsStore.info('Replayed existing normalize job from idempotency key.');
        } else {
          notificationsStore.success(
            input.dryRun
              ? 'Normalize dry-run started.'
              : 'Normalize execute started.'
          );
        }
      } catch (error) {
        const message = error instanceof Error ? error.message : 'normalize execute failed';
        update((state) => ({ ...state, submitting: false, error: message }));
        notificationsStore.error(`Normalize execute failed: ${message}`);
      }
    },
    async refreshStatus() {
      await refreshStatusInternal();
    },
    async refreshEntries() {
      let page = 1;
      update((state) => {
        page = state.entriesPage;
        return state;
      });
      await loadEntriesInternal(page);
    },
    async nextEntriesPage() {
      let page = 1;
      let totalPages = 0;
      update((state) => {
        page = state.entriesPage;
        totalPages = state.entriesTotalPages;
        return state;
      });

      if (totalPages === 0 || page >= totalPages) {
        return;
      }

      await loadEntriesInternal(page + 1);
    },
    async previousEntriesPage() {
      let page = 1;
      update((state) => {
        page = state.entriesPage;
        return state;
      });

      if (page <= 1) {
        return;
      }

      await loadEntriesInternal(page - 1);
    },
    reset() {
      stopPolling();
      set({ ...initialState });
    },
    destroy() {
      stopPolling();
    }
  };

  function applyExecuteResponse(response: ExecuteNormalizeResponse) {
    update((state) => ({
      ...state,
      jobId: response.job_id,
      status: response.status,
      replayed: response.replayed,
      submitting: false,
      entries: [],
      entriesPage: 1,
      entriesTotal: 0,
      entriesTotalPages: 0
    }));

    if (response.status === 'pending' || response.status === 'running') {
      startPolling();
    } else {
      stopPolling();
    }
  }
}

export const executeStore = createExecuteStore();
