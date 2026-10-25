import { describe, expect, it } from 'vitest';
import { appendProgressSample, estimateRemainingSeconds, formatDuration, lastProgressTime } from './progress';
import type { ProgressSample } from './progress';
const sample = (timeMs: number, scanned: number, stage = 'carving', total = 1000): ProgressSample => ({ timeMs, scanned, stage, total });
describe('measured stage ETA', () => {
  it('requires three seconds of observed byte movement', () => {
    expect(estimateRemainingSeconds([sample(0, 0), sample(2000, 200)], 2000)).toBeUndefined();
    expect(estimateRemainingSeconds([sample(0, 0), sample(4000, 200)], 4000)).toBe(16);
  });
  it('never invents an ETA from repeated equal counters', () => expect(estimateRemainingSeconds([sample(0, 100), sample(5000, 100)], 5000)).toBeUndefined());
  it('hides ETA when status responses are stale', () => expect(estimateRemainingSeconds([sample(0, 0), sample(4000, 200)], 9000)).toBeUndefined());
  it('hides ETA after ten seconds without movement even while engine responds', () => expect(estimateRemainingSeconds([sample(0, 0), sample(4000, 200), sample(14000, 200)], 14000)).toBeUndefined());
  it('has no remaining estimate for unknown totals or completed stage', () => {
    expect(estimateRemainingSeconds([sample(0, 0, 'metadata', 0), sample(4000, 200, 'metadata', 0)], 4000)).toBeUndefined();
    expect(estimateRemainingSeconds([sample(0, 0), sample(4000, 1000)], 4000)).toBeUndefined();
  });
  it('resets measurements when moving from metadata to carving', () => expect(appendProgressSample([sample(0, 500, 'metadata')], sample(4000, 0))).toEqual([sample(4000, 0)]));
  it('resets measurements when totals change or counters regress', () => {
    expect(appendProgressSample([sample(0, 500)], sample(4000, 600, 'carving', 2000))).toHaveLength(1);
    expect(appendProgressSample([sample(0, 500)], sample(4000, 100))).toHaveLength(1);
  });
  it('tracks indeterminate source reads separately from extent progress', () => {
    const samples = [{ ...sample(0, 0, 'metadata'), bytesRead: 100 }, { ...sample(4000, 0, 'metadata'), bytesRead: 200 }];
    expect(lastProgressTime(samples, 'bytesRead')).toBe(4000);
    expect(lastProgressTime(samples)).toBe(0);
    expect(estimateRemainingSeconds(samples, 4000)).toBeUndefined();
  });
  it('falls back to scanned activity when source-read counters are absent', () => expect(lastProgressTime([sample(0, 0), sample(4000, 200)], 'bytesRead')).toBe(4000));
  it('keeps a bounded rate window', () => expect(appendProgressSample([sample(0, 0), sample(30000, 300)], sample(31000, 310))).toHaveLength(2));
  it('distinguishes counter movement from a fresh backend response', () => expect(lastProgressTime([sample(0, 0), sample(4000, 200), sample(8000, 200)])).toBe(4000));
});
describe('elapsed duration', () => {
  it('does not wrap hours into minutes', () => expect(formatDuration(3661)).toBe('1:01:01'));
  it('handles preparation times without invalid displays', () => { expect(formatDuration(4.9)).toBe('00:04'); expect(formatDuration(NaN)).toBe('00:00'); });
});
