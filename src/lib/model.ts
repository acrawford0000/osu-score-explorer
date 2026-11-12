export type Score = {
  id: string;
  mapHash: string;
  player: string;
  date: string;
  mode: number;
  title: string;
  artist: string;
  difficulty: string;
  mapper: string;
  mods: number;
  n300: number;
  n100: number;
  n50: number;
  misses: number;
  nGeki?: number;
  nKatu?: number;
  combo: number;
  maxCombo: number | null;
  rawScore: number;
  accuracy: number | null;
  fc: boolean;
  ss: boolean;
  stars: number | null;
  pp: number | null;
  fcPp: number | null;
  source: 'stable' | 'lazer';
  status: string;
  hasArtwork: boolean;
  sources?: ('stable' | 'lazer')[];
  playerAliases?: string[];
  modsDisplay?: string;
  modsJson?: string;
  gameplayLazer?: boolean;
};
export type Library = {
  scores: Score[];
  warnings: string[];
  engine: string;
  importedAt: string | null;
  defaultPlayer: string | null;
  reused: number;
  calculated: number;
  unchanged: number;
  newScores: number;
  sourceUnchanged: boolean;
  scanMs: number;
  stableScores: number;
  lazerScores: number;
  mergedDuplicates: number;
  nativeLazerScores: number;
  lazerRoot?: string | null;
};
export type Filters = {
  search: string;
  preset: string;
  sort: string;
  player: string;
  mode: string;
  minStars: number;
  maxMisses: number | null;
  mod: string;
  from: string;
  to: string;
  includeStable: boolean;
  includeLazer: boolean;
  maxNon300: number | null;
};
export const defaultFilters: Filters = {
  search: '',
  preset: 'all',
  sort: 'pp',
  player: '',
  mode: '0',
  minStars: 0,
  maxMisses: null,
  mod: '',
  from: '',
  to: '',
  includeStable: true,
  includeLazer: true,
  maxNon300: null,
};
export const scoreMods = (score: Score) => score.modsDisplay || modLabel(score.mods);
export function hitBreakdown(s: Score): { text: string; label: string } {
  const counts =
    s.mode === 3
      ? [s.nGeki ?? 0, s.n300, s.nKatu ?? 0, s.n100, s.n50]
      : s.mode === 1
        ? [s.n300, s.n100]
        : s.mode === 2
          ? [s.n300, s.n100, s.n50, s.nKatu ?? 0]
          : [s.n300, s.n100, s.n50];
  const labels =
    s.mode === 3
      ? ['MAX', '300', '200', '100', '50']
      : s.mode === 1
        ? ['300', '100']
        : s.mode === 2
          ? ['fruits', 'droplets', 'tiny droplets', 'tiny droplet misses']
          : ['300', '100', '50'];
  return {
    text: `${counts.join(' / ')} · ${s.misses}× miss`,
    label: `${counts.map((n, i) => `${labels[i]}: ${n}`).join(', ')}; misses: ${s.misses}`,
  };
}
export function non300(s: Score): number {
  return s.mode === 2
    ? (s.nKatu ?? 0) + s.misses
    : s.mode === 1
      ? s.n100 + s.misses
      : s.n100 + s.n50 + s.misses + (s.mode === 3 ? (s.nKatu ?? 0) : 0);
}
export function modLabel(bits: number): string {
  const names: string[] = [];
  for (const [bit, name] of [
    [1, 'NF'],
    [2, 'EZ'],
    [8, 'HD'],
    [16, 'HR'],
    [32, 'SD'],
    [64, 'DT'],
    [128, 'RX'],
    [256, 'HT'],
    [512, 'NC'],
    [1024, 'FL'],
    [4096, 'SO'],
    [16384, 'PF'],
    [536870912, 'V2'],
  ] as const) {
    if (bits & bit && !(bit === 64 && bits & 512) && !(bit === 32 && bits & 16384))
      names.push(name);
  }
  return names.join('') || 'NM';
}
export function filterScores(scores: Score[], f: Filters): Score[] {
  const terms = f.search.toLocaleLowerCase().trim().split(/\s+/).filter(Boolean);
  const selected = scores.filter((s) => {
    const sources = s.sources?.length ? s.sources : [s.source];
    if (!sources.some((source) => (source === 'stable' ? f.includeStable : f.includeLazer)))
      return false;
    if (f.mode !== '' && s.mode !== Number(f.mode)) return false;
    const haystack =
      `${s.artist} ${s.title} ${s.difficulty} ${s.mapper} ${s.mapHash}`.toLocaleLowerCase();
    if (
      !terms.every((t) => haystack.includes(t)) ||
      (f.player && s.player !== f.player && !s.playerAliases?.includes(f.player))
    )
      return false;
    if (f.minStars > 0 && (s.stars === null || s.stars < f.minStars)) return false;
    if (f.maxMisses !== null && s.misses > f.maxMisses) return false;
    if (f.mod && scoreMods(s) !== f.mod) return false;
    if (f.maxNon300 !== null && non300(s) > f.maxNon300) return false;
    if ((f.from && s.date.slice(0, 10) < f.from) || (f.to && s.date.slice(0, 10) > f.to))
      return false;
    return (
      f.preset === 'all' ||
      (f.preset === 'oneMiss' && s.misses === 1) ||
      (f.preset === 'ssChoke' && !s.ss && non300(s) > 0 && non300(s) <= (f.maxNon300 ?? 1)) ||
      (f.preset === 'fc' && s.fc) ||
      (f.preset === 'ss' && s.ss) ||
      (f.preset === 'comboChoke' &&
        s.misses === 0 &&
        !s.fc &&
        s.maxCombo !== null &&
        s.combo < s.maxCombo)
    );
  });
  const value = (s: Score): number | null =>
    f.sort === 'stars'
      ? s.stars
      : f.sort === 'accuracy'
        ? s.accuracy
        : f.sort === 'date'
          ? Date.parse(s.date)
          : f.sort === 'gain'
            ? s.fcPp !== null && s.pp !== null
              ? s.fcPp - s.pp
              : null
            : s.pp;
  return selected.sort((a, b) => {
    const x = value(a),
      y = value(b);
    return x === null
      ? y === null
        ? b.date.localeCompare(a.date)
        : 1
      : y === null
        ? -1
        : y - x || b.date.localeCompare(a.date) || a.id.localeCompare(b.id);
  });
}
export type TrendPoint = { period: string; value: number | null; count: number; missing: number };
export function trends(
  scores: Score[],
  metric: 'stars' | 'pp' | 'accuracy',
  bucket: 'year' | 'month',
  unique: boolean,
): TrendPoint[] {
  if (!scores.length) return [];
  const groups = new Map<string, Score[]>();
  for (const s of scores) {
    const key = s.date.slice(0, bucket === 'year' ? 4 : 7);
    const group = groups.get(key);
    if (group) group.push(s);
    else groups.set(key, [s]);
  }
  const keys = [...groups.keys()].sort();
  const cursor = new Date(`${keys[0]}${bucket === 'year' ? '-01' : ''}-01T00:00:00Z`);
  const end = keys.at(-1)!;
  const points: TrendPoint[] = [];
  while (cursor.toISOString().slice(0, bucket === 'year' ? 4 : 7) <= end) {
    const period = cursor.toISOString().slice(0, bucket === 'year' ? 4 : 7);
    let rows = groups.get(period) || [];
    if (unique) {
      const best = new Map<string, Score>();
      for (const row of rows) {
        const key = `${row.mapHash}:${row.modsJson || row.mods}:${row.mode}`;
        const previous = best.get(key);
        if (!previous || (row[metric] ?? -Infinity) > (previous[metric] ?? -Infinity))
          best.set(key, row);
      }
      rows = [...best.values()];
    }
    const vals = rows
      .map((s) => s[metric])
      .filter((n): n is number => n !== null && Number.isFinite(n));
    points.push({
      period,
      value: vals.length ? vals.reduce((a, b) => a + b, 0) / vals.length : null,
      count: vals.length,
      missing: rows.length - vals.length,
    });
    if (bucket === 'year') cursor.setUTCFullYear(cursor.getUTCFullYear() + 1);
    else cursor.setUTCMonth(cursor.getUTCMonth() + 1);
  }
  return points;
}

// For future adapters: IDs from different namespaces must never collide.
export type Identity = {
  origin: 'stable' | 'lazer';
  ruleset: number;
  mapHash: string;
  legacyId?: string;
  onlineId?: string;
};
export function strongIdentity(s: Identity): string | null {
  if (s.legacyId && /^[1-9]\d*$/.test(s.legacyId))
    return `legacy:${s.ruleset}:${s.mapHash}:${s.legacyId}`;
  if (s.onlineId && /^[1-9]\d*$/.test(s.onlineId))
    return `solo:${s.ruleset}:${s.mapHash}:${s.onlineId}`;
  return null;
}
