import { filterScores, trends, type Filters, type Score, type TrendPoint } from './model';

export type Query = {
  requestId: number;
  filters: Filters;
  page: number;
  size: number;
  metric: 'stars' | 'pp' | 'accuracy';
  bucket: 'month' | 'year';
  unique: boolean;
  progression: boolean;
};
export type QueryResult = {
  requestId: number;
  total: number;
  ids: string[];
  maps: number;
  fcs: number;
  years: string[];
  points: TrendPoint[];
};
export function queryLibrary(scores: Score[], query: Query): QueryResult {
  const filtered = filterScores(scores, query.filters);
  return {
    requestId: query.requestId,
    total: filtered.length,
    ids: filtered.slice(query.page * query.size, (query.page + 1) * query.size).map((s) => s.id),
    maps: new Set(filtered.map((s) => s.mapHash)).size,
    fcs: filtered.reduce((n, s) => n + Number(s.fc), 0),
    years: [...new Set(filtered.map((s) => s.date.slice(0, 4)))].sort(),
    points:
      query.progression && query.filters.player && query.filters.mode !== ''
        ? trends(filtered, query.metric, query.bucket, query.unique)
        : [],
  };
}
