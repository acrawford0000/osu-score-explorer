/// <reference lib="webworker" />
import { queryLibrary, type Query } from './query';
import type { Score } from './model';

let scores: Score[] = [];
let positions = new Map<string, number>();
self.onmessage = (
  event: MessageEvent<
    { type: 'load' | 'patch'; scores: Score[] } | { type: 'query'; query: Query }
  >,
) => {
  if (event.data.type === 'query') self.postMessage(queryLibrary(scores, event.data.query));
  else if (event.data.type === 'load') {
    scores = event.data.scores;
    positions = new Map(scores.map((s, i) => [s.id, i]));
  } else
    for (const score of event.data.scores) {
      const index = positions.get(score.id);
      if (index !== undefined) scores[index] = score;
    }
};
