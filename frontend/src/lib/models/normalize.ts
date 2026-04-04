export type MediaType = 'tv' | 'movie';

export interface ScanSummary {
  total_shows: number;
  total_episodes: number;
  merges: number;
  conflicts: number;
}

export interface ScanGroup {
  normalized_name: string;
  source_directories: string[];
}

export interface ScanResult {
  scan_id: string;
  media_type: MediaType;
  groups: ScanGroup[];
  summary: ScanSummary;
}

export interface CreateScanPayload {
  media_type: MediaType;
  base_path: string;
}

export interface ExpireScanResult {
  scan_id: string;
  expired: boolean;
}

export interface ConflictCandidate {
  id: string;
  sourceDirectory: string;
}

export interface ConflictItem {
  id: string;
  normalizedName: string;
  candidates: ConflictCandidate[];
}

export interface RollbackSessionSummary {
  session_id: string;
  scan_id: string;
  media_type: MediaType;
  base_path: string;
  status: string;
  total_ops: number;
  completed_ops: number;
  failed_ops: number;
}

export interface RollbackSessionsResponse {
  sessions: RollbackSessionSummary[];
}

export interface RollbackOperationPreview {
  op_id: string;
  op_index: number;
  op_type: string;
  source_path: string;
  target_path: string;
  blocked_reason: string | null;
}

export interface RollbackPreviewResponse {
  session_id: string;
  operation_count: number;
  blocked_count: number;
  operations: RollbackOperationPreview[];
}

export interface RollbackExecuteRequest {
  session_id: string;
  idempotency_key: string;
  dry_run: boolean;
}

export interface RollbackExecuteResponse {
  rollback_job_id: string;
  session_id: string;
  status: 'pending' | 'running' | 'completed' | 'failed';
  reverted_operations: number;
  replayed: boolean;
}