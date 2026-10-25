import type { Entry } from './types';
export function formatBytes(value: number): string {
  if (!Number.isFinite(value) || value < 0) return '—';
  if (value === 0) return '0 B';
  const unit = Math.min(Math.max(0, Math.floor(Math.log10(value) / 3)), 4);
  return `${(value / 1000 ** unit).toLocaleString(undefined, { maximumFractionDigits: unit ? 2 : 0 })} ${['B', 'KB', 'MB', 'GB', 'TB'][unit]}`;
}
export function parseRange(start: string, end: string, size: number): { start: number | null; end: number | null } {
  const parse = (input: string) => {
    if (!input.trim()) return null;
    if (!/^\d+$/.test(input.trim())) throw new Error('range');
    const value = Number(input);
    if (!Number.isSafeInteger(value) || value < 0) throw new Error('range');
    return value;
  };
  const from = parse(start); const to = parse(end);
  if (to !== null && to <= (from ?? 0)) throw new Error('range');
  if (size > 0 && ((from !== null && from >= size) || (to !== null && to > size))) throw new Error('range');
  return { start: from, end: to };
}
export function filterEntries(entries: Entry[], query: string, kind: string, sort: string): Entry[] {
  const q = query.trim().toLocaleLowerCase();
  return entries.filter(e => (!q || e.name.toLocaleLowerCase().includes(q)) && (!kind || e.kind === kind)).sort((a, b) => sort === 'size' ? b.size - a.size : sort === 'offset' ? a.offset - b.offset : a.name.localeCompare(b.name));
}
export function selectedVisible(ids: Set<string>, entries: Entry[]): boolean { return entries.length > 0 && entries.every(e => ids.has(e.id)); }
export function toggleVisible(ids: Set<string>, entries: Entry[]): Set<string> {
  const next = new Set(ids); const remove = selectedVisible(ids, entries);
  entries.forEach(e => remove ? next.delete(e.id) : next.add(e.id)); return next;
}
export function safeImageUrl(url: string | null): string | null {
  return url && /^data:image\/(png|jpeg|gif|webp|bmp);base64,[a-zA-Z0-9+/=\r\n]+$/.test(url) ? url : null;
}
export function progressPercent(scanned: number, total: number): number | undefined {
  return Number.isFinite(scanned) && Number.isFinite(total) && total > 0 ? Math.min(100, Math.max(0, 100 * scanned / total)) : undefined;
}

export function formatSessionDate(value: string, locale: string): string {
  const date = new Date(/^\d+$/.test(value) ? Number(value) * 1000 : value);
  return Number.isNaN(date.getTime()) ? value : date.toLocaleString(locale, { dateStyle: 'medium', timeStyle: 'short' });
}
