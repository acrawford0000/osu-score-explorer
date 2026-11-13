export type TableColumn = { label: string; right?: boolean; maxWidth?: number };
const segmenter = new Intl.Segmenter(undefined, { granularity: 'grapheme' });
const segments = (value: string) => [...segmenter.segment(value)].map((s) => s.segment);
const clean = (value: string) => value.replace(/`/g, "'").replace(/\s+/g, ' ').trim();
const width = (s: string) =>
  /\p{Extended_Pictographic}|[\u1100-\u115f\u2e80-\ua4cf\uac00-\ud7af\uf900-\ufaff\uff01-\uff60]/u.test(
    s,
  )
    ? 2
    : 1;
export const textWidth = (value: string) => segments(value).reduce((sum, s) => sum + width(s), 0);
function clip(value: string, limit: number): string {
  const text = clean(value);
  if (textWidth(text) <= limit) return text;
  let result = '',
    used = 0;
  for (const part of segments(text)) {
    if (used + width(part) > limit - 1) break;
    result += part;
    used += width(part);
  }
  return result + '…';
}
export function formatTable(
  title: string,
  columns: TableColumn[],
  rows: string[][],
  limit = 2000,
): { full: string; parts: string[] } {
  const values = rows.map((row) =>
    columns.map((c, i) => clip(row[i] ?? '—', Math.max(textWidth(c.label), c.maxWidth ?? 60))),
  );
  const widths = columns.map((c, i) =>
    Math.max(textWidth(c.label), ...values.map((row) => textWidth(row[i]))),
  );
  const line = (row: string[]) =>
    row
      .map((cell, i) => {
        const padding = ' '.repeat(Math.max(0, widths[i] - textWidth(cell)));
        return columns[i].right ? padding + cell : cell + padding;
      })
      .join(' | ')
      .trimEnd();
  const prefix = `${clip(title, 120)}\n${line(columns.map((c) => c.label))}\n${widths.map((w) => '-'.repeat(w)).join('-+-')}\n`;
  const lines = values.length ? values.map(line) : ['(no rows)'];
  const fence = (body: string) => '```\n' + body.trimEnd() + '\n```';
  const parts: string[] = [];
  let current: string[] = [];
  for (const row of lines) {
    if (current.length && fence(prefix + [...current, row].join('\n')).length > limit) {
      parts.push(fence(prefix + current.join('\n')));
      current = [];
    }
    if (fence(prefix + row).length > limit)
      throw Error('This table is too wide for one Discord message.');
    current.push(row);
  }
  if (current.length) parts.push(fence(prefix + current.join('\n')));
  return { full: fence(prefix + lines.join('\n')), parts };
}
