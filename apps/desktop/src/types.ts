export interface Source { id: string; name: string; path: string; size: number; kind: string; accessible: boolean; detail: string }
export interface Entry { id: string; name: string; kind: string; size: number; offset: number; confidence: string; method: string; notes: string[]; extents: { offset: number; length: number }[] }
export interface Progress { scanned: number; total: number; found: number; bytes_read?: number; current_offset?: number | null }
export interface RecoveryReport { recovered: number; failed: number; bytes: number; destination: string; files: { name: string; status: string; sha256: string; error?: string | null }[] }
export interface Job { id: string; mode: string; sourceName: string; sourceId?: string; stage: string; elapsedSeconds: number; progressDeterminate: boolean; sleepInhibited: boolean; sleepInhibitionError: string | null; status: 'running' | 'completed' | 'cancelled' | 'failed'; progress: Progress; entries: Entry[]; warnings: string[]; error: string | null; report?: RecoveryReport | null }
export interface Preview { hex: string; text: string | null; dataUrl: string | null }
export interface Session { id: string; sourceName: string; date: string; count: number }
export type Lang = 'ru' | 'en';
export type Section = 'home' | 'recover' | 'images' | 'sessions' | 'advanced' | 'settings' | 'about';
