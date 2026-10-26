import { renderToStaticMarkup } from 'react-dom/server';
import { describe, expect, it } from 'vitest';
import { JobProgress } from './JobProgress';
import type { ProgressMonitor } from './JobProgress';
import type { Job, Lang } from './types';
const base: Job = { id: 'test', mode: 'deep', sourceName: 'fixture.img', stage: 'carving', elapsedSeconds: 4, progressDeterminate: true, sleepInhibited: true, sleepInhibitionError: null, status: 'running', progress: { scanned: 200, total: 1000, found: 2 }, entries: [], warnings: [], error: null };
const monitor: ProgressMonitor = { now: 4000, elapsed: 4, lastResponseAt: 4000, pollError: '', progressSamples: [{ timeMs: 0, stage: 'carving', scanned: 0, total: 1000 }, { timeMs: 4000, stage: 'carving', scanned: 200, total: 1000 }], cancelling: false, cancelError: '', launching: false, onRetry: () => {} };
const render = (job: Job, state = monitor, active = true, lang: Lang = 'en') => renderToStaticMarkup(<JobProgress job={job} active={active} monitor={state} lang={lang} onCancel={() => {}} />);
describe('visible operation state', () => {
  it('shows the real stage, elapsed time, measured ETA, and confirmed sleep protection', () => {
    const html = render(base);
    expect(html).toContain('Searching file signatures');
    expect(html).toContain('00:04'); expect(html).toContain('≈ 00:16');
    expect(html).toContain('Idle sleep prevented during the operation');
    expect(html).toContain('20.0%');
  });
  it('estimates after a poll arrives between the one-second display clock ticks', () => {
    const html = render(base, { ...monitor, now: 3500, lastResponseAt: 4000 });
    expect(html).toContain('≈ 00:16');
  });
  it('does not present an uninstrumented recovery source counter as zero bytes read', () => {
    const html = render({ ...base, mode: 'recover', stage: 'recovering', progress: { ...base.progress, bytes_read: 0 } });
    expect(html).toContain('Processed'); expect(html).not.toContain('Source bytes read');
  });
  it('keeps unbounded metadata scanning indeterminate without fabricated percentage or ETA', () => {
    const html = render({ ...base, mode: 'quick', stage: 'metadata', progressDeterminate: false });
    expect(html).toContain('Metadata read'); expect(html).not.toContain('20.0%'); expect(html).not.toContain('≈');
  });
  it('shows actual metadata source reads without a fabricated processed denominator', () => {
    for (const [bytesRead, size] of [[999, '999 B'], [1000, '1 KB'], [2048, '2.05 KB'], [1_048_576, '1.05 MB'], [1_000_000_000, '1 GB'], [1_000_000_000_000, '1 TB']] as const) {
      const html = render({ ...base, mode: 'quick', stage: 'metadata', progressDeterminate: false, progress: { scanned: 0, total: 1000, found: 2, bytes_read: bytesRead } });
      expect(html).toContain('Source bytes read'); expect(html).toContain(size);
      expect(html).toContain('including revisits and cache read-ahead'); expect(html).not.toContain('Metadata read'); expect(html).not.toContain('20.0%');
    }
  });
  it('shows preparation and queued cancellation before a job id exists', () => {
    const html = render({ ...base, id: '', stage: 'preparing', progressDeterminate: false, sleepInhibited: false }, { ...monitor, launching: true, cancelling: true, lastResponseAt: 0 });
    expect(html).toContain('Cancellation is queued'); expect(html).toContain('Checking sleep protection');
  });
  it('does not describe a poll failure as a failed scan or offer a stale ETA', () => {
    const html = render(base, { ...monitor, pollError: 'IPC unavailable' });
    expect(html).toContain('The operation may still be running'); expect(html).toContain('IPC unavailable'); expect(html).not.toContain('≈');
    expect(html).toContain('Operation in progress');
  });
  it('makes unavailable sleep protection explicit instead of claiming it is active', () => {
    const html = render({ ...base, sleepInhibited: false, sleepInhibitionError: 'OS request rejected' });
    expect(html).toContain('Sleep protection unavailable'); expect(html).toContain('OS request rejected');
    expect(html).not.toContain('Idle sleep prevented during the operation');
  });
  it('retains final elapsed duration and scope without showing a running stage indicator', () => {
    const html = render({ ...base, status: 'completed', stage: 'finished', elapsedSeconds: 3661 }, monitor, false);
    expect(html).toContain('1:01:01'); expect(html).toContain('Processing finished'); expect(html).not.toContain('Engine response');
    const quick = { ...base, mode: 'quick', status: 'completed' as const, stage: 'finished', elapsedSeconds: 0.12, progressDeterminate: false };
    const en = render(quick, monitor, false);
    expect(en).toContain('Less than 1 s'); expect(en).not.toContain('00:00');
    expect(en).toContain('Metadata scan finished'); expect(en).toContain('does not scan the entire contents'); expect(en).not.toContain('All operation stages finished');
    const ru = render(quick, monitor, false, 'ru');
    expect(ru).toContain('Менее 1 с'); expect(ru).toContain('Поиск по метаданным завершён'); expect(ru).toContain('Полное содержимое носителя в этом режиме не сканируется');
  });
  it('distinguishes recent IPC delivery from an unresponsive worker and suppresses its ETA', () => {
    const html = render({ ...base, workerHeartbeatAgeSeconds: 45 });
    expect(html).toContain('Status received'); expect(html).toContain('Worker activity'); expect(html).toContain('45 s ago');
    expect(html).toContain('a responsive interface does not imply scan progress'); expect(html).not.toContain('≈');
  });
  it('shows actual sector reads with repeat-read scope and no invented sector size', () => {
    const html = render({ ...base, progress: { ...base.progress, bytes_read: 4096, sector_size: 512 }, sourceSize: 1_000_000_000 });
    expect(html).toContain('Sectors read: 8'); expect(html).toContain('Sector size: 512 B'); expect(html).toContain('Source size: 1 GB');
    expect(render({ ...base, progress: { ...base.progress, bytes_read: 4096 } })).not.toContain('Sectors read');
  });
  it('retains final duration and explains terminal result transfer without offering a meaningless stop', () => {
    const html = render({ ...base, status: 'cancelled', resultsState: 'partial', elapsedSeconds: 0.3, sleepInhibited: false }, { ...monitor, elapsed: 20 });
    expect(html).toContain('Less than 1 s'); expect(html).toContain('final list is still loading');
    expect(html).not.toContain('Sleep protection unavailable'); expect(html).not.toContain('>Stop<');
  });
});
