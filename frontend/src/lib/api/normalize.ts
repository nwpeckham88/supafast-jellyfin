import type {
  CreateScanPayload,
  ExecuteNormalizeRequest,
  ExecuteNormalizeResponse,
  ExpireScanResult,
  ScanDefaults,
  NormalizeJobEntriesResponse,
  NormalizeJobStatusResponse,
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

export function getScanDefaults(): Promise<ScanDefaults> {
  return request<ScanDefaults>('/api/normalize/config');
}

export function getScan(scanId: string): Promise<ScanResult> {
  return request<ScanResult>(`/api/normalize/scan/${scanId}`);
}

export function expireScan(scanId: string): Promise<ExpireScanResult> {
  return request<ExpireScanResult>(`/api/normalize/scan/${scanId}`, {
    method: 'DELETE'
  });
}

export function listRollbackSessions(
  page = 1,
  pageSize = 50
): Promise<RollbackSessionsResponse> {
  return request<RollbackSessionsResponse>(
    `/api/rollback/sessions?page=${page}&page_size=${pageSize}`
  );
}

export function executeNormalize(
  payload: ExecuteNormalizeRequest
): Promise<ExecuteNormalizeResponse> {
  return request<ExecuteNormalizeResponse>('/api/normalize/execute', {
    method: 'POST',
    body: JSON.stringify(payload)
  });
}

export function getNormalizeJobStatus(jobId: string): Promise<NormalizeJobStatusResponse> {
  return request<NormalizeJobStatusResponse>(`/api/normalize/jobs/${jobId}`);
}

export function getNormalizeJobEntries(
  jobId: string,
  page: number,
  pageSize: number
): Promise<NormalizeJobEntriesResponse> {
  return request<NormalizeJobEntriesResponse>(
    `/api/normalize/jobs/${jobId}/entries?page=${page}&page_size=${pageSize}`
  );
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