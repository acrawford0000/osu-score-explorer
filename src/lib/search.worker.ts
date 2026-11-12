/// <reference lib="webworker" />
import { queryLibrary, type Query } from './query';
import type { Score } from './model';

let scores: Score[] = [];
self.onmessage = (
  event: MessageEvent<{ type: 'load'; scores: Score[] } | { type: 'query'; query: Query }>,
) => {
  if (event.data.type === 'load') scores = event.data.scores;
  else self.postMessage(queryLibrary(scores, event.data.query));
};
