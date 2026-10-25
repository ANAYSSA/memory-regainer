import { describe, expect, it } from 'vitest';
import { parseRange, safeImageUrl, toggleVisible, filterEntries, progressPercent } from './model';
import type { Entry } from './types';
const file = (id: string, name: string, kind: string, size: number): Entry => ({ id, name, kind, size, offset: 0, confidence: 'unknown', method: 'carving', notes: [], extents: [] });
describe('advanced ranges', () => {
  it('keeps an unspecified boundary unbounded', () => expect(parseRange('', '', 100)).toEqual({ start: null, end: null }));
  it('allows exact exclusive source end', () => expect(parseRange('20', '100', 100)).toEqual({ start: 20, end: 100 }));
  it.each([['-1', '50'], ['10.5', '50'], ['10', '10'], ['40', '30'], ['0', '101'], ['9007199254740992', '']])('rejects unsafe range %s to %s', (start, end) => expect(() => parseRange(start, end, 100)).toThrow());
});
describe('recovery selection', () => {
  it('preserves hidden selections when selecting filtered results', () => expect([...toggleVisible(new Set(['other']), [file('one', 'a.jpg', 'jpg', 1)])]).toEqual(['other', 'one']));
  it('removes only the visible selection', () => expect([...toggleVisible(new Set(['other', 'one']), [file('one', 'a.jpg', 'jpg', 1)])]).toEqual(['other']));
  it('searches Unicode filenames, filters type, and sorts without mutating input', () => {
    const entries = [file('1', 'Фото.JPG', 'jpg', 10), file('2', 'фото2.jpg', 'jpg', 20), file('3', 'фото.pdf', 'pdf', 30), file('4', 'Видео.MP4', 'mp4', 5_000_000_000), file('5', 'видео.mov', 'mov', 8_000_000_000)];
    expect(filterEntries(entries, 'ФОТО', 'jpg', 'size').map(e => e.id)).toEqual(['2', '1']);
    expect(filterEntries(entries, 'ВИДЕО', 'mp4', 'size').map(e => e.id)).toEqual(['4']);
    expect(filterEntries(entries, 'ВИДЕО', '', 'size').map(e => e.id)).toEqual(['5', '4']);
    expect(entries[0].id).toBe('1');
  });
});
describe('untrusted previews and progress', () => {
  it.each(['data:image/svg+xml;base64,PHN2Zz4=', 'https://tracking.example/x.png', 'javascript:alert(1)', 'data:text/html;base64,AAAA', 'data:video/mp4;base64,AAAA', 'data:video/quicktime;base64,AAAA'])('refuses active, video, or remote image preview %s', url => expect(safeImageUrl(url)).toBeNull());
  it('allows a local raster data image', () => expect(safeImageUrl('data:image/png;base64,AAAA')).not.toBeNull());
  it('does not invent progress when source size is unknown', () => expect(progressPercent(100, 0)).toBeUndefined());
  it('bounds known progress', () => expect(progressPercent(120, 100)).toBe(100));
});
