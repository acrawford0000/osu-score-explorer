import { afterEach, expect, test, vi } from 'vitest';
import { withTimeout } from './async';

afterEach(() => vi.useRealTimers());

test('a stalled lookup stops waiting at the deadline', async () => {
  vi.useFakeTimers();
  const result = expect(withTimeout(new Promise(() => {}), 8000, 'Audio unavailable')).rejects.toThrow('Audio unavailable');
  await vi.advanceTimersByTimeAsync(8000);
  await result;
  expect(vi.getTimerCount()).toBe(0);
});

test('successful and failed lookups clear the deadline', async () => {
  vi.useFakeTimers();
  await expect(withTimeout(Promise.resolve(null), 8000, 'timeout')).resolves.toBeNull();
  await expect(withTimeout(Promise.reject(new Error('missing')), 8000, 'timeout')).rejects.toThrow('missing');
  expect(vi.getTimerCount()).toBe(0);
});
