<script lang="ts">
  import { onMount } from 'svelte';
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
  import TrendChart from './lib/TrendChart.svelte';
  import MusicPlayer, { type Track } from './lib/MusicPlayer.svelte';
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
  } from './lib/model';
  import { demoScores } from './lib/demo';
  import {
    isDesktop,
    loadLibrary,
    importStable,
    importLazer,
    replayAvailability,
    exportReplay,
    saveSources,
    audioTrack,
  } from './lib/backend';
  import { themes } from './lib/themes';
  import type { QueryResult } from './lib/query';

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
  let replayIds = $state.raw(new Set<string>());
  let replayWarning = $state('');
  let exporting = $state('');
  let exportFeedback = $state<{ id: string; text: string } | null>(null);
  let track = $state.raw<Track | null>(null);
  let loadingSong = $state(false),
    songMessage = $state('');
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
  let visible = $derived(
    result.ids.flatMap((id) => {
      const s = byId.get(id);
      return s ? [s] : [];
    }),
  );
  $effect(() => {
    localStorage.setItem('sx-replay-folder', JSON.stringify(replayFolder));
  });
  $effect(() => {
    const ids = [...new Set([...visible.map((s) => s.id), ...(selected ? [selected.id] : [])])];
    if (!isDesktop || sample || !ids.length) {
      replayIds = new Set();
      return;
    }
    let active = true;
    replayIds = new Set();
    replayAvailability(ids)
      .then((status) => {
        if (!active) return;
        replayIds = new Set(status.available.map((r) => r.id));
        replayWarning = status.warning || '';
      })
      .catch((e) => {
        if (active) replayWarning = String(e);
      });
    return () => {
      active = false;
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
  let players = $derived(
    [...new Set(scores.flatMap((s) => [s.player, ...(s.playerAliases || [])]))].sort(),
  );
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
        if (library.gameSources) gameSources = library.gameSources;
        defaultPlayer();
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
    loadingSong = true;
    songMessage = '';
    try {
      const url = await audioTrack(score.id);
      if (!url) {
        songMessage = 'No installed audio was found for this map.';
        return;
      }
      track = { id: score.id, url, title: score.title, artist: score.artist };
      songMessage = '';
      selected = null;
    } catch (e) {
      songMessage = String(e);
    } finally {
      loadingSong = false;
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
      const destination = await exportReplay(score.id, folder);
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
            ><option value="">All players</option>{#each players as player}<option>{player}</option
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
              ><option value="gain">FC PP gain</option><option value="accuracy">Accuracy</option
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
            <label title="Non-perfect judgments: 100s, 50s and misses in standard."
              >Max non-300s<input
                id="non300"
                type="number"
                min="0"
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
              >Maximum misses<input
                type="number"
                min="0"
                placeholder="Any"
                value={filters.maxMisses ?? ''}
                oninput={(e) =>
                  (filters.maxMisses =
                    e.currentTarget.value === ''
                      ? null
                      : Math.max(0, Number(e.currentTarget.value)))}
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
                  >{#if replayIds.has(score.id)}<button
                      class="replay-button"
                      disabled={!!exporting}
                      title="Export local replay (.osr)"
                      aria-label={`Export replay for ${score.title}`}
                      onclick={() => downloadReplay(score)}
                      ><Download size={13} />{exporting === score.id ? 'Saving…' : 'Replay'}</button
                    >{:else}<ArrowUpRight size={14} />{/if}
                </div>
              </article>{/each}
          </div>
        {:else}<div class="table-wrap">
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
                  >{player}</option
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
        {#if trendPoints.length}<div class="table-wrap">
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
            <Database size={22} />
            <h2>Calculation cache</h2>
            <p>
              PP and star results persist in an app-owned SQLite database. Opening the app loads
              these results without rescanning.
            </p>
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
          <section class="settings-panel">
            <Info size={22} />
            <h2>Combined library</h2>
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
              exploration, and progression are connected. Online map recovery and automatic updates
              remain planned.
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
          {#if replayIds.has(selected.id)}<button
              class="button small"
              disabled={!!exporting}
              onclick={() => selected && downloadReplay(selected)}
              ><Download size={14} />{exporting === selected.id
                ? 'Saving…'
                : 'Export replay (.osr)'}</button
            >{/if}
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
