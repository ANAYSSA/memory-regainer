import type { Source } from './types';

export interface SourceSnapshot { sources: Source[]; loading: boolean; initialized: boolean; error: string }

/** One owner for initial, manual, focus and hotplug reads, including effect restarts. */
export class SourceDiscovery {
  private snapshot: SourceSnapshot = { sources: [], loading: false, initialized: false, error: '' };
  private listeners = new Set<() => void>();
  private inFlight: Promise<void> | null = null;
  constructor(private readonly read: () => Promise<Source[]>) {}
  getSnapshot = () => this.snapshot;
  subscribe = (listener: () => void) => { this.listeners.add(listener); return () => { this.listeners.delete(listener); }; };
  private publish(next: SourceSnapshot) { this.snapshot = next; this.listeners.forEach(listener => listener()); }
  addSource = (source: Source) => {
    this.publish({ ...this.snapshot, sources: [...this.snapshot.sources.filter(item => item.id !== source.id), source] });
  };
  refresh = (): Promise<void> => {
    if (this.inFlight) return this.inFlight;
    this.publish({ ...this.snapshot, loading: true });
    const request = Promise.resolve().then(this.read).then(sources => {
      // A file picker may register an image while device enumeration is in flight.
      const retainedImages = this.snapshot.sources.filter(source => source.kind === 'image' && !sources.some(item => item.id === source.id));
      this.publish({ sources: [...sources, ...retainedImages], loading: false, initialized: true, error: '' });
    }, error => {
      this.publish({ ...this.snapshot, loading: false, initialized: true, error: String(error) });
      throw error;
    }).finally(() => { if (this.inFlight === request) this.inFlight = null; });
    this.inFlight = request;
    return request;
  };
}

export function hasNativeBridge(host: object): boolean {
  const bridge = (host as { __TAURI_INTERNALS__?: { invoke?: unknown } }).__TAURI_INTERNALS__;
  return typeof bridge?.invoke === 'function';
}

/** Readiness is checked again after startup; an idle transition creates a fresh watcher. */
export function watchSourceDiscovery(options: { ready: () => boolean; busy: () => boolean; refresh: () => Promise<void> }) {
  let stopped = false;
  let nextRefresh = 0;
  const tick = (force = false) => {
    if (stopped || !options.ready() || options.busy()) return;
    const now = Date.now();
    if (!force && now < nextRefresh) return;
    nextRefresh = now + 5000;
    void options.refresh().catch(() => { /* The controller exposes failures and the timer retries. */ });
  };
  tick(true);
  // Readiness can arrive after module evaluation. Actual native enumeration stays at 5s.
  const timer = setInterval(() => tick(), 250);
  return { focus: () => tick(true), stop: () => { stopped = true; clearInterval(timer); } };
}

/** Tauri listener registration is asynchronous; cleanup may precede its resolution. */
export function listenForNativeFocus(register: (callback: (focused: boolean) => void) => Promise<() => void>, onFocus: () => void) {
  let stopped = false; let unlisten: (() => void) | undefined;
  void Promise.resolve().then(() => register(focused => { if (!stopped && focused) onFocus(); })).then(remove => {
    if (stopped) remove(); else unlisten = remove;
  }).catch(() => { /* DOM focus, visibility and polling remain available. */ });
  return () => { stopped = true; unlisten?.(); };
}
