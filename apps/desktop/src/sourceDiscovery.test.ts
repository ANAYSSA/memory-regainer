import { afterEach, describe, expect, it, vi } from 'vitest';
import type { Source } from './types';
import { hasNativeBridge, listenForNativeFocus, SourceDiscovery, watchSourceDiscovery } from './sourceDiscovery';

const drive = (id: string, kind = 'device'): Source => ({ id, name: id, kind, path: id, size: 1000, accessible: true, detail: '' });
const deferred = <T,>() => { let resolve!: (value: T) => void; let reject!: (reason: unknown) => void; const promise = new Promise<T>((yes, no) => { resolve = yes; reject = no; }); return { promise, resolve, reject }; };
afterEach(() => { vi.useRealTimers(); });

describe('automatic source discovery lifecycle', () => {
  it('starts enumeration without Refresh and shows pending rather than a completed empty list during a slow OS read', async () => {
    vi.useFakeTimers();
    const result = deferred<Source[]>(); const read = vi.fn(() => result.promise); const store = new SourceDiscovery(read);
    const watcher = watchSourceDiscovery({ ready: () => true, busy: () => false, refresh: store.refresh });
    expect(store.getSnapshot()).toMatchObject({ sources: [], loading: true, initialized: false });
    await vi.advanceTimersByTimeAsync(20_000);
    watcher.focus(); watcher.focus();
    expect(read).toHaveBeenCalledTimes(1);
    result.resolve([drive('E:')]); await vi.advanceTimersByTimeAsync(0);
    expect(store.getSnapshot()).toMatchObject({ sources: [drive('E:')], loading: false, initialized: true, error: '' });
    watcher.stop();
  });
  it('detects a bridge installed after startup and then enumerates automatically', async () => {
    vi.useFakeTimers();
    const host: { __TAURI_INTERNALS__?: { invoke?: unknown } } = {};
    const read = vi.fn(async () => [drive('USB')]); const store = new SourceDiscovery(read);
    const watcher = watchSourceDiscovery({ ready: () => hasNativeBridge(host), busy: () => false, refresh: store.refresh });
    await vi.advanceTimersByTimeAsync(1000); expect(read).not.toHaveBeenCalled();
    host.__TAURI_INTERNALS__ = {}; await vi.advanceTimersByTimeAsync(250); expect(read).not.toHaveBeenCalled();
    host.__TAURI_INTERNALS__.invoke = () => {};
    await vi.advanceTimersByTimeAsync(250);
    expect(read).toHaveBeenCalledTimes(1); expect(store.getSnapshot().sources[0].id).toBe('USB');
    watcher.stop();
  });
  it('retries an initially empty list and detects attachment without a focus event or manual action', async () => {
    vi.useFakeTimers();
    const read = vi.fn<() => Promise<Source[]>>().mockResolvedValueOnce([]).mockResolvedValue([drive('E:')]);
    const store = new SourceDiscovery(read); const watcher = watchSourceDiscovery({ ready: () => true, busy: () => false, refresh: store.refresh });
    await vi.advanceTimersByTimeAsync(0); expect(store.getSnapshot().sources).toEqual([]);
    await vi.advanceTimersByTimeAsync(5000);
    expect(read).toHaveBeenCalledTimes(2); expect(store.getSnapshot().sources[0].id).toBe('E:');
    watcher.stop();
  });
  it('clears its enumeration error after an automatic retry while keeping previously discovered files', async () => {
    vi.useFakeTimers();
    const read = vi.fn<() => Promise<Source[]>>().mockRejectedValueOnce(new Error('OS enumeration unavailable')).mockResolvedValue([drive('E:')]);
    const store = new SourceDiscovery(read); store.addSource(drive('opened.img', 'image'));
    const watcher = watchSourceDiscovery({ ready: () => true, busy: () => false, refresh: store.refresh });
    await vi.advanceTimersByTimeAsync(0);
    expect(store.getSnapshot().error).toContain('OS enumeration unavailable'); expect(store.getSnapshot().sources[0].id).toBe('opened.img');
    await vi.advanceTimersByTimeAsync(5000);
    expect(store.getSnapshot().error).toBe(''); expect(store.getSnapshot().sources.map(source => source.id)).toEqual(['E:', 'opened.img']);
    watcher.stop();
  });
  it('coalesces StrictMode effect restarts, native/DOM focus, and manual refresh into one in-flight request', async () => {
    vi.useFakeTimers();
    const result = deferred<Source[]>(); const read = vi.fn(() => result.promise); const store = new SourceDiscovery(read);
    const options = { ready: () => true, busy: () => false, refresh: store.refresh };
    const first = watchSourceDiscovery(options); first.stop(); const second = watchSourceDiscovery(options);
    second.focus(); first.focus(); const manual = store.refresh();
    await vi.advanceTimersByTimeAsync(0); expect(read).toHaveBeenCalledTimes(1);
    store.addSource(drive('picked-during-read.img', 'image'));
    result.resolve([drive('E:')]); await manual;
    expect(store.getSnapshot().sources.map(source => source.id)).toEqual(['E:', 'picked-during-read.img']);
    second.stop(); await vi.advanceTimersByTimeAsync(10_000); expect(read).toHaveBeenCalledTimes(1);
  });
  it('does not enumerate during a scan or recovery, and refreshes immediately when idle resumes', async () => {
    vi.useFakeTimers();
    const read = vi.fn(async () => [drive('USB')]); const store = new SourceDiscovery(read);
    const active = watchSourceDiscovery({ ready: () => true, busy: () => true, refresh: store.refresh });
    active.focus(); await vi.advanceTimersByTimeAsync(15_000); expect(read).not.toHaveBeenCalled(); active.stop();
    const idle = watchSourceDiscovery({ ready: () => true, busy: () => false, refresh: store.refresh });
    await vi.advanceTimersByTimeAsync(0); expect(read).toHaveBeenCalledTimes(1); idle.stop();
  });
  it('honors native full-window focus and removes a listener whose async registration outlives effect cleanup', async () => {
    vi.useFakeTimers();
    const registration = deferred<() => void>(); const remove = vi.fn(); const focus = vi.fn(); let callback!: (focused: boolean) => void;
    const stop = listenForNativeFocus(async listener => { callback = listener; return registration.promise; }, focus);
    await vi.advanceTimersByTimeAsync(0); callback(false); callback(true); expect(focus).toHaveBeenCalledTimes(1);
    stop(); callback(true); registration.resolve(remove);
    await vi.advanceTimersByTimeAsync(0);
    expect(remove).toHaveBeenCalledTimes(1); expect(focus).toHaveBeenCalledTimes(1);
  });
});
