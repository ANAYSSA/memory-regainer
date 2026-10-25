export interface ProgressSample {
  timeMs: number;
  stage: string;
  scanned: number;
  total: number;
  bytesRead?: number;
}

/** A new stage, source range, or regressing counter starts a new rate window. */
export function appendProgressSample(previous: ProgressSample[], next: ProgressSample): ProgressSample[] {
  const last = previous.at(-1);
  const reset = !last || last.stage !== next.stage || last.total !== next.total || next.scanned < last.scanned || next.timeMs < last.timeMs;
  return [...(reset ? [] : previous.filter(sample => next.timeMs - sample.timeMs <= 30_000)), next].slice(-80);
}

export function lastProgressTime(samples: ProgressSample[], counter: 'scanned' | 'bytesRead' = 'scanned'): number | undefined {
  const value = (sample: ProgressSample) => counter === 'bytesRead' ? sample.bytesRead ?? sample.scanned : sample.scanned;
  let changed = samples[0]?.timeMs;
  for (let i = 1; i < samples.length; i++) if (value(samples[i]) !== value(samples[i - 1])) changed = samples[i].timeMs;
  return changed;
}

/** Estimate only from actual same-stage byte movement, never from elapsed time alone. */
export function estimateRemainingSeconds(samples: ProgressSample[], nowMs: number): number | undefined {
  const first = samples[0]; const last = samples.at(-1);
  if (!first || !last || !Number.isFinite(nowMs) || nowMs < last.timeMs || nowMs - last.timeMs >= 5_000) return undefined;
  if (last.stage !== first.stage || last.total !== first.total || last.total <= 0 || last.scanned >= last.total) return undefined;
  if (![first.timeMs, last.timeMs, first.scanned, last.scanned, last.total].every(Number.isFinite)) return undefined;
  const duration = last.timeMs - first.timeMs;
  const moved = last.scanned - first.scanned;
  const changedAt = lastProgressTime(samples);
  if (duration < 3_000 || moved <= 0 || changedAt === undefined || nowMs - changedAt >= 10_000) return undefined;
  const seconds = (last.total - last.scanned) / (moved / (duration / 1000));
  return Number.isFinite(seconds) && seconds >= 0 ? Math.ceil(seconds) : undefined;
}

export function formatDuration(seconds: number): string {
  const safe = Math.max(0, Math.floor(Number.isFinite(seconds) ? seconds : 0));
  const hours = Math.floor(safe / 3600);
  const minutes = Math.floor(safe / 60) % 60;
  const rest = safe % 60;
  return hours ? `${hours}:${String(minutes).padStart(2, '0')}:${String(rest).padStart(2, '0')}` : `${String(minutes).padStart(2, '0')}:${String(rest).padStart(2, '0')}`;
}
