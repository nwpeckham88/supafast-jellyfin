import type {
  CreateScanPayload,
  ExpireScanResult,
  ScanResult
} from '$lib/models/normalize';

import { request } from '$lib/api/client';

export function createScan(payload: CreateScanPayload): Promise<ScanResult> {
  return request<ScanResult>('/api/normalize/scan', {
    method: 'POST',
    body: JSON.stringify(payload)
  });
}

export function getScan(scanId: string): Promise<ScanResult> {
  return request<ScanResult>(`/api/normalize/scan/${scanId}`);
}

export function expireScan(scanId: string): Promise<ExpireScanResult> {
  return request<ExpireScanResult>(`/api/normalize/scan/${scanId}`, {
    method: 'DELETE'
  });
}