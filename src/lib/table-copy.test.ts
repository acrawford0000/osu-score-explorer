import { expect, test } from 'vitest';
import { formatTable, textWidth } from './table-copy';
test('copies displayed values in order with aligned numeric columns and a Discord code fence', () => {
  const table = formatTable(
    'Average stars',
    [{ label: 'Period' }, { label: 'Average', right: true }],
    [
      ['2017', '4.20'],
      ['2018', '—'],
    ],
  );
  expect(table.parts).toEqual([table.full]);
  expect(table.full).toContain('2017   |    4.20');
  expect(table.full).toContain('2018   |       —');
  expect(table.full.startsWith('```\n')).toBe(true);
  expect(table.full.endsWith('\n```')).toBe(true);
});
test('splits long tables into complete Discord messages with repeated headers and no missing rows', () => {
  const rows = Array.from({ length: 60 }, (_, i) => [`Map ${i}`, `${i}.50`]);
  const table = formatTable('Scores', [{ label: 'Map' }, { label: 'PP', right: true }], rows, 300);
  expect(table.parts.length).toBeGreaterThan(1);
  expect(table.parts.every((p) => p.length <= 300 && p.includes('Map    |    PP'))).toBe(true);
  const copied = table.parts.flatMap((p) => p.split('\n').filter((s) => /^Map \d/.test(s)));
  expect(copied).toHaveLength(60);
  expect(copied[0]).toContain('Map 0 ');
  expect(copied.at(-1)).toContain('Map 59');
});
test('keeps CJK and emoji padding readable and prevents embedded line breaks or code-fence escapes', () => {
  expect(textWidth('曲A')).toBe(3);
  expect(textWidth('🎵')).toBe(2);
  const table = formatTable(
    'Scores',
    [{ label: 'Map', maxWidth: 8 }],
    [['曲曲曲曲曲'], ['hello\n```@everyone']],
  );
  expect(table.full).toContain('曲曲曲…');
  expect(table.full.match(/```/g)).toHaveLength(2);
  expect(table.full).not.toContain('\n@everyone');
});
