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