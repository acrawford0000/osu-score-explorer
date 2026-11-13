import type { OnlineReplayStatus } from './backend';

// One score may fail verification or time out without abandoning the page.
// Rate limits are different: stop issuing requests until the caller retries.
export async function checkReplayPage(
  ids: string[],
  check: (id: string) => Promise<OnlineReplayStatus>,
  active: () => boolean,
  result: (value: OnlineReplayStatus) => void,
  failed: (id: string, error: unknown) => void,
) {
  for (const id of ids) {
    if (!active()) return;
    let value: OnlineReplayStatus;
    try {
      value = await check(id);
    } catch (error) {
      if (!active()) return;
      failed(id, error);
      continue;
    }
    if (!active()) return;
    result(value);
    if (value.state === 'rate-limited') return;
  }
}
