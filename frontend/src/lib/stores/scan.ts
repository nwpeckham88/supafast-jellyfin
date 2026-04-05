import { writable } from 'svelte/store';

import { createScan, expireScan, getScan, getScanDefaults } from '$lib/api/normalize';
import type { MediaType, ScanDefaults, ScanResult } from '$lib/models/normalize';
import { notificationsStore } from '$lib/stores/notifications';

export interface ScanStoreState {
  mediaType: MediaType;
  basePath: string;
  scanResult: ScanResult | null;
  loading: boolean;
  error: string | null;
}

const initialState: ScanStoreState = {
  mediaType: 'tv',
  basePath: '',
  scanResult: null,
  loading: false,
  error: null
};

const FALLBACK_DEFAULTS: ScanDefaults = {
  tv_base_path: '/media/tv',
  movies_base_path: '/media/movies'
};

function createScanStore() {
  const { subscribe, update, set } = writable<ScanStoreState>(initialState);
  let defaults: ScanDefaults = FALLBACK_DEFAULTS;
  let defaultsLoaded = false;

  function defaultBasePathFor(mediaType: MediaType): string {
    return mediaType === 'tv' ? defaults.tv_base_path : defaults.movies_base_path;
  }

  return {
    subscribe,
    async hydrateDefaults() {
      if (defaultsLoaded) {
        return;
      }

      try {
        defaults = await getScanDefaults();
      } catch {
        defaults = FALLBACK_DEFAULTS;
      } finally {
        defaultsLoaded = true;
      }

      update((state) => {
        const nextBasePath = state.basePath.trim().length > 0
          ? state.basePath
          : defaultBasePathFor(state.mediaType);

        return {
          ...state,
          basePath: nextBasePath
        };
      });
    },
    setMediaType(mediaType: MediaType) {
      update((state) => ({
        ...state,
        mediaType,
        basePath: defaultBasePathFor(mediaType)
      }));
    },
    setBasePath(basePath: string) {
      update((state) => ({ ...state, basePath }));
    },
    async startScan() {
      let requestBasePath = '';
      let requestMediaType: MediaType = 'tv';

      update((state) => {
        requestBasePath = state.basePath;
        requestMediaType = state.mediaType;
        return { ...state, loading: true, error: null };
      });

      try {
        const result = await createScan({
          media_type: requestMediaType,
          base_path: requestBasePath
        });
        update((state) => ({ ...state, scanResult: result, loading: false }));
        notificationsStore.success('Scan created successfully.');
      } catch (error) {
        const message = error instanceof Error ? error.message : 'scan failed';
        update((state) => ({ ...state, error: message, loading: false }));
        notificationsStore.error(`Scan failed: ${message}`);
      }
    },
    async refreshScan() {
      let scanId = '';
      update((state) => {
        scanId = state.scanResult?.scan_id ?? '';
        return { ...state, loading: true, error: null };
      });

      if (!scanId) {
        update((state) => ({ ...state, loading: false }));
        return;
      }

      try {
        const result = await getScan(scanId);
        update((state) => ({ ...state, scanResult: result, loading: false }));
        notificationsStore.info('Scan refreshed.');
      } catch (error) {
        const message = error instanceof Error ? error.message : 'scan refresh failed';
        update((state) => ({ ...state, error: message, loading: false }));
        notificationsStore.error(`Refresh failed: ${message}`);
      }
    },
    async clearScanSession(): Promise<boolean> {
      let scanId = '';
      update((state) => {
        scanId = state.scanResult?.scan_id ?? '';
        return { ...state, loading: true, error: null };
      });

      if (scanId) {
        try {
          await expireScan(scanId);
        } catch (error) {
          const message =
            error instanceof Error ? error.message : 'failed to expire scan';
          update((state) => ({ ...state, error: message, loading: false }));
          notificationsStore.error(`Expire scan failed: ${message}`);
          return false;
        }
      }

      set({ ...initialState });
      notificationsStore.success('Scan session cleared.');
      return true;
    }
  };
}

export const scanStore = createScanStore();