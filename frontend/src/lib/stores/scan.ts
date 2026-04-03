import { writable } from 'svelte/store';

import { createScan, expireScan, getScan } from '$lib/api/normalize';
import type { MediaType, ScanResult } from '$lib/models/normalize';

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

function createScanStore() {
  const { subscribe, update, set } = writable<ScanStoreState>(initialState);

  return {
    subscribe,
    setMediaType(mediaType: MediaType) {
      update((state) => ({ ...state, mediaType }));
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
      } catch (error) {
        const message = error instanceof Error ? error.message : 'scan failed';
        update((state) => ({ ...state, error: message, loading: false }));
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
      } catch (error) {
        const message = error instanceof Error ? error.message : 'scan refresh failed';
        update((state) => ({ ...state, error: message, loading: false }));
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
          return false;
        }
      }

      set({ ...initialState });
      return true;
    }
  };
}

export const scanStore = createScanStore();