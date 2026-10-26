import { useEffect, useMemo, useRef, useState, useSyncExternalStore } from 'react';
import { invoke } from '@tauri-apps/api/core';
import { getCurrentWindow } from '@tauri-apps/api/window';
import type { Source } from './types';
import { hasNativeBridge, listenForNativeFocus, SourceDiscovery, watchSourceDiscovery } from './sourceDiscovery';

export function useSources(busy: boolean) {
  const [native, setNative] = useState(() => hasNativeBridge(window));
  const discovery = useMemo(() => new SourceDiscovery(() => invoke<Source[]>('list_sources')), []);
  const snapshot = useSyncExternalStore(discovery.subscribe, discovery.getSnapshot, discovery.getSnapshot);
  const watcher = useRef<ReturnType<typeof watchSourceDiscovery> | null>(null);
  useEffect(() => {
    const current = watchSourceDiscovery({ ready: () => { const ready = hasNativeBridge(window); if (ready) setNative(true); return ready; }, busy: () => busy, refresh: discovery.refresh });
    watcher.current = current;
    const focus = () => current.focus();
    const visible = () => { if (document.visibilityState === 'visible') current.focus(); };
    window.addEventListener('focus', focus);
    window.addEventListener('pageshow', focus);
    document.addEventListener('visibilitychange', visible);
    return () => {
      current.stop(); if (watcher.current === current) watcher.current = null;
      window.removeEventListener('focus', focus);
      window.removeEventListener('pageshow', focus);
      document.removeEventListener('visibilitychange', visible);
    };
  }, [busy, discovery]);
  useEffect(() => {
    if (!native) return;
    return listenForNativeFocus(callback => getCurrentWindow().onFocusChanged(event => callback(event.payload)), () => watcher.current?.focus());
  }, [native]);
  return { ...snapshot, native, refreshSources: discovery.refresh, addSource: discovery.addSource };
}
