import { expect, test } from 'vitest';
import { demoScores } from './demo';
import { defaultFilters, filterScores, playerChoices } from './model';
import { queryLibrary } from './query';
test('one account covers capitalization, historical names, scores, and progression', () => {
  const base = demoScores()[0];
  const scores = [
    { ...base, id: 'a', player: 'Old Name', playerId: 42, playerAliases: ['Old Name', 'current'] },
    { ...base, id: 'b', player: 'CURRENT', playerId: 42, playerAliases: ['Old Name', 'current'] },
    { ...base, id: 'c', player: 'Other', playerId: 99 },
  ];
  const choices = playerChoices(scores, [
    { id: 42, name: 'current', aliases: ['Old Name', 'current'] },
    { id: 99, name: 'Other', aliases: ['Other'] },
  ]);
  expect(choices).toHaveLength(2);
  expect(choices.find((c) => c.value === 'id:42')?.label).toBe('current');
  const filters = { ...defaultFilters, player: 'id:42', mode: '' };
  expect(
    filterScores(scores, filters)
      .map((s) => s.id)
      .sort(),
  ).toEqual(['a', 'b']);
  const result = queryLibrary(scores, {
    requestId: 1,
    filters: { ...filters, mode: String(base.mode) },
    page: 0,
    size: 36,
    metric: 'stars',
    bucket: 'year',
    unique: false,
    progression: true,
  });
  expect(result.total).toBe(2);
  expect(result.points.reduce((n, p) => n + p.count + p.missing, 0)).toBe(2);
});
test('unidentified names ignore case without merging distinct verified accounts', () => {
  const base = demoScores()[0];
  const scores = [
    { ...base, id: 'a', player: 'Mixed Name' },
    { ...base, id: 'b', player: 'mixed name' },
  ];
  expect(playerChoices(scores)).toHaveLength(1);
  expect(filterScores(scores, { ...defaultFilters, mode: '', player: 'MIXED NAME' })).toHaveLength(
    2,
  );
  expect(playerChoices(scores.map((s, i) => ({ ...s, playerId: 40 + i })))).toHaveLength(2);
});

test('selecting an unlinked reused name does not include either known account', () => {
  const base = demoScores()[0];
  const scores = [
    { ...base, id: 'a', player: 'Old Name', playerId: 42 },
    { ...base, id: 'b', player: 'old name', playerId: 99 },
    { ...base, id: 'c', player: 'OLD NAME' },
  ];
  const choices = playerChoices(scores);
  expect(choices).toHaveLength(3);
  expect(new Set(choices.map((c) => c.label)).size).toBe(3);
  expect(
    filterScores(scores, { ...defaultFilters, mode: '', player: 'name:old name' }).map((s) => s.id),
  ).toEqual(['c']);
});
