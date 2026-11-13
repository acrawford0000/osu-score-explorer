<script lang="ts">
  import { onMount, tick } from 'svelte';
  import { version } from '../package.json';
  import {
    Search,
    SlidersHorizontal,
    LayoutGrid,
    List,
    ArrowUpRight,
    ChevronRight,
    Sun,
    Moon,
    FolderOpen,
    X,
    TrendingUp,
    Settings,
    Disc3,
    Target,
    Star,
    Trophy,
    RefreshCw,
    Database,
    ChevronLeft,
    Check,
    Info,
    Download,
  } from '@lucide/svelte';
  import Artwork from './lib/Artwork.svelte';
  import CopyTable from './lib/CopyTable.svelte';
  import TrendChart from './lib/TrendChart.svelte';
  import MusicPlayer, { type Track } from './lib/MusicPlayer.svelte';
  import DownloadPanel from './lib/DownloadPanel.svelte';
  import {
    defaultFilters,
    modLabel,
    scoreMods,
    hitBreakdown,
    type Score,
    type Library,
    type Filters,
    type GameSource,
    clampPage,
    playerChoices,
  } from './lib/model';
  import { demoScores } from './lib/demo';
  import {
    isDesktop,
    loadLibrary,
    importStable,
    importLazer,
    replayAvailability,
    exportReplay,
    connectOnlineReplays,
    disconnectOnlineReplays,
    restoreApiConnection,
    forgetApiConnection,
    openApiRegistration,
    type ApiConnection,
    checkOnlineReplay,
    type OnlineReplayStatus,
    saveSources,
    audioTrack,
    startDownloads,
    downloadStatus,
    pauseDownloads,
    type DownloadStatus,
    calculationStatus,
    startCalculations,
    selectCalculator,
    type CalculationStatus,
    savePlayerAccount,
    refreshPlayerIdentity,
  } from './lib/backend';
  import { themes } from './lib/themes';
  import type { QueryResult } from './lib/query';
  import { withTimeout } from './lib/async';
  import { checkReplayPage } from './lib/replayQueue';

  function showDialog(node: HTMLDialogElement) {
    const previous = document.activeElement as HTMLElement | null;
    node.showModal();
    return {
      destroy() {
        node.close();
        previous?.focus();
      },
    };
  }
  function read<T>(key: string, fallback: T): T {
    try {
      return JSON.parse(localStorage.getItem(key) || 'null') ?? fallback;
    } catch {
      return fallback;
    }
  }
  let filters = $state<Filters>({ ...defaultFilters, ...read('sx-filters', {}) });
  let view = $state<'cards' | 'table'>(read('sx-view', 'cards'));
  let section = $state<'explore' | 'trends' | 'settings'>('explore');
  let theme = $state(localStorage.getItem('sx-theme') || 'dark');
  let scores = $state.raw<Score[]>([]),
    sample = $state(false),
    library = $state.raw<Library | null>(null);
  let busy = $state(false),
    error = $state(''),
    message = $state(''),
    importing = $state(false),
    advanced = $state(false);
  let gameSources = $state<GameSource[]>(
    read(
      'sx-game-sources',
      [
        { kind: 'stable' as const, root: read('sx-root', ''), songs: read('sx-songs', '') || null },
        { kind: 'lazer' as const, root: read('sx-lazer-root', ''), songs: null },
      ].filter((s) => s.root),
    ),
  );
  let replayFolder = $state(read<string>('sx-replay-folder', ''));
  let automaticDownloads = $state(read<boolean>('sx-auto-downloads', false));
  let downloadSongs = $state(read<string>('sx-download-songs', ''));
  let apiClientId = $state(''),
    apiClientSecret = $state('');
  let queue = $state.raw<DownloadStatus | null>(null);
  let calculator = $state('lazer');
  let calculationProgress = $state.raw<CalculationStatus | null>(null);
  let selectingCalculator = $state(false);
  let identityId = $state(''),
    identityName = $state(''),
    identityAliases = $state('');
  const downloadFolders = $derived([
    ...new Set(
      gameSources
        .filter((s) => s.kind === 'stable' && s.root)
        .map((s) => s.songs || `${s.root.replace(/[\\/]+$/, '')}/Songs`),
    ),
  ]);
  $effect(() => {
    localStorage.setItem('sx-auto-downloads', JSON.stringify(automaticDownloads));
    localStorage.setItem('sx-download-songs', JSON.stringify(downloadSongs));
  });
  let replayIds = $state.raw(new Set<string>());
  let replayWarning = $state('');
  let onlineEnabled = $state(read<boolean>('sx-online-replays', true));
  let onlineConnected = $state(false),
    onlineConnecting = $state(false),
    onlineChecking = $state(false);
  let onlineWarning = $state(''),
    onlineEpoch = $state(0);
  let onlineReplays = $state.raw(new Map<string, OnlineReplayStatus>());
  let onlineProgress = $state({ checked: 0, total: 0, available: 0, skipped: 0 });
  const onlineProgressLabel = $derived(
    `Online replays: ${onlineProgress.checked}/${onlineProgress.total} checked · ${onlineProgress.available} available${onlineProgress.skipped ? ` · ${onlineProgress.skipped} could not be verified` : ''}${onlineChecking ? ' · Checking…' : onlineProgress.checked < onlineProgress.total ? ' · Paused' : ''}`,
  );
  let exporting = $state('');
  let exportFeedback = $state<{ id: string; text: string } | null>(null);
  let track = $state.raw<Track | null>(null);
  let loadingSong = $state(false),
    songMessage = $state('');
  let songRequest = 0;
  $effect(() => {
    void selected?.id;
    songRequest++;
    loadingSong = false;
    songMessage = '';
  });
  let selected = $state<Score | null>(null),
    page = $state(0);
  let metric = $state<'stars' | 'pp' | 'accuracy'>(read('sx-metric', 'stars')),
    bucket = $state<'year' | 'month'>('year'),
    unique = $state(false);
  const size = 36;
  const presets = [
    { id: 'all', label: 'All scores', icon: Disc3 },
    { id: 'oneMiss', label: '1× miss chokes', icon: Target },
    { id: 'ssChoke', label: 'SS chokes', icon: Star },
    { id: 'fc', label: 'Full combos', icon: Trophy },
    { id: 'ss', label: 'SS scores', icon: Check },
    { id: 'comboChoke', label: 'Combo chokes', icon: Target },
  ];
  let worker = $state.raw<Worker | null>(null);
  let searching = $state(false);
  let requestId = 0;
  let result = $state.raw<QueryResult>({
    requestId: 0,
    total: 0,
    ids: [],
    maps: 0,
    fcs: 0,
    years: [],
    points: [],
  });
  let byId = $derived(new Map(scores.map((s) => [s.id, s])));
  let patchedScores: Score[] | null = null;
  function applyUpdates(updates: Score[]) {
    if (sample || !updates.length) return;
    const patches = new Map(updates.map((s) => [s.id, s]));
    const changed: Score[] = [];
    const next = scores.map((s) => {
      const patch = patches.get(s.id);
      if (!patch) return s;
      const sameEngine =
        !patch.calculationVersion ||
        patch.calculationVersion.startsWith('lazer-2026') === (calculator === 'lazer');
      const updated = !sameEngine
        ? {
            ...s,
            title: patch.title,
            artist: patch.artist,
            difficulty: patch.difficulty,
            mapper: patch.mapper,
            hasArtwork: patch.hasArtwork,
          }
        : {
            ...s,
            ...patch,
            sources: s.sources,
            playerAliases: s.playerAliases,
            playerId: s.playerId,
          };
      changed.push(updated);
      return updated;
    });
    if (worker) {
      patchedScores = next;
      worker.postMessage({ type: 'patch', scores: changed });
    }
    scores = next;
    if (selected && patches.has(selected.id))
      selected = scores.find((s) => s.id === selected!.id) || selected;
  }
  onMount(() => {
    if (!isDesktop) return;
    let active = true;
    let timer: ReturnType<typeof setTimeout>;
    async function poll() {
      try {
        const [status, calculations] = await Promise.all([downloadStatus(), calculationStatus()]);
        if (!active) return;
        queue = status;
        calculationProgress = calculations;
        applyUpdates([...status.updates, ...(calculator === 'lazer' ? calculations.updates : [])]);
      } catch {
        /* An older preview may not expose a queue yet. */
      }
      if (active) timer = setTimeout(poll, 700);
    }
    void poll();
    return () => {
      active = false;
      clearTimeout(timer);
    };
  });
  let visible = $derived(
    result.ids.flatMap((id) => {
      const s = byId.get(id);
      return s ? [s] : [];
    }),
  );
  let replayPageKey = $derived(
    JSON.stringify([...new Set([...(selected ? [selected.id] : []), ...visible.map((s) => s.id)])]),
  );
  $effect(() => {
    localStorage.setItem('sx-replay-folder', JSON.stringify(replayFolder));
  });
  $effect(() => {
    localStorage.setItem('sx-online-replays', JSON.stringify(onlineEnabled));
  });
  $effect(() => {
    const ids: string[] = JSON.parse(replayPageKey);
    const online = onlineEnabled && onlineConnected;
    onlineChecking = false;
    onlineProgress = { checked: 0, total: 0, available: 0, skipped: 0 };
    void onlineEpoch;
    if (!isDesktop || sample || !ids.length) {
      replayIds = new Set();
      return;
    }
    let active = true;
    let retry: ReturnType<typeof setTimeout> | undefined;
    replayIds = new Set();
    replayAvailability(ids)
      .then(async (status) => {
        if (!active) return;
        replayIds = new Set(status.available.map((r) => r.id));
        replayWarning = status.warning || '';
        if (!online) return;
        await new Promise((resolve) => setTimeout(resolve, 300));
        if (!active) return;
        const pending = ids.filter((id) => !status.available.some((r) => r.id === id));
        onlineProgress = { checked: 0, total: pending.length, available: 0, skipped: 0 };
        onlineChecking = true;
        try {
          await checkReplayPage(
            pending,
            checkOnlineReplay,
            () => active,
            (result) => {
              onlineReplays = new Map(onlineReplays).set(result.id, result);
              if (result.state === 'rate-limited') {
                onlineWarning = result.message;
                retry = setTimeout(
                  () => {
                    if (active) onlineEpoch++;
                  },
                  Math.max(1, result.retryAfter || 60) * 1000,
                );
                return;
              }
              onlineProgress = {
                ...onlineProgress,
                checked: onlineProgress.checked + 1,
                available: onlineProgress.available + Number(result.state === 'available'),
                skipped: onlineProgress.skipped + Number(result.state === 'mismatch'),
              };
            },
            (id, e) => {
              const message = String(e);
              onlineReplays = new Map(onlineReplays).set(id, {
                id,
                state: 'error',
                message,
                retryAfter: null,
              });
              onlineProgress = {
                ...onlineProgress,
                checked: onlineProgress.checked + 1,
                skipped: onlineProgress.skipped + 1,
              };
            },
          );
        } finally {
          if (active) onlineChecking = false;
        }
      })
      .catch((e) => {
        if (active) replayWarning = String(e);
      });
    return () => {
      active = false;
      if (retry) clearTimeout(retry);
    };
  });
  onMount(() => {
    const background = new Worker(new URL('./lib/search.worker.ts', import.meta.url), {
      type: 'module',
    });
    worker = background;
    background.onmessage = (event: MessageEvent<QueryResult>) => {
      if (event.data.requestId !== requestId) return;
      result = event.data;
      searching = false;
    };
    background.onerror = () => {
      error = 'The search worker stopped. Reopen the application to retry.';
      searching = false;
    };
    return () => background.terminate();
  });
  $effect(() => {
    if (scores === patchedScores) {
      patchedScores = null;
      return;
    }
    worker?.postMessage({ type: 'load', scores });
  });
  $effect(() => {
    const background = worker;
    const query = {
      requestId: ++requestId,
      filters: $state.snapshot(filters),
      page,
      size,
      metric,
      bucket,
      unique,
      progression: section === 'trends',
    };
    void scores;
    if (!background) return;
    searching = true;
    // Coalesce typing; stale in-flight replies cannot replace newer results.
    const timer = setTimeout(
      () => background.postMessage({ type: 'query', query }),
      filters.search ? 120 : 0,
    );
    return () => clearTimeout(timer);
  });
  let players = $derived(playerChoices(scores, library?.playerAccounts));
  const checkedAccounts = new Set<number>();
  async function refreshIdentity(id: number, report = false) {
    if (!isDesktop || sample || id <= 1) return;
    checkedAccounts.add(id);
    try {
      const identity = await refreshPlayerIdentity(id);
      if (sample) return;
      const ids = new Map(identity.assignments);
      const accounts = new Map(identity.accounts.map((a) => [a.id, a]));
      const selectedId = filters.player.startsWith('id:') ? Number(filters.player.slice(3)) : 0;
      const reassigned = new Set(
        scores
          .filter((s) => s.playerId === selectedId)
          .map((s) => (ids.has(s.id) ? ids.get(s.id) : s.playerId)),
      );
      scores = scores.map((s) => {
        const playerId = ids.has(s.id) ? (ids.get(s.id) ?? undefined) : s.playerId;
        return {
          ...s,
          playerId,
          playerAliases: playerId ? accounts.get(playerId)?.aliases || [s.player] : [s.player],
        };
      });
      if (selectedId && !scores.some((s) => s.playerId === selectedId) && reassigned.size === 1) {
        const replacement = [...reassigned][0];
        if (replacement) filters.player = `id:${replacement}`;
      }
      if (library) library = { ...library, playerAccounts: identity.accounts };
      if (report) message = 'Player names refreshed from osu! profiles.';
    } catch (e) {
      if (report) error = String(e);
    }
  }
  $effect(() => {
    const id = filters.player.startsWith('id:') ? Number(filters.player.slice(3)) : 0;
    if (!busy && id > 1 && !checkedAccounts.has(id)) void refreshIdentity(id);
  });
  let selectedPlayerLabel = $derived(
    players.find((p) => p.value === filters.player)?.label || filters.player,
  );
  $effect(() => {
    if (filters.player && !players.some((p) => p.value === filters.player)) {
      const matching = players.filter((p) =>
        p.names.some((n) => n.toLowerCase() === filters.player.toLowerCase()),
      );
      if (matching.length === 1) filters.player = matching[0].value;
    }
  });
  let mods = $derived([...new Set(scores.map((s) => scoreMods(s)))].sort());
  let currentPreset = $derived(presets.find((p) => p.id === filters.preset) || presets[0]);
  let trendPoints = $derived(result.points);
  let rated = $derived(scores.filter((s) => s.stars !== null).length);
  let filteredDates = $derived(result.years);
  let summary = $derived([
    { label: 'Imported scores', value: scores.length.toLocaleString() },
    {
      label: 'Beatmaps in view',
      value: result.maps.toLocaleString(),
    },
    { label: 'Full combos in view', value: result.fcs.toLocaleString() },
    { label: 'Years in view', value: new Set(filteredDates).size.toLocaleString() },
  ]);
  $effect(() => {
    localStorage.setItem('sx-filters', JSON.stringify(filters));
    page = 0;
  });
  $effect(() => {
    localStorage.setItem('sx-view', JSON.stringify(view));
  });
  $effect(() => {
    void section;
    window.scrollTo(0, 0);
  });
  $effect(() => {
    localStorage.setItem('sx-theme', theme);
    document.documentElement.dataset.theme = theme;
  });
  $effect(() => {
    localStorage.setItem('sx-metric', JSON.stringify(metric));
  });
  onMount(async () => {
    if (isDesktop) {
      busy = true;
      try {
        library = await loadLibrary();
        scores = library.scores;
        calculator = library.calculator || 'rosu';
        if (calculator === 'lazer')
          void startCalculations().catch((e) => {
            error = String(e);
          });
        if (library.gameSources) gameSources = library.gameSources;
        defaultPlayer();
        const own = library.playerAccounts?.find((a) =>
          [a.name, ...a.aliases].some(
            (n) => n.toLowerCase() === library?.defaultPlayer?.toLowerCase(),
          ),
        );
        if (own && !checkedAccounts.has(own.id)) void refreshIdentity(own.id);
      } catch (e) {
        error = String(e);
      } finally {
        busy = false;
      }
    }
  });
  const number = (n: number | null, digits = 0) =>
    n === null
      ? '—'
      : n.toLocaleString(undefined, {
          minimumFractionDigits: digits,
          maximumFractionDigits: digits,
        });
  function demo() {
    scores = demoScores();
    sample = true;
    filters = { ...defaultFilters, player: 'Sample player' };
    message = 'Sample library loaded. Values are illustrative; your files have not been imported.';
  }
  function defaultPlayer() {
    if (
      !localStorage.getItem('sx-player-selected') &&
      library?.defaultPlayer &&
      scores.some((s) => s.player === library!.defaultPlayer)
    )
      filters.player = library.defaultPlayer;
  }
  async function playSong(score: Score) {
    const request = ++songRequest;
    loadingSong = true;
    songMessage = '';
    try {
      const url = await withTimeout(
        audioTrack(score.id),
        8000,
        'Audio lookup took too long. Try another score or rescan your libraries.',
      );
      if (request !== songRequest) return;
      if (!url) {
        songMessage = 'No installed audio found for this map. Rescan after adding its beatmap.';
        return;
      }
      track = { id: score.id, url, title: score.title, artist: score.artist };
      songMessage = '';
      selected = null;
    } catch (e) {
      if (request === songRequest) songMessage = String(e);
    } finally {
      if (request === songRequest) loadingSong = false;
    }
  }
  async function chooseReplayFolder(): Promise<string | null> {
    const { open } = await import('@tauri-apps/plugin-dialog');
    const folder = await open({
      directory: true,
      multiple: false,
      title: 'Choose where to save exported replays',
      ...(replayFolder ? { defaultPath: replayFolder } : {}),
    });
    if (typeof folder !== 'string') return null;
    replayFolder = folder;
    return folder;
  }
  async function downloadReplay(score: Score) {
    if (exporting) return;
    exporting = score.id;
    exportFeedback = null;
    try {
      const folder = replayFolder || (await chooseReplayFolder());
      if (!folder) return;
      const destination = await exportReplay(score.id, folder, !replayIds.has(score.id));
      replayIds = new Set(replayIds).add(score.id);
      message = `Replay saved to ${destination}`;
      exportFeedback = { id: score.id, text: message };
      error = '';
    } catch (e) {
      error = String(e);
      exportFeedback = { id: score.id, text: error };
    } finally {
      exporting = '';
    }
  }
  let apiSaved = $state(false),
    savedApiClientId = $state('');
  function applyApiState(state: ApiConnection) {
    onlineConnected = state.connected;
    apiSaved = state.saved;
    savedApiClientId = state.clientId;
    apiClientId = state.clientId;
    apiClientSecret = '';
    onlineWarning = state.message;
    if (state.connected) onlineEpoch++;
  }
  onMount(() => {
    if (!isDesktop) return;
    onlineConnecting = true;
    void restoreApiConnection()
      .then(applyApiState)
      .catch((e) => (onlineWarning = String(e)))
      .finally(() => (onlineConnecting = false));
  });
  async function showApiSettings() {
    importing = false;
    section = 'settings';
    await tick();
    document
      .getElementById('osu-api-settings')
      ?.scrollIntoView({ behavior: 'smooth', block: 'start' });
    document.getElementById('osu-api-client-id')?.focus({ preventScroll: true });
  }
  async function visitApiRegistration(event: MouseEvent) {
    if (!isDesktop) return;
    event.preventDefault();
    try {
      await openApiRegistration();
    } catch (e) {
      onlineWarning = String(e);
    }
  }
  async function connectReplays() {
    onlineConnecting = true;
    onlineWarning = '';
    try {
      applyApiState(await connectOnlineReplays(apiClientId.trim(), apiClientSecret.trim()));
    } catch (e) {
      onlineWarning = String(e);
    } finally {
      onlineConnecting = false;
    }
  }
  async function disconnectReplays() {
    onlineConnecting = true;
    try {
      applyApiState(await disconnectOnlineReplays());
      onlineChecking = false;
      onlineReplays = new Map();
    } catch (e) {
      onlineWarning = String(e);
    } finally {
      onlineConnecting = false;
    }
  }
  async function forgetApi() {
    onlineConnecting = true;
    try {
      applyApiState(await forgetApiConnection());
      onlineReplays = new Map();
    } catch (e) {
      onlineWarning = String(e);
    } finally {
      onlineConnecting = false;
    }
  }
  async function changeCalculator(value: string) {
    selectingCalculator = true;
    try {
      library = await selectCalculator(value);
      calculator = library.calculator || value;
      scores = library.scores;
    } catch (e) {
      error = String(e);
    } finally {
      selectingCalculator = false;
    }
  }
  async function linkPlayerNames() {
    try {
      library = await savePlayerAccount({
        id: Number(identityId),
        name: identityName,
        aliases: identityAliases
          .split(/[,\n]/)
          .map((v) => v.trim())
          .filter(Boolean),
      });
      scores = library.scores;
      filters.player = `id:${Number(identityId)}`;
      message =
        'Player names linked. Score exploration and progression now include this account’s history.';
      error = '';
    } catch (e) {
      error = String(e);
    }
  }
  async function chooseDownloadSongs() {
    if (!isDesktop) return;
    const { open } = await import('@tauri-apps/plugin-dialog');
    const folder = await open({
      directory: true,
      multiple: false,
      title: 'Choose a stable Songs folder for downloaded beatmaps',
    });
    if (typeof folder === 'string') downloadSongs = folder;
  }
  async function beginDownloads() {
    try {
      if (!downloadSongs.trim()) {
        error = 'Choose a Songs folder for downloaded beatmaps first.';
        importing = true;
        return;
      }
      await startDownloads({
        songs: downloadSongs,
        player: filters.player || null,
      });
      queue = {
        running: true,
        phase: 'preparing',
        message: 'Preparing missing beatmap downloads…',
        total: 0,
        completed: 0,
        unavailable: 0,
        pending: 0,
        bytes: 0,
        totalBytes: null,
        installedSets: 0,
        updates: [],
        issues: [],
      };
      importing = false;
      error = '';
    } catch (e) {
      error = String(e);
    }
  }
  async function pauseQueue() {
    try {
      await pauseDownloads();
    } catch (e) {
      error = String(e);
    }
  }
  function addSource(kind: 'stable' | 'lazer') {
    gameSources.push({ kind, root: '', songs: null });
    importing = true;
  }
  $effect(() => {
    localStorage.setItem('sx-game-sources', JSON.stringify(gameSources));
  });
  async function persistSources() {
    if (!isDesktop) return;
    gameSources = await saveSources($state.snapshot(gameSources).filter((s) => s.root.trim()));
  }
  async function saveFolders() {
    try {
      await persistSources();
      importing = false;
      error = '';
      message = 'Game folders saved. Previously imported scores are kept when a folder is removed.';
    } catch (e) {
      error = String(e);
    }
  }
  async function choose(index: number, which: 'root' | 'songs') {
    if (!isDesktop) return;
    const { open } = await import('@tauri-apps/plugin-dialog');
    const path = await open({
      directory: true,
      multiple: false,
      title:
        which === 'songs'
          ? 'Choose the Songs folder for this install'
          : `Choose your ${gameSources[index].kind} folder`,
    });
    if (typeof path === 'string') {
      gameSources[index][which] = path;
    }
  }
  async function scan(force = false, target: 'both' | 'stable' | 'lazer' = 'both') {
    if (queue?.running) {
      error = 'Pause beatmap downloads before starting another import.';
      return;
    }
    busy = true;
    error = '';
    importing = false;
    message = 'Reading changed score sources and calculating unfamiliar plays…';
    const problems: string[] = [];
    let latest: Library | null = null;
    let completed = 0,
      calculated = 0;
    try {
      await persistSources();
      const selectedSources = $state
        .snapshot(gameSources)
        .filter((s) => target === 'both' || s.kind === target);
      for (const source of selectedSources) {
        message = `Scanning ${completed + 1} of ${selectedSources.length}: ${source.root}`;
        try {
          latest =
            source.kind === 'stable'
              ? await importStable(source.root, source.songs, force)
              : await importLazer(source.root, force);
          completed++;
          calculated += latest.calculated;
        } catch (e) {
          problems.push(`${source.kind} (${source.root}): ${String(e)}`);
        }
      }
    } catch (e) {
      problems.push(String(e));
    }
    if (latest) {
      library = latest;
      scores = latest.scores;
      defaultPlayer();
      sample = false;
      message = `Library ready · ${completed} folders checked · ${scores.length.toLocaleString()} combined scores · ${calculated.toLocaleString()} calculations saved`;
    } else message = '';
    error = problems.join(' ');
    busy = false;
    if (latest && calculator === 'lazer')
      void startCalculations().catch((e) => {
        error = String(e);
      });
    if (latest && automaticDownloads) await beginDownloads();
  }
  function preset(id: string) {
    filters.preset = id;
    section = 'explore';
  }
  function goPage(target: number) {
    page = clampPage(target, result.total, size);
  }
</script>

<svelte:window
  onkeydown={(e) => {
    if (e.key === 'Escape') {
      selected = null;
      importing = false;
    }
  }}
/>
<div class="app-shell">
  <aside class="sidebar">
    <a class="brand" href="#explore" onclick={() => preset('all')}
      ><div class="brand-mark"><span></span></div>
      <div>score explorer<small>YOUR PLAYS, REDISCOVERED</small></div></a
    >
    <div class="nav-caption">LIBRARY</div>
    <button class:active={section === 'explore'} class="nav-item" onclick={() => preset('all')}
      ><Disc3 size={18} />Explore scores<span class="nav-count"
        >{scores.length ? number(scores.length) : '—'}</span
      ></button
    >
    <button
      class:active={section === 'trends'}
      class="nav-item"
      onclick={() => (section = 'trends')}><TrendingUp size={18} />Progression</button
    >
    <div class="nav-caption discoveries">DISCOVER</div>
    {#each presets.slice(1) as p}<button
        class="nav-item secondary"
        class:selected-preset={section === 'explore' && filters.preset === p.id}
        onclick={() => preset(p.id)}><p.icon size={16} />{p.label}</button
      >{/each}
    <div class="sidebar-spacer"></div>
    <MusicPlayer {track} />
    <div class="local-note">
      <span class="status-dot"></span>{sample
        ? 'Sample library'
        : scores.length
          ? 'Local library ready'
          : 'Your library, on your device'}<small
        >{sample ? 'Illustrative scores only' : 'No uploads. Original files stay untouched.'}</small
      >
    </div>
    <button
      class="nav-item"
      class:active={section === 'settings'}
      onclick={() => (section = 'settings')}><Settings size={17} />Library & settings</button
    >
    <div class="sidebar-footer">
      <span>EARLY BUILD <b>{version}</b></span><button
        class="icon-button"
        aria-label="Toggle light or dark theme"
        onclick={() => (theme = theme === 'dark' ? 'light' : 'dark')}
        >{#if theme === 'dark'}<Sun size={17} />{:else}<Moon size={17} />{/if}</button
      >
    </div>
  </aside>

  <main>
    <header class="topbar">
      <span class="breadcrumb"
        >Your library <ChevronRight size={14} /><strong
          >{section === 'explore'
            ? 'Explore scores'
            : section === 'trends'
              ? 'Progression'
              : 'Settings'}</strong
        ></span
      >
      <div class="topbar-actions">
        {#if sample}<span class="sample-badge">SAMPLE DATA</span>{/if}<span class="local-label"
          ><span class="status-dot"></span> LOCAL FIRST</span
        ><button class="button primary small" disabled={busy} onclick={() => (importing = true)}
          ><FolderOpen size={15} />{busy ? 'Importing…' : 'Import scores'}</button
        >
      </div>
    </header>
    <div class="content">
      {#if error}<div class="notice error" role="alert">
          <Info size={16} /><span>{error}</span><button
            class="icon-button"
            aria-label="Dismiss error"
            onclick={() => (error = '')}><X size={14} /></button
          >
        </div>{/if}
      {#if message}<div class="notice" role="status">
          <Info size={16} /><span>{message}</span><button
            class="icon-button"
            aria-label="Dismiss notification"
            onclick={() => (message = '')}><X size={14} /></button
          >
        </div>{/if}
      {#if queue?.running}<div class="notice download-notice" role="status">
          <Download size={16} /><span>{queue.message}</span><button
            class="button small"
            onclick={pauseQueue}>Pause downloads</button
          >
        </div>{/if}
      {#if calculationProgress?.running && calculator === 'lazer'}<div class="notice" role="status">
          <RefreshCw size={16} /><span
            >Updating lazer PP · {calculationProgress.processed.toLocaleString()} / {calculationProgress.total.toLocaleString()}
            scores</span
          >
        </div>{/if}
      {#if section === 'explore'}
        <div class="page-heading">
          <div>
            <div class="eyebrow">EVERY PLAY HAS A STORY</div>
            <h1>Explore your scores<span class="pink-dot">.</span></h1>
            <p>The records. The almosts. The ones worth another try.</p>
          </div>
          <div class="heading-decoration"><Disc3 size={48} strokeWidth={1} /></div>
        </div>
        <div class="summary-grid">
          {#each summary as item, i}<div class="summary">
              <span>{item.label}</span><strong>{item.value}</strong><span class="summary-symbol"
                >{['◉', '◇', '✧', '↗'][i]}</span
              >
            </div>{/each}
        </div>
        <div class="preset-strip">
          {#each presets as p}<button
              class:chosen={filters.preset === p.id}
              onclick={() => (filters.preset = p.id)}
              ><p.icon size={15} />{p.label}{#if p.id === 'all'}<span>{number(scores.length)}</span
                >{/if}</button
            >{/each}
        </div>
        <fieldset class="source-toggles">
          <legend>Score sources</legend><label
            ><input type="checkbox" bind:checked={filters.includeStable} />stable</label
          ><label><input type="checkbox" bind:checked={filters.includeLazer} />lazer</label>
        </fieldset>
        <div class="filter-bar">
          <label class="search-field"
            ><Search size={18} /><input
              aria-label="Search scores"
              placeholder="Search songs, artists, difficulties, mappers…"
              bind:value={filters.search}
            />{#if filters.search}<button
                class="icon-button"
                aria-label="Clear search"
                onclick={() => (filters.search = '')}><X size={14} /></button
              >{/if}</label
          ><select
            class="player-filter"
            aria-label="Player"
            bind:value={filters.player}
            onchange={() => localStorage.setItem('sx-player-selected', 'yes')}
            ><option value="">All players</option>{#each players as player}<option
                value={player.value}>{player.label}</option
              >{/each}</select
          ><select class="mode-filter" aria-label="Gamemode" bind:value={filters.mode}
            ><option value="0">osu!standard</option><option value="1">osu!taiko</option><option
              value="2">osu!catch</option
            ><option value="3">osu!mania</option><option value="">All modes</option></select
          ><button class="button" class:enabled={advanced} onclick={() => (advanced = !advanced)}
            ><SlidersHorizontal size={16} />Filters</button
          ><label class="sort-field"
            ><span>Sort by</span><select aria-label="Sort scores" bind:value={filters.sort}
              ><option value="pp">Highest PP</option><option value="stars">Highest stars</option
              ><option value="fcPp">PP if FC</option><option value="gain">FC PP gain</option><option
                value="accuracy">Accuracy</option
              ><option value="date">Most recent</option></select
            ></label
          >
          <div class="view-switch">
            <button
              class:chosen={view === 'cards'}
              aria-label="Card view"
              onclick={() => (view = 'cards')}><LayoutGrid size={17} /></button
            ><button
              class:chosen={view === 'table'}
              aria-label="Table view"
              onclick={() => (view = 'table')}><List size={18} /></button
            >
          </div>
        </div>
        {#if advanced}<div class="advanced-filters">
            <label
              title="Show 1 through N non-perfect judgments: 100s, 50s and misses in standard. Excludes SSs; FCs with non-300s remain visible."
              >Max non-300s<input
                id="non300"
                type="number"
                min="1"
                step="1"
                placeholder="Any"
                value={filters.maxNon300 ?? ''}
                oninput={(e) =>
                  (filters.maxNon300 =
                    e.currentTarget.value === ''
                      ? null
                      : Math.max(0, Math.floor(Number(e.currentTarget.value))))}
              /></label
            >
            <label
              >Minimum stars<input
                type="number"
                min="0"
                step="0.1"
                bind:value={filters.minStars}
              /></label
            ><label
              >Maximum stars<input
                type="number"
                min="0"
                step="0.1"
                placeholder="Any"
                value={filters.maxStars ?? ''}
                oninput={(e) =>
                  (filters.maxStars =
                    e.currentTarget.value === ''
                      ? null
                      : Math.max(0, Number(e.currentTarget.value)))}
              /></label
            ><label title="Show scores with 1 through N misses. Zero-miss plays are excluded."
              >Maximum misses<input
                type="number"
                min="1"
                step="1"
                placeholder="Any"
                value={filters.maxMisses ?? ''}
                oninput={(e) =>
                  (filters.maxMisses =
                    e.currentTarget.value === ''
                      ? null
                      : Math.max(0, Math.floor(Number(e.currentTarget.value))))}
              /></label
            ><label
              >Mods<select bind:value={filters.mod}
                ><option value="">Any combination</option>{#each mods as m}<option>{m}</option
                  >{/each}</select
              ></label
            ><label>From<input type="date" bind:value={filters.from} /></label><label
              >Through<input type="date" bind:value={filters.to} /></label
            ><button class="text-button" onclick={() => (filters = { ...defaultFilters })}
              >Reset all</button
            >
          </div>{/if}
        {#if filters.preset === 'comboChoke'}<div class="insight-note">
            <Info size={17} />
            <p>
              <strong>Combo choke:</strong> no recorded misses, but the achieved combo is below the map's
              maximum. In standard this can happen after a slider break. This identifies the result, not
              the exact location or cause of the break.
            </p>
          </div>{:else if filters.preset === 'ssChoke'}<div class="insight-note">
            <Info size={17} />
            <p>
              SS chokes have 1–{filters.maxNon300 ?? 1} non-perfect judgments and are not SS plays. Change
              <strong>Max non-300s</strong> to include more: for standard, each 100, 50, or miss counts
              as one.
            </p>
          </div>{/if}
        <div class="results-header">
          <span
            ><strong>{number(result.total)}</strong>
            {currentPreset.label.toLowerCase()}{#if searching}
              · Updating results…{/if}{#if filters.search}
              matching “{filters.search}”{/if}</span
          ><span
            >{#if scores.length}{filteredDates[0] ?? '—'} — {filteredDates.at(-1) ?? '—'}<span
                class="divider">/</span
              >{sample
                ? 'Sample values'
                : `${rated.toLocaleString()} with calculated stars`}{:else}A little history. A lot
              to discover.{/if}</span
          >
        </div>
        {#if onlineEnabled && onlineConnected && onlineProgress.total > 0}<p
            class="online-replay-progress muted"
            role="status"
          >
            {onlineProgressLabel}
          </p>{/if}
        {#if busy && !scores.length}<div class="empty-state">
            <RefreshCw class="spin" size={30} />
            <h2>Building your local library</h2>
            <p>
              The first import reads your maps and calculates PP.<br />Next time, saved results will
              load directly.
            </p>
          </div>
        {:else if !scores.length}<div class="empty-state">
            <div class="empty-orbit"><Disc3 size={40} strokeWidth={1.3} /></div>
            <span class="eyebrow">WELCOME TO YOUR SCORE ARCHIVE</span>
            <h2>Your next discovery is already in your history.</h2>
            <p>
              Connect your osu! libraries to explore your records,<br />one-miss chokes, full
              combos, and everything in between.
            </p>
            <div class="empty-actions">
              <button class="button primary" onclick={() => (importing = true)}
                ><FolderOpen size={17} />Connect your library</button
              ><button class="button" onclick={demo}
                >Explore a sample library<ArrowUpRight size={16} /></button
              >
            </div>
            <small
              >{isDesktop
                ? 'Reads scores.db and osu!.db. Never changes your game files.'
                : 'Browser preview · native imports are available in the desktop app.'}</small
            >
          </div>
        {:else if !result.total && searching}<div class="empty-state">
            <RefreshCw class="spin" size={24} />
            <p>Finding your scores…</p>
          </div>
        {:else if !result.total}<div class="empty-state">
            <Search size={30} />
            <h2>No plays match these filters.</h2>
            <p>Try a wider date range or fewer filters.</p>
            <button class="button" onclick={() => (filters = { ...defaultFilters })}
              >Reset filters</button
            >
          </div>
        {:else if view === 'cards'}<div class="score-grid">
            {#each visible as score, i (score.id)}{@const hits = hitBreakdown(score)}
              <article class="score-card">
                <button class="card-open" onclick={() => (selected = score)}
                  ><div class="card-art">
                    <Artwork
                      id={score.id}
                      available={score.hasArtwork}
                      hue={210 + ((i * 37) % 130)}
                    />
                    <div class="card-shade"></div>
                    <div class="card-top">
                      <span class="stars"
                        ><Star size={11} fill="currentColor" />{number(score.stars, 2)}</span
                      ><span class="mod-badge">{scoreMods(score)}</span>
                    </div>
                    <div class="card-map">
                      <span>{score.artist || 'Unknown artist'}</span>
                      <h3>{score.title}</h3>
                      <p>{score.difficulty}</p>
                    </div>
                  </div>
                  <div class="card-body">
                    <div class="card-primary">
                      <span class="grade" class:fc={score.fc}
                        >{score.ss
                          ? 'SS'
                          : score.fc
                            ? 'FC'
                            : score.misses
                              ? `${score.misses}×`
                              : '—'}</span
                      >
                      <div class="card-pp">{number(score.pp)}<small>pp</small></div>
                      <div class="card-accuracy">
                        {number(score.accuracy, 2)}<small>% accuracy</small>
                      </div>
                    </div>
                    <div class="card-hit-row">
                      <span class="card-hits" title={hits.label} aria-label={hits.label}
                        >{hits.text}</span
                      >
                      {#if !score.fc && score.fcPp !== null}<span class="fc-estimate"
                          >{number(score.fcPp)} pp if FC</span
                        >{/if}
                    </div>
                    <div class="card-secondary">
                      <span
                        >{score.misses === 0
                          ? 'No misses'
                          : `${score.misses} miss${score.misses === 1 ? '' : 'es'}`}<span
                          class="middot">·</span
                        >{number(score.combo)} / {number(score.maxCombo)}× combo</span
                      ><time datetime={score.date}>{score.date.slice(0, 10)}</time>
                    </div>
                  </div></button
                >
                <div class="card-footer">
                  <span
                    >{(score.sources?.length ? score.sources : [score.source]).join(' + ')}<span
                      class="middot">·</span
                    >{score.player}</span
                  >{#if replayIds.has(score.id) || (onlineEnabled && onlineConnected && onlineReplays.get(score.id)?.state === 'available')}<button
                      class="replay-button"
                      disabled={!!exporting}
                      title={replayIds.has(score.id)
                        ? 'Export local replay (.osr)'
                        : 'Download online replay (.osr)'}
                      aria-label={`Export replay for ${score.title}`}
                      onclick={() => downloadReplay(score)}
                      ><Download size={13} />{exporting === score.id
                        ? 'Saving…'
                        : replayIds.has(score.id)
                          ? 'Replay'
                          : 'Online replay'}</button
                    >{:else}<ArrowUpRight size={14} />{/if}
                </div>
              </article>{/each}
          </div>
        {:else}<CopyTable
            title={`Scores · ${selectedPlayerLabel || 'All players'} · Page ${page + 1}`}
            disabled={searching}
            columns={[
              { label: 'Beatmap', maxWidth: 60 },
              { label: 'Mods', maxWidth: 24 },
              { label: 'Stars', right: true },
              { label: 'Accuracy', right: true },
              { label: 'Misses', right: true },
              { label: 'Combo', right: true },
              { label: 'PP', right: true },
              { label: 'PP if FC', right: true },
              { label: 'Date' },
            ]}
            rows={visible.map((s) => [
              `${s.artist} — ${s.title} [${s.difficulty}]${s.fc ? ' · FC' : ''}`,
              scoreMods(s),
              number(s.stars, 2),
              `${number(s.accuracy, 2)}%`,
              String(s.misses),
              `${number(s.combo)}×`,
              number(s.pp, 1),
              number(s.fcPp, 1),
              s.date.slice(0, 10),
            ])}
          />
          <div class="table-wrap">
            <table>
              <thead
                ><tr
                  ><th>Beatmap</th><th>Mods</th><th>Stars</th><th>Accuracy</th><th>Misses</th><th
                    >Combo</th
                  ><th>PP</th><th>PP if FC</th><th>Date</th></tr
                ></thead
              ><tbody
                >{#each visible as score}<tr
                    ><td
                      ><button class="table-map" onclick={() => (selected = score)}
                        ><strong>{score.artist} — {score.title}</strong><span
                          >{score.difficulty}{score.fc ? ' · FC' : ''}</span
                        ></button
                      ></td
                    ><td><span class="mod-badge">{scoreMods(score)}</span></td><td
                      >{number(score.stars, 2)}</td
                    ><td>{number(score.accuracy, 2)}%</td><td>{score.misses}</td><td
                      >{number(score.combo)}×</td
                    ><td class="numeric-accent">{number(score.pp, 1)}</td><td
                      >{number(score.fcPp, 1)}</td
                    ><td>{score.date.slice(0, 10)}</td></tr
                  >{/each}</tbody
              >
            </table>
          </div>{/if}
        {#if result.total > size}<div class="pagination">
            <span
              >Showing {page * size + 1}–{Math.min((page + 1) * size, result.total)} of {number(
                result.total,
              )}</span
            >
            <div>
              <button
                class="button small"
                disabled={searching || page === 0}
                onclick={() => goPage(0)}>First</button
              >
              <button
                class="button small"
                aria-label="Back 10 pages"
                disabled={searching || page === 0}
                onclick={() => goPage(page - 10)}>−10</button
              >
              <button
                class="button small"
                disabled={searching || page === 0}
                onclick={() => goPage(page - 1)}><ChevronLeft size={15} />Previous</button
              ><span>{page + 1} / {Math.ceil(result.total / size)}</span><button
                class="button small"
                disabled={searching || (page + 1) * size >= result.total}
                onclick={() => goPage(page + 1)}>Next<ChevronRight size={15} /></button
              >
              <button
                class="button small"
                aria-label="Forward 10 pages"
                disabled={searching || (page + 1) * size >= result.total}
                onclick={() => goPage(page + 10)}>+10</button
              >
              <button
                class="button small"
                disabled={searching || (page + 1) * size >= result.total}
                onclick={() => goPage(Math.ceil(result.total / size) - 1)}>Last</button
              >
            </div>
          </div>{/if}
      {:else if section === 'trends'}
        <div class="page-heading">
          <div>
            <div class="eyebrow">THE LONG GAME</div>
            <h1>Your progression<span class="pink-dot">.</span></h1>
            <p>See how your retained score history changes over time.</p>
          </div>
        </div>
        <fieldset class="source-toggles">
          <legend>Score sources</legend><label
            ><input type="checkbox" bind:checked={filters.includeStable} />stable</label
          ><label><input type="checkbox" bind:checked={filters.includeLazer} />lazer</label>
        </fieldset>
        <div class="analysis-controls">
          <label
            >Player<select
              bind:value={filters.player}
              onchange={() => localStorage.setItem('sx-player-selected', 'yes')}
              ><option value="">Choose one player</option>{#each players as player}<option
                  value={player.value}>{player.label}</option
                >{/each}</select
            ></label
          >
          <label
            >Gamemode<select bind:value={filters.mode}
              ><option value="0">osu!standard</option><option value="1">osu!taiko</option><option
                value="2">osu!catch</option
              ><option value="3">osu!mania</option><option value="">Choose one mode</option></select
            ></label
          ><label
            >Metric<select bind:value={metric}
              ><option value="stars">Average star rating</option><option value="pp"
                >Average calculated PP</option
              ><option value="accuracy">Average accuracy</option></select
            ></label
          ><label
            >Group by<select bind:value={bucket}
              ><option value="year">Year</option><option value="month">Month</option></select
            ></label
          ><label class="checkbox"
            ><input type="checkbox" bind:checked={unique} />Best result per map + mods in each
            period</label
          >
        </div>
        <div class="analysis-panel">
          <div class="panel-heading">
            <div>
              <h2>
                {metric === 'stars'
                  ? 'Average star rating'
                  : metric === 'pp'
                    ? 'Average PP'
                    : 'Average accuracy'}
              </h2>
              <p>Uses your current explorer filters · {number(result.total)} retained plays</p>
            </div>
            <span class="sample-badge"
              >{sample ? 'SAMPLE VALUES' : library?.engine || 'NO LIBRARY'}</span
            >
          </div>
          {#if trendPoints.some((p) => p.value !== null)}<TrendChart
              points={trendPoints}
              unit={metric === 'stars' ? '★' : metric === 'pp' ? 'pp' : '%'}
            />{:else}<div class="empty-state compact">
              <TrendingUp size={28} />
              <h2>
                {filters.mode === '' || !filters.player
                  ? 'Select a player and gamemode to see personal trends.'
                  : 'Build a picture of your progress.'}
              </h2>
              <p>Import scores with available calculations, or explore sample data.</p>
              {#if !scores.length}<button class="button" onclick={demo}>Load sample library</button
                >{/if}
            </div>{/if}
        </div>
        <div class="insight-note">
          <Info size={17} />
          <p>
            These are averages of <strong>retained imported scores</strong>, not a complete record
            of every attempt. Stars are recalculated with one engine version, so an old play is
            measured on the same basis as a new one. Missing periods stay empty; unavailable values
            are excluded and counted below.
          </p>
        </div>
        {#if trendPoints.length}<CopyTable
            title={`Average ${metric} · ${selectedPlayerLabel} · ${bucket}`}
            disabled={searching}
            columns={[
              { label: 'Period' },
              { label: `Average ${metric}`, right: true },
              { label: 'Included scores', right: true },
              { label: 'Values unavailable', right: true },
            ]}
            rows={trendPoints.map((p) => [
              p.period,
              number(p.value, 2),
              String(p.count),
              String(p.missing),
            ])}
          />
          <div class="table-wrap">
            <table>
              <thead
                ><tr
                  ><th>Period</th><th>Average {metric}</th><th>Included scores</th><th
                    >Values unavailable</th
                  ></tr
                ></thead
              ><tbody
                >{#each trendPoints as p}<tr
                    ><td>{p.period}</td><td class="numeric-accent">{number(p.value, 2)}</td><td
                      >{p.count}</td
                    ><td>{p.missing}</td></tr
                  >{/each}</tbody
              >
            </table>
          </div>{/if}
      {:else}
        <div class="page-heading">
          <div>
            <div class="eyebrow">MAKE YOURSELF AT HOME</div>
            <h1>Library & settings<span class="pink-dot">.</span></h1>
            <p>Local data, remembered preferences, and transparent calculations.</p>
          </div>
        </div>
        <div class="settings-grid">
          <section class="settings-panel">
            <FolderOpen size={22} />
            <h2>Sources & exports</h2>
            <p>Read scores from stable and lazer, and choose where to save replay exports.</p>
            {#each gameSources.filter((s) => s.root) as source}<div class="source-row">
                <span>{source.kind === 'stable' ? 'Stable' : 'Lazer'}</span><code
                  >{source.root}</code
                ><button class="text-button" onclick={() => (importing = true)}>Configure</button>
              </div>{/each}
            <div class="setting-actions">
              <button class="button small" onclick={() => addSource('stable')}
                >Add stable folder</button
              ><button class="button small" onclick={() => addSource('lazer')}
                >Add lazer folder</button
              >
            </div>
            <div class="source-row replay-folder-row">
              <label for="replay-folder">Replay exports</label><input
                id="replay-folder"
                aria-label="Default export folder"
                bind:value={replayFolder}
                placeholder="Ask on first export"
              />
              <button class="text-button" disabled={!isDesktop} onclick={chooseReplayFolder}
                >Choose folder</button
              >
              <button class="text-button" onclick={() => (replayFolder = '')}>Clear default</button>
            </div>
            {#if replayWarning}<p class="muted">{replayWarning}</p>{/if}
            <details class="online-replay-settings">
              <summary>Online replays</summary>
              <label class="download-toggle"
                ><input type="checkbox" bind:checked={onlineEnabled} />Check online when a local
                replay is missing</label
              >
              <p class="muted">
                Checks visible scores in the background, including imported friends’ scores.
                Available replays download to your replay export folder. Some scores have no
                retained online replay or online score ID.
              </p>
              <a
                href="#osu-api-settings"
                onclick={(e) => {
                  e.preventDefault();
                  void showApiSettings();
                }}>Connect API</a
              >
              {#if onlineConnected}<button
                  class="text-button"
                  onclick={() => {
                    onlineWarning = '';
                    onlineEpoch++;
                  }}>Retry online checks</button
                >{/if}
              {#if onlineProgress.total > 0}<p class="muted" role="status">
                  {onlineProgressLabel}
                </p>{/if}
            </details>
            <DownloadPanel
              bind:automatic={automaticDownloads}
              bind:songs={downloadSongs}
              connectApi={showApiSettings}
              folders={downloadFolders}
              player={selectedPlayerLabel}
              status={queue}
              disabled={busy || sample}
              desktop={isDesktop}
              choose={chooseDownloadSongs}
              start={beginDownloads}
              pause={pauseQueue}
            />
            <div class="setting-actions">
              <button class="button" disabled={busy || !isDesktop} onclick={() => scan()}
                ><RefreshCw size={16} />Rescan all folders</button
              ><button
                class="text-button"
                disabled={busy || !isDesktop}
                onclick={() => scan(false, 'lazer')}>Rescan lazer only</button
              ><button class="text-button" disabled={busy || !isDesktop} onclick={() => scan(true)}
                >Recheck all map files</button
              >
            </div>
          </section>
          <section class="settings-panel">
            <Sun size={22} />
            <h2>Appearance & session</h2>
            <p>
              Theme, filters, sort order, and card/table preference are remembered on this device.
            </p>
            <div class="theme-grid">
              {#each themes as choice}<button
                  class:chosen={theme === choice.id}
                  class="theme-choice"
                  onclick={() => (theme = choice.id)}
                  ><span class="theme-swatches"
                    >{#each choice.colors as color}<i style={`background:${color}`}
                      ></i>{/each}</span
                  >{choice.name}</button
                >{/each}
            </div>
            <p class="theme-credit">
              Additional palettes inspired by <a
                href="https://github.com/monkeytypegame/monkeytype/blob/master/frontend/src/ts/constants/themes.ts"
                target="_blank"
                rel="noreferrer">Monkeytype themes</a
              >.
            </p>
            <button
              class="text-button"
              onclick={() => {
                filters = { ...defaultFilters };
                view = 'cards';
              }}>Reset browser preferences</button
            >
          </section>
          <section class="settings-panel" id="osu-api-settings">
            <Settings size={22} />
            <h2>osu! API connection</h2>
            <p>
              One connection for online replays and missing beatmap lookup. Your friends do not need
              to sign in.
            </p>
            <ol class="api-steps">
              <li>
                <a
                  href="https://osu.ppy.sh/home/account/edit#new-oauth-application"
                  target="_blank"
                  rel="noreferrer"
                  onclick={visitApiRegistration}>Open your osu! account settings</a
                > and sign in.
              </li>
              <li>
                Create an OAuth application named <strong>osu! Score Explorer</strong>. Leave the
                callback URL empty.
              </li>
              <li>
                Copy its Client ID and Client secret below, then connect to verify and save them.
              </li>
            </ol>
            <div class="download-api-fields">
              <label class="form-label"
                >Client ID<input
                  id="osu-api-client-id"
                  aria-label="osu API client ID"
                  bind:value={apiClientId}
                  inputmode="numeric"
                  autocomplete="off"
                  disabled={onlineConnecting}
                /></label
              >
              <label class="form-label"
                >Client secret<input
                  aria-label="osu API client secret"
                  type="password"
                  bind:value={apiClientSecret}
                  placeholder={apiSaved && apiClientId.trim() === savedApiClientId
                    ? 'Saved client secret'
                    : 'Paste your client secret'}
                  autocomplete="off"
                  disabled={onlineConnecting}
                /></label
              >
            </div>
            <div class="setting-actions">
              <button
                class="button"
                disabled={!isDesktop ||
                  onlineConnecting ||
                  !apiClientId.trim() ||
                  (!apiClientSecret.trim() &&
                    !(apiSaved && apiClientId.trim() === savedApiClientId))}
                onclick={connectReplays}
                >{onlineConnecting
                  ? 'Connecting…'
                  : onlineConnected
                    ? 'Reconnect API'
                    : 'Connect osu! API'}</button
              >
              {#if onlineConnected}<button
                  class="button"
                  disabled={onlineConnecting}
                  onclick={disconnectReplays}>Disconnect</button
                >{/if}
              {#if apiSaved}<button
                  class="text-button"
                  disabled={onlineConnecting}
                  onclick={forgetApi}>Forget saved connection</button
                >{/if}
            </div>
            <p role="status">
              {onlineConnected
                ? 'Connected for this session.'
                : 'Not connected. Local features work without an API connection.'}
            </p>
            <p class="muted">
              {apiSaved
                ? onlineConnected
                  ? 'Details saved securely for this Windows account. Reconnects automatically when you reopen the app, including after an update.'
                  : 'Saved details are available. Connect to enable automatic reconnection.'
                : 'After a successful connection, details are encrypted and saved for this Windows account across app versions.'}
            </p>
            {#if onlineWarning}<p role="status">{onlineWarning}</p>{/if}
          </section>
          <section class="settings-panel">
            <Database size={22} />
            <h2>Library & calculations</h2>
            <label class="form-label"
              >PP calculator
              <select
                aria-label="PP calculator"
                value={calculator}
                disabled={!isDesktop || selectingCalculator}
                onchange={(e) => changeCalculator(e.currentTarget.value)}
              >
                <option value="lazer">Lazer 2026 rules · Rust port (default)</option>
                <option value="rosu">rosu-pp 4.0.1 · previous rules</option>
              </select>
            </label>
            <p>
              Each calculator keeps its own results. Switching preserves your saved rosu PP. Missing
              lazer results are calculated in the background from exact installed maps.
            </p>
            {#if calculationProgress?.message && calculator === 'lazer'}<p
                class="muted"
                role="status"
              >
                {calculationProgress.message}
              </p>{/if}
            <dl>
              <dt>Engine</dt>
              <dd>{library?.engine || 'rosu-pp 4.0.1'}</dd>
              <dt>Last import</dt>
              <dd>
                {library?.importedAt
                  ? new Date(library.importedAt).toLocaleString()
                  : 'Not imported'}
              </dd>
              <dt>Artwork policy</dt>
              <dd>Read from Songs or lazer’s file store. No copied backgrounds.</dd>
            </dl>
            <details class="account-links">
              <summary>Player names & account IDs</summary>
              <p>
                Names are matched without regard to capitalization. Public profile history helps
                identify former names and reused usernames. Ambiguous scores stay separate. Original
                score names stay intact.
              </p>
              {#if library?.playerAccounts?.length}<label class="form-label"
                  >Known accounts<select
                    aria-label="Known player accounts"
                    onchange={(e) => {
                      const a = library?.playerAccounts?.find(
                        (a) => String(a.id) === e.currentTarget.value,
                      );
                      if (a) {
                        identityId = String(a.id);
                        identityName = a.name;
                        identityAliases = a.aliases.join(', ');
                      }
                    }}
                    ><option value="">Choose an account…</option
                    >{#each library.playerAccounts as account}<option value={account.id}
                        >{account.name} · {account.id}</option
                      >{/each}</select
                  ></label
                >{/if}
              <label class="form-label"
                >Account ID<input
                  aria-label="Player account ID"
                  inputmode="numeric"
                  bind:value={identityId}
                /></label
              >
              <label class="form-label"
                >Display name<input
                  aria-label="Player account display name"
                  bind:value={identityName}
                /></label
              >
              <label class="form-label"
                >Historical names, separated by commas<input
                  aria-label="Historical player names"
                  bind:value={identityAliases}
                /></label
              >
              <button
                class="button"
                disabled={!isDesktop || !identityId || !identityName.trim()}
                onclick={linkPlayerNames}>Link player names</button
              >
              <button
                class="button"
                disabled={!isDesktop || !identityId}
                onclick={() => refreshIdentity(Number(identityId), true)}
                >Refresh names from osu!</button
              >
            </details>
            <dl>
              <dt>Stable records</dt>
              <dd>{number(library?.stableScores ?? 0)}</dd>
              <dt>Lazer records</dt>
              <dd>{number(library?.lazerScores ?? 0)}</dd>
              <dt>Native lazer plays</dt>
              <dd>{number(library?.nativeLazerScores ?? 0)}</dd>
              <dt>Matched duplicates</dt>
              <dd>{number(library?.mergedDuplicates ?? 0)}</dd>
            </dl>
            <p>
              Stable and lazer import, reversible duplicate links, cached calculations, score
              exploration, and progression are connected. Recover missing beatmaps through no-video
              downloads, with catalog IDs or osu! API lookup and automatic score updates.
            </p>
            <button
              class="button"
              onclick={() => {
                demo();
                section = 'explore';
              }}>Explore sample data<ArrowUpRight size={16} /></button
            >{#if sample && isDesktop}<button
                class="text-button"
                onclick={async () => {
                  library = await loadLibrary();
                  scores = library.scores;
                  if (library.gameSources) gameSources = library.gameSources;
                  sample = false;
                }}>Return to imported library</button
              >{/if}
          </section>
        </div>
        {#if library?.warnings.length}<div class="settings-panel">
            <h2>Import notices</h2>
            {#each library.warnings as warning}<p>{warning}</p>{/each}
          </div>{/if}
      {/if}
      <footer class="content-footer">
        <span>Built for the scores you almost forgot.</span><span
          >Score Explorer <span class="middot">/</span>
          {sample ? 'Sample mode' : isDesktop ? 'Desktop preview' : 'Browser preview'}</span
        >
      </footer>
    </div>
  </main>
</div>

{#if importing}<div class="modal-backdrop" role="presentation">
    <dialog
      use:showDialog
      oncancel={() => (importing = false)}
      class="modal"
      aria-modal="true"
      aria-labelledby="import-title"
      tabindex="-1"
    >
      <div class="modal-heading">
        <div>
          <span class="eyebrow">CONNECT YOUR HISTORY</span>
          <h2 id="import-title">Import your score libraries</h2>
        </div>
        <button
          class="icon-button"
          aria-label="Close import dialog"
          onclick={() => (importing = false)}><X size={20} /></button
        >
      </div>
      <p>
        Add as many stable and lazer folders as you use. Each stable install can have its own Songs
        folder. Removing a path keeps its imported score history.
      </p>
      {#each gameSources as source, i}<div class="install-entry">
          <div class="install-heading">
            <label
              >Client<select aria-label={`Client for folder ${i + 1}`} bind:value={source.kind}
                ><option value="stable">stable</option><option value="lazer">lazer</option></select
              ></label
            ><button
              class="text-button"
              aria-label={`Remove folder ${i + 1}`}
              onclick={() => gameSources.splice(i, 1)}>Remove</button
            >
          </div>
          <label class="form-label"
            >Game folder {i + 1}
            <div class="path-input">
              <input
                aria-label={`Game folder ${i + 1}`}
                bind:value={source.root}
                placeholder={source.kind === 'stable'
                  ? 'Folder containing scores.db'
                  : 'Folder containing client.realm'}
              /><button
                class="icon-button"
                aria-label={`Choose game folder ${i + 1}`}
                onclick={() => choose(i, 'root')}
                disabled={!isDesktop}><FolderOpen size={18} /></button
              >
            </div></label
          >
          {#if source.kind === 'stable'}<label class="form-label"
              >Custom Songs folder <span class="muted">optional</span>
              <div class="path-input">
                <input
                  aria-label={`Songs folder ${i + 1}`}
                  bind:value={source.songs}
                  placeholder="Defaults to this install’s Songs folder"
                /><button
                  class="icon-button"
                  aria-label={`Choose Songs folder ${i + 1}`}
                  onclick={() => choose(i, 'songs')}
                  disabled={!isDesktop}><FolderOpen size={18} /></button
                >
              </div></label
            >{/if}
        </div>{/each}
      <DownloadPanel
        bind:automatic={automaticDownloads}
        bind:songs={downloadSongs}
        connectApi={showApiSettings}
        folders={downloadFolders}
        player={selectedPlayerLabel}
        status={queue}
        disabled={busy || sample}
        desktop={isDesktop}
        choose={chooseDownloadSongs}
        start={beginDownloads}
        pause={pauseQueue}
      />
      <div class="setting-actions">
        <button class="button" onclick={() => addSource('stable')}>Add stable folder</button><button
          class="button"
          onclick={() => addSource('lazer')}>Add lazer folder</button
        >
      </div>
      <div class="insight-note">
        <Info size={17} />
        <p>
          The first import calculates supported plays in all four modes. Saved PP and stars are
          reused on later imports. Close osu!lazer during a changed-source import so a consistent
          read-only snapshot can be taken.
        </p>
      </div>
      {#if !isDesktop}<p class="muted">
          Run the Tauri desktop app to access local game files. You can explore the sample library
          here.
        </p>{/if}
      <div class="modal-actions">
        <button class="button" onclick={() => (importing = false)}>Close</button><button
          class="button"
          disabled={!isDesktop || busy}
          onclick={saveFolders}>Save folders</button
        ><button
          class="button primary"
          disabled={!isDesktop || !gameSources.some((s) => s.root.trim()) || busy}
          onclick={() => scan()}><Database size={16} />Import scores</button
        >
      </div>
    </dialog>
  </div>{/if}
{#if selected}<div class="modal-backdrop" role="presentation">
    <dialog
      use:showDialog
      oncancel={() => (selected = null)}
      class="modal score-details"
      aria-modal="true"
      aria-labelledby="score-title"
      tabindex="-1"
    >
      <div class="detail-art">
        <Artwork id={selected.id} available={selected.hasArtwork} />
        <div class="card-shade"></div>
        <button
          class="icon-button detail-close"
          aria-label="Close score details"
          onclick={() => (selected = null)}><X size={20} /></button
        >
        <div class="detail-title">
          <span class="eyebrow">{selected.artist}</span>
          <h2 id="score-title">{selected.title}</h2>
          <p>{selected.difficulty} · mapped by {selected.mapper || 'unknown'}</p>
        </div>
      </div>
      <div class="detail-content">
        <div class="detail-pills">
          <button
            class="button small"
            disabled={!isDesktop || loadingSong}
            onclick={() => selected && playSong(selected)}
            >{loadingSong ? 'Finding audio…' : 'Play song'}</button
          >
          {#if replayIds.has(selected.id) || (onlineEnabled && onlineConnected && onlineReplays.get(selected.id)?.state === 'available')}<button
              class="button small"
              disabled={!!exporting}
              onclick={() => selected && downloadReplay(selected)}
              ><Download size={14} />{exporting === selected.id
                ? 'Saving…'
                : replayIds.has(selected.id)
                  ? 'Export replay (.osr)'
                  : 'Download online replay (.osr)'}</button
            >{/if}
          {#if !replayIds.has(selected.id)}<p class="muted">
              {onlineEnabled && onlineConnected
                ? onlineReplays.get(selected.id)?.message ||
                  (onlineChecking
                    ? 'Checking for an online replay…'
                    : 'Online replay not checked yet.')
                : 'No local replay found. Connect online replays in Settings to check osu!.'}
            </p>{/if}
          <span class="mod-badge">{scoreMods(selected)}</span><span class="stars"
            >★ {number(selected.stars, 2)}</span
          >{#if selected.fc}<span class="sample-badge">FULL COMBO</span>{/if}<span class="muted"
            >{selected.source} · {selected.player}</span
          >
        </div>
        <div class="detail-metrics">
          <div><span>Calculated PP</span><strong>{number(selected.pp, 1)}</strong></div>
          <div><span>PP if FC</span><strong class="pink">{number(selected.fcPp, 1)}</strong></div>
          <div><span>Accuracy</span><strong>{number(selected.accuracy, 2)}%</strong></div>
        </div>
        {#if songMessage}<p role="status">{songMessage}</p>{/if}
        {#if exportFeedback?.id === selected.id}<p role="status">{exportFeedback.text}</p>{/if}
        <dl class="score-facts">
          <dt>Combo</dt>
          <dd>{number(selected.combo)} / {number(selected.maxCombo)}</dd>
          <dt>300 / 100 / 50 / miss</dt>
          <dd>{selected.n300} / {selected.n100} / {selected.n50} / {selected.misses}</dd>
          <dt>Geki / katu (mode-specific)</dt>
          <dd>{selected.nGeki ?? 0} / {selected.nKatu ?? 0}</dd>
          <dt>Raw score</dt>
          <dd>{number(selected.rawScore)}</dd>
          <dt>Played</dt>
          <dd>{new Date(selected.date).toLocaleString()}</dd>
          <dt>Status</dt>
          <dd>{selected.status}</dd>
          <dt>Ruleset</dt>
          <dd>
            {['osu!standard', 'osu!taiko', 'osu!catch', 'osu!mania'][selected.mode] ||
              selected.mode}
          </dd>
          <dt>Exact map revision</dt>
          <dd class="hash">{selected.mapHash}</dd>
        </dl>
        <div class="insight-note">
          <Info size={17} />
          <p>
            {sample ? 'Sample values are illustrative. ' : ''}{selected.mode === 0
              ? 'FC estimates replace misses with 300s, keep 100s and 50s, and use full combo.'
              : selected.mode === 1
                ? 'FC estimates replace misses with great hits and retain existing ok hits.'
                : selected.mode === 2
                  ? 'FC estimates restore missed fruits and droplets; tiny droplet misses are retained.'
                  : 'FC estimates replace misses with MAX judgments while retaining other judgments. Mania PP depends on accuracy rather than combo alone.'}
            These are hypothetical results, not awarded account PP.
          </p>
        </div>
      </div>
    </dialog>
  </div>{/if}
