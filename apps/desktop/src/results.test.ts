import { describe, expect, it } from 'vitest';
import type { Entry, Job } from './types';
import { appendResultPage, buildResultTree, filterCatalog, findResultNode, mayRecover, toggleIds } from './results';
import { pollJob } from './jobPolling';
import type { JobTransport, JobUpdate } from './jobPolling';

const entry = (id: string, fields: Partial<Entry> = {}): Entry => ({ id, name: `${id}.jpg`, kind: 'jpg', size: 100, offset: 512, confidence: 'low', method: 'signature', notes: [], extents: [{ offset: 512, length: 100 }], ...fields });
const job = (fields: Partial<Job> = {}): Job => ({ id: 'scan', mode: 'deep', sourceName: 'USB', status: 'running', stage: 'metadata', elapsedSeconds: 2, progressDeterminate: false, sleepInhibited: true, sleepInhibitionError: null, progress: { scanned: 512, total: 1000, found: 1 }, entries: [], warnings: [], error: null, canRecover: true, canPreview: true, resultsState: 'scanning', ...fields });
const empty = () => ({ entries: [] as Entry[], cursor: 0 });

describe('incremental scan delivery', () => {
  it('retains immutable entries when repeated pages contain an already published id', () => {
    const first = entry('a');
    const catalog = appendResultPage(empty(), { entries: [first], next: 1, total: 2, hasMore: true });
    const next = appendResultPage(catalog, { entries: [entry('a', { name: 'unexpected change' }), entry('b')], next: 2, total: 2, hasMore: false });
    expect(next.entries).toHaveLength(2); expect(next.entries[0]).toBe(first); expect(next.cursor).toBe(2);
  });
  it('rejects cursor loops instead of locking the UI into an endless page drain', () => {
    expect(() => appendResultPage(empty(), { entries: [], next: 0, total: 20, hasMore: true })).toThrow('cursor');
    expect(() => appendResultPage(empty(), { entries: [], next: 2, total: 1, hasMore: false })).toThrow('cursor');
  });
  it('drains final pages after cancellation, while limiting work in each poll', async () => {
    const entries = Array.from({ length: 2101 }, (_, index) => entry(String(index)));
    const transport: JobTransport = {
      status: async () => job({ status: 'cancelled', resultsState: 'partial', entryCount: entries.length }),
      results: async (_, after) => ({ entries: entries.slice(after, after + 500), next: Math.min(after + 500, entries.length), total: entries.length, hasMore: after + 500 < entries.length }),
    };
    const first = await pollJob('scan', true, empty(), transport, () => {}, () => true);
    expect(first?.job.status).toBe('cancelled'); expect(first?.catalog.entries).toHaveLength(2000); expect(first?.hasMore).toBe(true);
    const second = await pollJob('scan', true, first!.catalog, transport, () => {}, () => true);
    expect(second?.catalog.entries).toHaveLength(2101); expect(second?.hasMore).toBe(false);
  });
  it('preserves pages received before an IPC error and resumes from the retained cursor', async () => {
    let latest: JobUpdate | undefined;
    const transport: JobTransport = {
      status: async () => job({ status: 'completed', resultsState: 'complete', entryCount: 2 }),
      results: async (_, after) => { if (after) throw new Error('interrupted IPC'); return { entries: [entry('a')], next: 1, total: 2, hasMore: true }; },
    };
    await expect(pollJob('scan', true, empty(), transport, update => { latest = update; }, () => true)).rejects.toThrow('interrupted IPC');
    expect(latest?.job.status).toBe('completed'); expect(latest?.catalog.entries.map(item => item.id)).toEqual(['a']);
    const resumed = await pollJob('scan', true, latest!.catalog, { ...transport, results: async (_, after) => { expect(after).toBe(1); return { entries: [entry('b')], next: 2, total: 2, hasMore: false }; } }, () => {}, () => true);
    expect(resumed?.job.entries.map(item => item.id)).toEqual(['a', 'b']);
  });
  it('lets recovery finish while a scan page is still pending without replacing the scan catalog', async () => {
    let releasePage!: (value: Awaited<ReturnType<JobTransport['results']>>) => void;
    const pendingPage = new Promise<Awaited<ReturnType<JobTransport['results']>>>(resolve => { releasePage = resolve; });
    const published = new Map<string, JobUpdate>();
    const transport: JobTransport = {
      status: async id => job({ id, ...(id === 'recover' ? { mode: 'recover', status: 'completed', report: { recovered: 1, failed: 0, bytes: 100, destination: '/other', files: [] } } : {}) }),
      results: async id => { expect(id).toBe('scan'); return pendingPage; },
    };
    const scanPromise = pollJob('scan', true, empty(), transport, update => published.set('scan', update), () => true);
    const recovered = await pollJob('recover', false, empty(), transport, update => published.set('recover', update), () => true);
    expect(recovered?.job.report?.recovered).toBe(1); expect(published.get('scan')?.job.status).toBe('running');
    releasePage({ entries: [entry('a')], next: 1, total: 1, hasMore: false });
    await scanPromise;
    expect(published.get('scan')?.job.entries[0].id).toBe('a'); expect(published.get('recover')?.job.report?.recovered).toBe(1);
  });
  it('discards an old in-flight page when its operation is no longer current', async () => {
    let current = true; const updates: JobUpdate[] = [];
    const result = await pollJob('scan', true, empty(), { status: async () => job(), results: async () => { current = false; return { entries: [entry('old')], next: 1, total: 1, hasMore: false }; } }, update => updates.push(update), () => current);
    expect(result).toBeNull(); expect(updates).toHaveLength(1); expect(updates[0].job.entries).toEqual([]);
  });
});

describe('live result groups and recovery permissions', () => {
  const entries = [
    entry('a', { relative_path: 'DCIM/100CAM/a.jpg', partition: 'Camera', deletion_status: 'existing' }),
    entry('b', { relative_path: 'DCIM/100CAM/b.jpg', partition: 'Camera', deletion_status: 'deleted' }),
    entry('c', { relative_path: 'MEDIA/c.mov', partition: 'Camera', kind: 'mov', deletion_status: 'existing' }),
    entry('d', { partition: 'Camera' }),
    entry('e', { relative_path: 'Unknown location/e.jpg', partition: 'Camera' }),
  ];
  const labels = { unknown: 'Unknown location', source: 'Source' };
  it('groups directories by partition without treating basenames as folders or mixing carved locations', () => {
    const tree = buildResultTree(entries, 'location', labels);
    expect(tree).toHaveLength(1); expect(tree[0].entryIds).toHaveLength(5);
    const dcim = tree[0].children.find(node => node.label === 'DCIM')!;
    expect(dcim.entryIds).toEqual(['a', 'b']); expect(dcim.children[0].label).toBe('100CAM'); expect(dcim.children[0].children).toEqual([]);
    const unknown = tree[0].children.filter(node => node.label === 'Unknown location');
    expect(unknown).toHaveLength(2); expect(unknown[0].id).not.toBe(unknown[1].id);
    expect(findResultNode(tree, dcim.children[0].id)?.entryIds).toEqual(['a', 'b']);
    expect(buildResultTree(entries, 'type', labels).find(node => node.label === 'MOV')?.entryIds).toEqual(['c']);
  });
  it('selects folder descendants without removing a selected file hidden by the current filters', () => {
    const selected = toggleIds(new Set(['c']), ['a', 'b']);
    expect([...selected]).toEqual(['c', 'a', 'b']);
    expect([...toggleIds(selected, ['a', 'b'])]).toEqual(['c']);
    const filtered = filterCatalog(entries, { query: '.jpg', path: 'DCIM', kind: 'jpg', status: 'deleted', sort: 'name', ids: new Set(['a', 'b', 'c']) });
    expect(filtered.map(item => item.id)).toEqual(['b']);
    expect(filterCatalog(entries, { query: '', path: '', kind: '', status: 'unknown', sort: 'name' }).map(item => item.id)).toEqual(['d', 'e']);
  });
  it('allows recoverable running or cancelled scan results while blocking concurrent recoveries and changed sources', () => {
    expect(mayRecover(job(), false, 1)).toBe(true);
    expect(mayRecover(job({ status: 'cancelled', resultsState: 'partial' }), false, 1)).toBe(true);
    expect(mayRecover(job(), true, 1)).toBe(false);
    expect(mayRecover(job({ resultsState: 'invalid-source' }), false, 1)).toBe(false);
    expect(mayRecover(job({ status: 'failed', canRecover: false }), false, 1)).toBe(false);
    expect(mayRecover(job(), false, 0)).toBe(false);
  });
});
