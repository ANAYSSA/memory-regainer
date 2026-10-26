export interface Source { id: string; name: string; path: string; size: number; kind: string; accessible: boolean; detail: string }
export type DeletionStatus = 'existing' | 'deleted' | 'unknown';
export interface Entry { id: string; name: string; kind: string; size: number; offset: number; confidence: string; method: string; notes: string[]; extents: { offset: number; length: number }[]; relative_path?: string | null; partition?: string | null; deletion_status?: DeletionStatus }
export interface Progress { scanned: number; total: number; found: number; bytes_read?: number; current_offset?: number | null; sector_size?: number | null; source_size?: number }
export interface RecoveryReport { recovered: number; failed: number; bytes: number; destination: string; files: { name: string; status: string; sha256: string; error?: string | null; relative_path?: string | null }[] }
export interface Partition { name: string; offset: number; size: number; scheme: string; filesystem: string }
export interface Job { id: string; mode: string; sourceName: string; sourceId?: string; sourceSize?: number; stage: string; elapsedSeconds: number; progressDeterminate: boolean; sleepInhibited: boolean; sleepInhibitionError: string | null; status: 'running' | 'completed' | 'cancelled' | 'failed'; progress: Progress; entries: Entry[]; entryCount?: number; resultsState?: 'scanning' | 'partial' | 'complete' | 'invalid-source'; canPreview?: boolean; canRecover?: boolean; workerHeartbeatAgeSeconds?: number; diagnostics?: { candidates: number; accepted: number; rejected: number; reasons: Record<string, number> }; partitions?: Partition[]; methodNotes?: string[]; warnings: string[]; error: string | null; report?: RecoveryReport | null }
export interface ResultPage { entries: Entry[]; next: number; total: number; hasMore: boolean }
export interface Format { id: string; label: string; aliases: string[]; family: string; enabled: boolean }
export interface Preview { hex: string; text: string | null; dataUrl: string | null }
export interface Session { id: string; sourceName: string; date: string; count: number }
export type Lang = 'ru' | 'en';
export type Section = 'home' | 'recover' | 'images' | 'sessions' | 'settings' | 'about';
