import { useCallback, useEffect, useRef, useState } from 'react';
import { invoke } from '@tauri-apps/api/core';
import type { Job, ResultPage } from './types';
import { appendProgressSample } from './progress';
import type { ProgressSample } from './progress';
import type { Catalog } from './results';
import { pollJob } from './jobPolling';

const initialJob = (mode: string, sourceName: string): Job => ({ id: '', mode, sourceName, stage: 'preparing', elapsedSeconds: 0, progressDeterminate: false, sleepInhibited: false, sleepInhibitionError: null, status: 'running', progress: { scanned: 0, total: 0, found: 0 }, entries: [], warnings: [], error: null });

/** Each instance owns its IPC loop, cursor, cancellation, timing and errors. */
export function useJob(scan: boolean) {
  const [job, setJob] = useState<Job | null>(null);
  const [activeId, setActiveId] = useState('');
  const [launching, setLaunching] = useState(false);
  const [cancelling, setCancelling] = useState(false);
  const [cancelError, setCancelError] = useState('');
  const [pollError, setPollError] = useState('');
  const [retry, setRetry] = useState(0);
  const [lastResponseAt, setLastResponseAt] = useState(0);
  const [startedAt, setStartedAt] = useState(0);
  const [now, setNow] = useState(Date.now());
  const [progressSamples, setSamples] = useState<ProgressSample[]>([]);
  const catalog = useRef<Catalog>({ entries: [], cursor: 0 });
  const cancelQueued = useRef(false);
  const generation = useRef(0);
  const busy = launching || !!activeId;

  useEffect(() => {
    if (!busy) return;
    setNow(Date.now());
    const timer = setInterval(() => setNow(Date.now()), 1000);
    return () => clearInterval(timer);
  }, [busy]);

  useEffect(() => {
    if (!activeId) return;
    let stopped = false; let timer: ReturnType<typeof setTimeout>;
    const poll = async () => {
      try {
        const update = await pollJob(activeId, scan, catalog.current, {
          status: async id => {
            const status = await invoke<Job>('job_status', { jobId: id });
            if (!stopped) {
              const received = Date.now(); setLastResponseAt(received);
              setSamples(previous => appendProgressSample(previous, { timeMs: received, stage: status.stage, scanned: status.progress.scanned, total: status.progress.total, bytesRead: status.progress.bytes_read }));
            }
            return status;
          },
          results: (id, after) => invoke<ResultPage>('job_results', { jobId: id, after, limit: 500 }),
        }, update => { catalog.current = update.catalog; setJob(update.job); }, () => !stopped);
        if (!update) return;
        setPollError('');
        if (update.job.status !== 'running' && !update.hasMore) { setActiveId(''); setCancelling(false); return; }
        timer = setTimeout(poll, update.hasMore ? 50 : 500);
      } catch (error) {
        if (!stopped) { setPollError(String(error)); timer = setTimeout(poll, 2000); }
      }
    };
    void poll();
    return () => { stopped = true; clearTimeout(timer); };
  }, [activeId, retry, scan]);

  const reset = useCallback(() => {
    generation.current++; catalog.current = { entries: [], cursor: 0 };
    setJob(null); setActiveId(''); setLaunching(false); setCancelling(false); setCancelError(''); setPollError('');
  }, []);
  async function launch(mode: string, sourceName: string, request: () => Promise<string>) {
    const token = ++generation.current; const start = Date.now();
    catalog.current = { entries: [], cursor: 0 }; cancelQueued.current = false;
    setStartedAt(start); setNow(start); setLaunching(true); setCancelling(false); setCancelError(''); setPollError(''); setLastResponseAt(0); setSamples([]);
    const preparing = initialJob(mode, sourceName); setJob(preparing);
    try {
      const id = await request();
      if (generation.current !== token) return id;
      setJob({ ...preparing, id }); setActiveId(id); setLaunching(false);
      if (cancelQueued.current) {
        try { await invoke('cancel_job', { jobId: id }); }
        catch (error) { setCancelError(String(error)); setCancelling(false); }
      }
      return id;
    } catch (error) {
      if (generation.current === token) { setJob({ ...preparing, status: 'failed', error: String(error), elapsedSeconds: (Date.now() - start) / 1000 }); setLaunching(false); setCancelling(false); }
      throw error;
    }
  }
  async function cancel() {
    setCancelling(true); setCancelError('');
    if (launching) { cancelQueued.current = true; return; }
    if (!activeId) { setCancelling(false); return; }
    try { await invoke('cancel_job', { jobId: activeId }); }
    catch (error) { setCancelError(String(error)); setCancelling(false); }
  }
  return { job, busy, launching, launch, cancel, reset, monitor: { now, elapsed: Math.max(0, (now - startedAt) / 1000), lastResponseAt, pollError, progressSamples, cancelling, cancelError, launching, onRetry: () => setRetry(value => value + 1) } };
}
