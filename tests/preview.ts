// Development-only integration fixture. No game files or real exports are used.
import { demoScores } from '../src/lib/demo';
import { defaultFilters } from '../src/lib/model';
const scores = Array.from({ length: 320 }, (_, batch) =>
  demoScores().map((s) => ({ ...s, id: `${batch}:${s.id}`, hasArtwork: false })),
).flat();
localStorage.setItem('sx-filters', JSON.stringify({ ...defaultFilters, player: 'Sample player' }));
if (new URLSearchParams(location.search).has('fresh')) localStorage.removeItem('sx-replay-folder');
const events: string[] = [];
const status = document.getElementById('test-status')!;
let inputEvents = 0;
let maxInputDelay = 0;
let longTasks = 0;
let monitoring = false;
function report() {
  status.textContent = `Synthetic ${scores.length} scores | inputs ${inputEvents} | max input delay ${maxInputDelay.toFixed(1)} ms | long tasks ${longTasks} | ${events.join(' → ')}`;
}
new PerformanceObserver((list) => {
  if (monitoring) longTasks += list.getEntries().length;
  report();
}).observe({ type: 'longtask', buffered: false });
document.addEventListener(
  'input',
  (e) => {
    monitoring = true;
    inputEvents++;
    maxInputDelay = Math.max(maxInputDelay, performance.now() - e.timeStamp);
    report();
  },
  true,
);
Object.assign(window, {
  __TAURI_INTERNALS__: {
    invoke: async (command: string, args: Record<string, unknown>) => {
      if (command === 'load_library')
        return {
          scores,
          warnings: [],
          defaultPlayer: 'Sample player',
          engine: 'Synthetic test fixture',
          stableScores: scores.length,
          lazerScores: 0,
          mergedDuplicates: 0,
          nativeLazerScores: 0,
        };
      if (command === 'replay_availability')
        return {
          available: (args.ids as string[]).map((id) => ({ id, filename: 'sample.osr' })),
          warning: null,
        };
      if (command === 'plugin:dialog|open') {
        events.push('folder prompt');
        report();
        return 'Test exports';
      }
      if (command === 'export_replay') {
        events.push(`export to ${args.directory}`);
        report();
        return 'Test exports/sample.osr';
      }
      throw Error(`Unexpected preview command: ${command}`);
    },
  },
});
report();
await import('../src/main');
