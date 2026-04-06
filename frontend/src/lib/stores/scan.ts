import { writable } from 'svelte/store';

import { createScan, expireScan, getScan, getScanDefaults } from '$lib/api/normalize';
import type { MediaType, ScanDefaults, ScanResult } from '$lib/models/normalize';
import { notificationsStore } from '$lib/stores/notifications';

export interface ScanStoreState {
  mediaType: MediaType;
  activeBasePath: string;
  scanResult: ScanResult | null;
  loading: boolean;
  error: string | null;
}

const initialState: ScanStoreState = {
  mediaType: 'tv',
  activeBasePath: '/media/tv',
  scanResult: null,
  loading: false,
  error: null
};

const fallbackDefaults: ScanDefaults = {
  tv_base_path: '/media/tv',
  movies_base_path: '/media/movies'
};

function createScanStore() {
  const { subscribe, update, set } = writable<ScanStoreState>(initialState);
  let defaultsLoaded = false;
  let defaults = fallbackDefaults;

  function basePathFor(mediaType: MediaType): string {
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
        defaults = fallbackDefaults;
      } finally {
        defaultsLoaded = true;
      }

      update((state) => ({
        ...state,
        activeBasePath: basePathFor(state.mediaType)
      }));
    },
    setMediaType(mediaType: MediaType) {
      update((state) => ({
        ...state,
        mediaType,
        activeBasePath: basePathFor(mediaType)
      }));
    },
    async startScan() {
      let requestMediaType: MediaType = 'tv';
      let requestBasePath = '';

      update((state) => {
        requestMediaType = state.mediaType;
        requestBasePath = state.activeBasePath;
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