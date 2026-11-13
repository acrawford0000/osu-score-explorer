import { expect, test } from 'vitest';
import { checkReplayPage } from './replayQueue';
import type { OnlineReplayStatus } from './backend';
const status = (id: string, state: string): OnlineReplayStatus => ({
  id,
  state,
  message: state,
  retryAfter: null,
});
test('one rejected lookup and one mismatch do not skip later scores', async () => {
  const checked: string[] = [],
    results: string[] = [],
    errors: string[] = [];
  await checkReplayPage(
    ['bad', 'mismatch', 'good'],
    async (id) => {
      checked.push(id);
      if (id === 'bad') throw Error('lookup failed');
      return status(id, id === 'mismatch' ? 'mismatch' : 'available');
    },
    () => true,
    (s) => results.push(s.state),
    (id) => errors.push(id),
  );
  expect(checked).toEqual(['bad', 'mismatch', 'good']);
  expect(results).toEqual(['mismatch', 'available']);
  expect(errors).toEqual(['bad']);
});
test('rate limits pause remaining requests', async () => {
  const checked: string[] = [];
  await checkReplayPage(
    ['limited', 'later'],
    async (id) => {
      checked.push(id);
      return status(id, 'rate-limited');
    },
    () => true,
    () => {},
    () => {},
  );
  expect(checked).toEqual(['limited']);
});
test('changing pages discards late results and stops old requests', async () => {
  let active = true;
  const checked: string[] = [],
    results: string[] = [];
  await checkReplayPage(
    ['old', 'next'],
    async (id) => {
      checked.push(id);
      active = false;
      return status(id, 'available');
    },
    () => active,
    (s) => results.push(s.id),
    () => {},
  );
  expect(checked).toEqual(['old']);
  expect(results).toEqual([]);
});
