import { describe, it, expect } from 'vitest';
import { queryLibrary, type Query } from '../src/lib/query';
import { defaultFilters, filterScores } from '../src/lib/model';
import { demoScores } from '../src/lib/demo';

const query: Query = {
  requestId: 7,
  filters: { ...defaultFilters, mode: '' },
  page: 0,
  size: 3,
  metric: 'stars',
  bucket: 'year',
  unique: false,
  progression: false,
};
describe('background search queries', () => {
  it('returns only the requested page with correct whole-result summaries', () => {
    const scores = demoScores();
    const expected = filterScores(scores, query.filters);
    const result = queryLibrary(scores, { ...query, page: 1 });
    expect(result.ids).toEqual(expected.slice(3, 6).map((s) => s.id));
    expect(result.requestId).toBe(7);
    expect(result.total).toBe(expected.length);
    expect(result.maps).toBe(new Set(expected.map((s) => s.mapHash)).size);
    expect(result.fcs).toBe(expected.filter((s) => s.fc).length);
    expect(result.points).toEqual([]);
  });
  it('applies player isolation to statistics and progression as well as the page', () => {
    const base = demoScores()[0];
    const scores = [
      { ...base, id: 'mine', player: 'Mine', stars: 4 },
      { ...base, id: 'other', player: 'Other', stars: 10 },
    ];
    const result = queryLibrary(scores, {
      ...query,
      filters: { ...defaultFilters, player: 'Mine', mode: String(base.mode) },
      progression: true,
    });
    expect(result.ids).toEqual(['mine']);
    expect(result.total).toBe(1);
    expect(result.points.filter((p) => p.count).map((p) => p.value)).toEqual([4]);
  });
  it('handles rapid successive searches without mutating the library', () => {
    const scores = demoScores();
    const before = scores.map((s) => s.id);
    const earlier = queryLibrary(scores, {
      ...query,
      requestId: 1,
      filters: { ...query.filters, search: 'no such map' },
    });
    const latest = queryLibrary(scores, { ...query, requestId: 2 });
    expect(earlier.total).toBe(0);
    expect(latest.total).toBe(scores.length);
    expect(latest.requestId).toBe(2);
    expect(scores.map((s) => s.id)).toEqual(before);
  });
});
