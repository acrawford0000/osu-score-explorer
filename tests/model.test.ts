import { describe, it, expect } from 'vitest';
import {
  defaultFilters,
  filterScores,
  trends,
  strongIdentity,
  modLabel,
  hitBreakdown,
  clampPage,
} from '../src/lib/model';
import { demoScores } from '../src/lib/demo';
const base = demoScores()[0];
describe('score exploration', () => {
  it('uses maximum misses as a 1 through N choke range', () => {
    const rows = [0, 1, 2, 3].map((misses) => ({
      ...base,
      id: String(misses),
      misses,
      fc: misses === 0,
    }));
    expect(
      filterScores(rows, { ...defaultFilters, maxMisses: 2 })
        .map((s) => s.misses)
        .sort(),
    ).toEqual([1, 2]);
    expect(filterScores(rows, { ...defaultFilters, maxMisses: 0 })).toHaveLength(0);
    expect(filterScores(rows, { ...defaultFilters, maxMisses: null })).toHaveLength(4);
  });
  it('sorts by potential FC PP independently of achieved PP or PP gain, with missing values last', () => {
    const rows = [
      { ...base, id: 'a', pp: 400, fcPp: 500 },
      { ...base, id: 'b', pp: 500, fcPp: 650 },
      { ...base, id: 'c', pp: 100, fcPp: 600 },
      { ...base, id: 'missing', pp: 999, fcPp: null },
    ];
    expect(filterScores(rows, { ...defaultFilters, sort: 'fcPp' }).map((s) => s.id)).toEqual([
      'b',
      'c',
      'a',
      'missing',
    ]);
  });
  it('clamps first, last and ten-page jumps to valid page boundaries', () => {
    expect(clampPage(10, 30720, 36)).toBe(10);
    expect(clampPage(-9, 30720, 36)).toBe(0);
    expect(clampPage(900, 30720, 36)).toBe(853);
    expect(clampPage(5, 0, 36)).toBe(0);
    expect(clampPage(10, 37, 36)).toBe(1);
  });
  it('shows the exact SS choke judgments with mode-specific labels', () => {
    const s = { ...base, n300: 400, n100: 3, n50: 2, misses: 1, nGeki: 500, nKatu: 4 };
    expect(hitBreakdown({ ...s, mode: 0 })).toEqual({
      text: '400 / 3 / 2 · 1× miss',
      label: '300: 400, 100: 3, 50: 2; misses: 1',
    });
    expect(hitBreakdown({ ...s, mode: 1 }).text).toBe('400 / 3 · 1× miss');
    expect(hitBreakdown({ ...s, mode: 2 }).label).toBe(
      'fruits: 400, droplets: 3, tiny droplets: 2, tiny droplet misses: 4; misses: 1',
    );
    expect(hitBreakdown({ ...s, mode: 3 }).text).toBe('500 / 400 / 4 / 3 / 2 · 1× miss');
    expect(hitBreakdown({ ...s, mode: 0, n100: 0, n50: 0, misses: 0 }).text).toBe(
      '400 / 0 / 0 · 0× miss',
    );
  });
  it('defaults to both sources and counts a merged play once under either source', () => {
    const rows = [
      { ...base, id: 'stable', source: 'stable' as const },
      { ...base, id: 'lazer', source: 'lazer' as const },
      {
        ...base,
        id: 'merged',
        source: 'stable' as const,
        sources: ['stable', 'lazer'] as ('stable' | 'lazer')[],
      },
    ];
    expect(filterScores(rows, defaultFilters)).toHaveLength(3);
    expect(
      filterScores(rows, { ...defaultFilters, includeStable: false })
        .map((s) => s.id)
        .sort(),
    ).toEqual(['lazer', 'merged']);
    expect(
      filterScores(rows, { ...defaultFilters, includeLazer: false })
        .map((s) => s.id)
        .sort(),
    ).toEqual(['merged', 'stable']);
    expect(
      filterScores(rows, { ...defaultFilters, includeStable: false, includeLazer: false }),
    ).toHaveLength(0);
  });
  it('allows SS farmers to choose the total number of non-300 judgments', () => {
    const rows = [
      { ...base, id: 'miss', misses: 1, n100: 0, n50: 0, ss: false },
      { ...base, id: '100', misses: 0, n100: 1, n50: 0, fc: true, ss: false },
      { ...base, id: '50s', misses: 0, n100: 0, n50: 2, fc: true, ss: false },
      { ...base, id: '100s', misses: 0, n100: 3, n50: 0, ss: false },
      { ...base, id: 'ss', misses: 0, n100: 0, n50: 0, ss: true },
    ];
    expect(filterScores(rows, { ...defaultFilters, preset: 'ssChoke', maxNon300: 3 })).toHaveLength(
      4,
    );
    expect(filterScores(rows, { ...defaultFilters, preset: 'ssChoke', maxNon300: 2 })).toHaveLength(
      3,
    );
    expect(filterScores(rows, { ...defaultFilters, maxNon300: 0 })).toHaveLength(0);
    expect(
      filterScores(rows, { ...defaultFilters, maxNon300: 2 })
        .map((s) => s.id)
        .sort(),
    ).toEqual(['100', '50s', 'miss']);
    expect(filterScores(rows, { ...defaultFilters, maxNon300: null })).toHaveLength(5);
  });
  it('uses verified player aliases for a unified history', () => {
    expect(
      filterScores([{ ...base, player: 'New name', playerAliases: ['Old name', 'New name'] }], {
        ...defaultFilters,
        player: 'Old name',
      }),
    ).toHaveLength(1);
  });
  it('keeps other players out of exploration and progression for either source', () => {
    const rows = [
      {
        ...base,
        id: 'mine-stable',
        source: 'stable' as const,
        player: 'acrawford69',
        playerAliases: ['acrawford69'],
        stars: 4,
      },
      {
        ...base,
        id: 'mine-lazer',
        source: 'lazer' as const,
        player: 'ACrawford69',
        playerAliases: ['acrawford69', 'ACrawford69'],
        stars: 6,
      },
      {
        ...base,
        id: 'other-lazer',
        source: 'lazer' as const,
        player: 'WhiteCat',
        playerAliases: ['WhiteCat'],
        stars: 10,
      },
      {
        ...base,
        id: 'other-stable',
        source: 'stable' as const,
        player: 'Shinobu',
        playerAliases: ['Shinobu'],
        sources: ['stable', 'lazer'] as ('stable' | 'lazer')[],
        stars: 9,
      },
    ];
    for (const includeStable of [true, false]) {
      const filtered = filterScores(rows, {
        ...defaultFilters,
        player: 'acrawford69',
        includeStable,
      });
      expect(filtered.map((s) => s.id).sort()).toEqual(
        includeStable ? ['mine-lazer', 'mine-stable'] : ['mine-lazer'],
      );
      for (const unique of [true, false]) {
        const chart = trends(filtered, 'stars', 'month', unique);
        expect(chart.reduce((n, p) => n + p.count, 0)).toBe(unique ? 1 : filtered.length);
        expect(chart.find((p) => p.count)?.value).toBe(unique || !includeStable ? 6 : 5);
      }
    }
    expect(filterScores(rows, { ...defaultFilters, player: '' })).toHaveLength(4);
  });
  it('filters all four modes and supports all modes explicitly', () => {
    const rows = [0, 1, 2, 3].map((mode) => ({ ...base, id: String(mode), mode }));
    for (const mode of ['0', '1', '2', '3'])
      expect(filterScores(rows, { ...defaultFilters, mode }).map((s) => s.mode)).toEqual([
        Number(mode),
      ]);
    expect(filterScores(rows, { ...defaultFilters, mode: '' })).toHaveLength(4);
  });
  it('requires exactly one miss for the one miss preset', () => {
    const rows = [
      { ...base, id: 'a', misses: 0 },
      { ...base, id: 'b', misses: 1 },
      { ...base, id: 'c', misses: 2 },
    ];
    expect(filterScores(rows, { ...defaultFilters, preset: 'oneMiss' }).map((s) => s.id)).toEqual([
      'b',
    ]);
  });
  it('does not classify every zero-miss play as FC', () => {
    expect(
      filterScores([{ ...base, misses: 0, fc: false }], { ...defaultFilters, preset: 'fc' }),
    ).toHaveLength(0);
  });
  it('SS chokes require one imperfect judgement, not just high accuracy', () => {
    const rows = [
      { ...base, id: 'a', n100: 1, n50: 0, misses: 0 },
      { ...base, id: 'b', n100: 2, n50: 0, misses: 0 },
    ];
    expect(filterScores(rows, { ...defaultFilters, preset: 'ssChoke' }).map((s) => s.id)).toEqual([
      'a',
    ]);
  });
  it('keeps unavailable PP last and out of star threshold matches', () => {
    const missing = { ...base, id: 'missing', pp: null, stars: null };
    expect(filterScores([missing, base], defaultFilters).at(-1)?.id).toBe('missing');
    expect(filterScores([missing], { ...defaultFilters, minStars: 1 })).toHaveLength(0);
  });
  it('applies artist search, player and date boundaries together', () => {
    expect(
      filterScores([base], {
        ...defaultFilters,
        search: base.artist,
        player: base.player,
        from: base.date.slice(0, 10),
        to: base.date.slice(0, 10),
      }),
    ).toHaveLength(1);
  });
  it('does not repeat DT when showing NC', () => expect(modLabel(512 | 64)).toBe('NC'));
});
describe('progression', () => {
  it('leaves missing years empty and excludes unavailable stars from means', () => {
    const rows = [
      { ...base, id: 'a', date: '2017-01-01T00:00:00Z', stars: 4 },
      { ...base, id: 'b', date: '2017-02-01T00:00:00Z', stars: 6 },
      { ...base, id: 'c', date: '2017-03-01T00:00:00Z', stars: null },
      { ...base, id: 'd', date: '2019-01-01T00:00:00Z', stars: 8 },
    ];
    expect(trends(rows, 'stars', 'year', false)).toEqual([
      { period: '2017', value: 5, count: 2, missing: 1 },
      { period: '2018', value: null, count: 0, missing: 0 },
      { period: '2019', value: 8, count: 1, missing: 0 },
    ]);
  });
  it('buckets dates by UTC month', () => {
    const rows = [
      { ...base, date: '2020-12-31T23:59:59Z' },
      { ...base, date: '2021-01-01T00:00:00Z' },
    ];
    expect(trends(rows, 'stars', 'month', false).map((p) => p.period)).toEqual([
      '2020-12',
      '2021-01',
    ]);
  });
  it('selects the best unique map result independently of input sort order', () => {
    const rows = [
      { ...base, id: 'a', pp: 100 },
      { ...base, id: 'b', pp: 200 },
    ];
    expect(trends(rows, 'pp', 'year', true)[0].value).toBe(200);
    expect(trends([...rows].reverse(), 'pp', 'year', true)[0].value).toBe(200);
  });
});
describe('cross-client identity groundwork', () => {
  it('matches the same legacy ID across clients but never collides with solo IDs', () => {
    const s = { origin: 'stable' as const, ruleset: 0, mapHash: 'abc', legacyId: '123' };
    expect(strongIdentity(s)).toBe(strongIdentity({ ...s, origin: 'lazer' }));
    expect(strongIdentity(s)).not.toBe(
      strongIdentity({ ...s, legacyId: undefined, onlineId: '123' }),
    );
  });
  it('rejects sentinel IDs and isolates rulesets and map revisions', () => {
    expect(
      strongIdentity({ origin: 'lazer', ruleset: 0, mapHash: 'abc', legacyId: '-1' }),
    ).toBeNull();
    expect(
      strongIdentity({ origin: 'lazer', ruleset: 0, mapHash: 'abc', legacyId: '123' }),
    ).not.toBe(strongIdentity({ origin: 'lazer', ruleset: 1, mapHash: 'abc', legacyId: '123' }));
  });
});
