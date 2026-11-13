// Development-only integration fixture. No game files or real exports are used.
import { demoScores } from '../src/lib/demo';
import { defaultFilters } from '../src/lib/model';
const reusedIdentity = new URLSearchParams(location.search).get('identity') === 'reused';
const scores = Array.from({ length: 320 }, (_, batch) =>
  demoScores().map((s) => ({ ...s, id: `${batch}:${s.id}`, hasArtwork: false })),
)
  .flat()
  .map((s, i) =>
    reusedIdentity
      ? {
          ...s,
          player: i % 3 === 0 ? 'Current Name' : 'Old Name',
          playerId: i % 3 === 0 ? 42 : 99,
          playerAliases: [i % 3 === 0 ? 'Current Name' : 'Old Name'],
          date: i % 3 === 1 ? '2018-06-01T00:00:00Z' : s.date,
        }
      : s,
  );
localStorage.setItem(
  'sx-filters',
  JSON.stringify({ ...defaultFilters, player: reusedIdentity ? 'Current Name' : 'Sample player' }),
);
if (new URLSearchParams(location.search).has('fresh')) localStorage.removeItem('sx-replay-folder');
const events: string[] = [];
let calculator = 'lazer';
let downloadStep = -1;
let downloadPaused = false;
let sources = [
  { kind: 'stable', root: 'C:\\Games\\osu!', songs: null },
  { kind: 'lazer', root: 'C:\\Games\\osu-lazer', songs: null },
];
const wav = new Uint8Array(44 + 160000);
wav.fill(128, 44);
const header = new DataView(wav.buffer);
for (const [offset, text] of [
  [0, 'RIFF'],
  [8, 'WAVEfmt '],
  [36, 'data'],
] as const)
  for (let i = 0; i < text.length; i++) wav[offset + i] = text.charCodeAt(i);
header.setUint32(4, wav.length - 8, true);
header.setUint32(16, 16, true);
header.setUint16(20, 1, true);
header.setUint16(22, 1, true);
header.setUint32(24, 8000, true);
header.setUint32(28, 8000, true);
header.setUint16(32, 1, true);
header.setUint16(34, 8, true);
header.setUint32(40, 160000, true);
const silentAudio =
  'data:audio/wav;base64,' + btoa(Array.from(wav, (b) => String.fromCharCode(b)).join(''));
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
    convertFileSrc: (path: string) => path,
    invoke: async (command: string, args: Record<string, unknown>) => {
      if (command === 'calculation_status')
        return {
          running: false,
          processed: 0,
          total: 0,
          calculated: 0,
          unavailable: 0,
          message: '',
          updates: [],
        };
      if (command === 'start_calculations') return;
      if (command === 'refresh_player_identity')
        return {
          accounts: reusedIdentity
            ? [
                { id: 42, name: 'Current Name', aliases: ['Current Name', 'Old Name'] },
                { id: 99, name: 'Old Name', aliases: ['Old Name'] },
              ]
            : [],
          assignments: reusedIdentity ? scores.map((s, i) => [s.id, i % 3 === 2 ? 99 : 42]) : [],
        };
      if (command === 'select_calculator') calculator = String(args.calculator);
      if (command === 'download_status') {
        if (downloadStep >= 0 && !downloadPaused) downloadStep++;
        const done = downloadStep >= 5;
        return {
          running: downloadStep >= 0 && !done && !downloadPaused,
          phase: downloadPaused
            ? 'paused'
            : done
              ? 'finished'
              : downloadStep >= 0
                ? 'downloading'
                : '',
          message: downloadPaused
            ? 'Download queue paused. Resume when ready.'
            : done
              ? '1 map revision ready'
              : downloadStep >= 0
                ? 'Downloading mapset 5 without video…'
                : '',
          total: downloadStep < 0 ? 0 : 1,
          completed: done ? 1 : 0,
          pending: done || downloadStep < 0 ? 0 : 1,
          unavailable: 0,
          bytes: done ? 0 : 1048576,
          totalBytes: 2097152,
          installedSets: done ? 1 : 0,
          updates:
            downloadStep === 5
              ? [{ ...scores[0], pp: 999, fcPp: 1001, title: 'Recovered song' }]
              : [],
          issues: [],
        };
      }
      if (command === 'start_downloads') {
        downloadStep = 0;
        downloadPaused = false;
        events.push('download queue started');
        report();
        return;
      }
      if (command === 'pause_downloads') {
        downloadPaused = true;
        events.push('download queue paused');
        report();
        return;
      }
      if (
        command === 'load_library' ||
        command === 'import_stable' ||
        command === 'import_lazer' ||
        command === 'select_calculator'
      )
        return {
          calculator,
          playerAccounts: reusedIdentity
            ? [
                { id: 42, name: 'Current Name', aliases: ['Current Name'] },
                { id: 99, name: 'Old Name', aliases: ['Old Name'] },
              ]
            : [],
          scores,
          warnings: [],
          defaultPlayer: reusedIdentity ? 'Current Name' : 'Sample player',
          engine: 'Synthetic test fixture',
          calculated: 0,
          reused: scores.length,
          unchanged: scores.length,
          newScores: 0,
          sourceUnchanged: true,
          scanMs: 0,
          stableScores: scores.length,
          lazerScores: 0,
          mergedDuplicates: 0,
          nativeLazerScores: 0,
          gameSources: sources,
        };
      if (command === 'save_sources') {
        sources = args.sources as typeof sources;
        events.push(`saved ${sources.length} folders`);
        report();
        return sources;
      }
      if (command === 'audio_track') {
        const behavior = new URLSearchParams(location.search).get('audio');
        if (behavior === 'missing') return null;
        if (behavior === 'hang') return new Promise(() => {});
        return silentAudio;
      }
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
