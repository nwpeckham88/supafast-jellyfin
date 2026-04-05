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
  base_path: string;
  groups: ScanGroup[];
  summary: ScanSummary;
}

export interface CreateScanPayload {
  media_type: MediaType;
}

export interface ScanDefaults {
  tv_base_path: string;
  movies_base_path: string;
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
  pagination: PaginationMeta;
}

export interface PaginationMeta {
  page: number;
  page_size: number;
  total: number;
  total_pages: number;
}

export interface ExecuteNormalizeRequest {
  scan_id: string;
  idempotency_key: string;
  media_type: MediaType;
  base_path: string;
  name_overrides: Array<{ source_directory: string; normalized_name: string }>;
  conflict_resolutions: Array<{
    normalized_name: string;
    selected_source_directory: string;
  }>;
  dry_run: boolean;
}

export interface ExecuteNormalizeResponse {
  job_id: string;
  scan_id: string;
  status: 'pending' | 'running' | 'completed' | 'failed';
  replayed: boolean;
}

export interface NormalizeJobStatusResponse {
  job_id: string;
  scan_id: string;
  status: 'pending' | 'running' | 'completed' | 'failed';
  plan_hash: string;
  total_ops: number;
  completed_ops: number;
  failed_ops: number;
  error: string | null;
  entries: NormalizeAuditEntry[];
}

export interface NormalizeJobEntriesResponse {
  job_id: string;
  entries: NormalizeAuditEntry[];
  pagination: PaginationMeta;
}

export interface NormalizeAuditEntry {
  op_id: string;
  op_index: number;
  session_id: string;
  op_type: string;
  source_path: string;
  target_path: string;
  file_size: number | null;
  status: 'pending' | 'running' | 'succeeded' | 'failed';
  error_message: string | null;
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