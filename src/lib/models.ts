export interface ModelInfo {
  id: string;
  name: string;
  description: string;
  size_bytes: number;
  filename: string;
  recommended: boolean;
}

export interface LlmModelInfo {
  id: string;
  name: string;
  description: string;
  size_bytes: number;
  filename: string;
  /** Present when the backend marks this option as recommended */
  recommended?: boolean;
}

export interface DownloadProgress {
  model_id: string;
  downloaded_bytes: number;
  total_bytes: number;
}

export function formatSize(bytes: number): string {
  if (bytes >= 1_000_000_000) return `${(bytes / 1_000_000_000).toFixed(1)} GB`;
  if (bytes >= 1_000_000) return `${(bytes / 1_000_000).toFixed(0)} MB`;
  return `${(bytes / 1_000).toFixed(0)} KB`;
}
