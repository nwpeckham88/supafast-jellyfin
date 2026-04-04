import type {
  CreateScanPayload,
  ExpireScanResult,
  RollbackExecuteRequest,
  RollbackExecuteResponse,
  RollbackPreviewResponse,
  RollbackSessionsResponse,
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

export function listRollbackSessions(): Promise<RollbackSessionsResponse> {
  return request<RollbackSessionsResponse>('/api/rollback/sessions');
}

export function previewRollback(sessionId: string): Promise<RollbackPreviewResponse> {
  return request<RollbackPreviewResponse>('/api/rollback/preview', {
    method: 'POST',
    body: JSON.stringify({ session_id: sessionId })
  });
}

export function executeRollback(
  payload: RollbackExecuteRequest
): Promise<RollbackExecuteResponse> {
  return request<RollbackExecuteResponse>('/api/rollback/execute', {
    method: 'POST',
    body: JSON.stringify(payload)
  });
}