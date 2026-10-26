import type { Entry, Job, ResultPage } from './types';

export interface Catalog { entries: Entry[]; cursor: number }
/** A page cursor follows server positions, even when repeated IDs are ignored. */
export function appendResultPage(catalog: Catalog, page: ResultPage): Catalog {
  if (!Number.isSafeInteger(page.next) || !Number.isSafeInteger(page.total) || page.next > page.total || page.next < catalog.cursor || (page.hasMore && page.next === catalog.cursor)) throw new Error('Invalid result-page cursor');
  if (!page.entries.length) return { entries: catalog.entries, cursor: page.next };
  const ids = new Set(catalog.entries.map(entry => entry.id));
  const added = page.entries.filter(entry => { if (ids.has(entry.id)) return false; ids.add(entry.id); return true; });
  return { entries: added.length ? [...catalog.entries, ...added] : catalog.entries, cursor: page.next };
}
export function attachCatalog(status: Job, catalog: Catalog): Job { return { ...status, entries: catalog.entries }; }
export function mayRecover(scan: Job | null, operationBusy: boolean, selectedCount: number): boolean {
  return !!scan && selectedCount > 0 && !operationBusy && scan.resultsState !== 'invalid-source' && (scan.canRecover ?? scan.status === 'completed');
}
export function toggleIds(selected: Set<string>, ids: readonly string[]): Set<string> {
  const next = new Set(selected); const remove = ids.length > 0 && ids.every(id => next.has(id));
  for (const id of ids) if (remove) next.delete(id); else next.add(id);
  return next;
}
export interface ResultNode { id: string; label: string; entryIds: string[]; children: ResultNode[] }
export type TreeView = 'location' | 'type';
export function buildResultTree(entries: Entry[], view: TreeView, labels: { unknown: string; source: string }): ResultNode[] {
  const nodes = new Map<string, ResultNode>(); const roots: ResultNode[] = [];
  for (const entry of entries) {
    const parts = view === 'type' ? [entry.kind.toUpperCase() || labels.unknown] : entry.relative_path
      ? [entry.partition || labels.source, ...entry.relative_path.replaceAll('\\', '/').split('/').filter(Boolean).slice(0, -1)]
      : [entry.partition || labels.source, labels.unknown];
    // Keep unknown paths separate from genuine directories with the same label.
    const namespace = view === 'location' && !entry.relative_path ? 'unknown' : 'known';
    let parent: ResultNode | undefined;
    parts.forEach((label, index) => {
      const id = JSON.stringify([view, index === 0 ? 'root' : namespace, ...parts.slice(0, index + 1)]);
      let node = nodes.get(id);
      if (!node) { node = { id, label, entryIds: [], children: [] }; nodes.set(id, node); if (parent) parent.children.push(node); else roots.push(node); }
      node.entryIds.push(entry.id); parent = node;
    });
  }
  const sort = (list: ResultNode[]) => { list.sort((a, b) => a.label.localeCompare(b.label)); list.forEach(node => sort(node.children)); };
  sort(roots); return roots;
}
export function findResultNode(nodes: ResultNode[], id: string): ResultNode | undefined {
  for (const node of nodes) { if (node.id === id) return node; const child = findResultNode(node.children, id); if (child) return child; }
}
export function filterCatalog(entries: Entry[], filters: { query: string; path: string; kind: string; status: string; sort: string; ids?: Set<string> }): Entry[] {
  const query = filters.query.trim().toLocaleLowerCase(); const path = filters.path.trim().toLocaleLowerCase();
  return entries.filter(entry => (!filters.ids || filters.ids.has(entry.id))
    && (!query || entry.name.toLocaleLowerCase().includes(query))
    && (!path || (entry.relative_path || '').toLocaleLowerCase().includes(path))
    && (!filters.kind || entry.kind === filters.kind)
    && (!filters.status || (entry.deletion_status || 'unknown') === filters.status))
    .sort((a, b) => filters.sort === 'size' ? b.size - a.size : filters.sort === 'offset' ? a.offset - b.offset : a.name.localeCompare(b.name));
}
