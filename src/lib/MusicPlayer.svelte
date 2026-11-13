<script lang="ts">
  import {
    Play,
    Pause,
    Music2,
    ChevronDown,
    ChevronUp,
    ArrowUpRight,
    CornerDownLeft,
    GripHorizontal,
  } from '@lucide/svelte';
  export type Track = { id: string; url: string; title: string; artist: string };
  let { track }: { track: Track | null } = $props();
  let audio = $state<HTMLAudioElement>();
  let panel = $state<HTMLElement>();
  let playing = $state(false),
    expanded = $state(false),
    floating = $state(false);
  let position = $state(0),
    duration = $state(0),
    problem = $state('');
  const savedVolume = Number(localStorage.getItem('sx-volume') ?? '0.5');
  let volume = $state(Number.isFinite(savedVolume) ? Math.max(0, Math.min(1, savedVolume)) : 0.5);
  let x = $state(260),
    y = $state(180);
  let drag: { x: number; y: number; left: number; top: number } | null = null;
  const time = (seconds: number) =>
    `${Math.floor(seconds / 60)}:${String(Math.floor(seconds % 60)).padStart(2, '0')}`;
  function move(left: number, top: number) {
    x = Math.max(8, Math.min(window.innerWidth - (panel?.offsetWidth || 300) - 8, left));
    y = Math.max(8, Math.min(window.innerHeight - (panel?.offsetHeight || 260) - 8, top));
  }
  $effect(() => {
    if (!audio) return;
    audio.volume = volume;
    localStorage.setItem('sx-volume', String(volume));
  });
  $effect(() => {
    const url = track?.url;
    const element = audio;
    if (!url || !element) return;
    problem = '';
    position = 0;
    duration = 0;
    let active = true;
    element.load();
    void element.play().catch(() => {
      if (active) problem = 'Press Play to start the song.';
    });
    return () => {
      active = false;
      element.pause();
    };
  });
  async function toggle() {
    if (!audio || !track) return;
    if (playing) audio.pause();
    else {
      try {
        await audio.play();
        problem = '';
      } catch {
        problem = 'This audio could not be played.';
      }
    }
  }
  function seek(value: number) {
    if (audio && duration > 0) {
      audio.currentTime = Math.max(0, Math.min(duration, value));
      position = audio.currentTime;
    }
  }
</script>

<svelte:window onresize={() => floating && move(x, y)} />
<audio
  bind:this={audio}
  src={track?.url}
  preload="metadata"
  onplay={() => (playing = true)}
  onpause={() => (playing = false)}
  onended={() => (playing = false)}
  ontimeupdate={() => (position = audio?.currentTime || 0)}
  onloadedmetadata={() => (duration = Number.isFinite(audio?.duration) ? audio!.duration : 0)}
  onerror={() => (problem = 'The audio is unavailable or its format could not be played.')}
></audio>
<section
  bind:this={panel}
  class="music-player"
  class:floating
  style={floating ? `left:${x}px;top:${y}px` : ''}
  aria-label="Music player"
>
  {#if floating}<button
      class="drag-handle"
      aria-label="Move music player"
      title="Drag to move, or use arrow keys"
      onpointerdown={(e) => {
        if (e.button !== 0) return;
        drag = { x: e.clientX, y: e.clientY, left: x, top: y };
        e.currentTarget.setPointerCapture(e.pointerId);
      }}
      onpointermove={(e) => {
        if (drag) move(drag.left + e.clientX - drag.x, drag.top + e.clientY - drag.y);
      }}
      onpointerup={() => (drag = null)}
      onpointercancel={() => (drag = null)}
      onkeydown={(e) => {
        const delta: Record<string, number[]> = {
          ArrowLeft: [-16, 0],
          ArrowRight: [16, 0],
          ArrowUp: [0, -16],
          ArrowDown: [0, 16],
        };
        if (delta[e.key]) {
          e.preventDefault();
          move(x + delta[e.key][0], y + delta[e.key][1]);
        }
      }}><GripHorizontal size={16} />Now playing</button
    >{/if}
  <div class="track">
    <Music2 size={15} />
    <div>
      <strong title={track?.title}>{track?.title || 'Your soundtrack'}</strong><span
        >{track?.artist || 'Play a song from score details'}</span
      >
    </div>
  </div>
  <div class="controls">
    <button
      class="play"
      disabled={!track}
      onclick={toggle}
      aria-label={playing ? 'Pause song' : 'Play song'}
      >{#if playing}<Pause size={15} />{:else}<Play size={15} />{/if}</button
    >
    <span class="elapsed">{time(position)}</span>
    <button
      disabled={!track}
      aria-label={expanded ? 'Collapse music controls' : 'Expand music controls'}
      aria-expanded={expanded}
      onclick={() => (expanded = !expanded)}
      >{#if expanded}<ChevronDown size={15} />{:else}<ChevronUp size={15} />{/if}</button
    >
    <button
      disabled={!track}
      aria-label={floating ? 'Dock music player' : 'Float music player'}
      onclick={() => {
        floating = !floating;
        if (floating) expanded = true;
      }}
      >{#if floating}<CornerDownLeft size={15} />{:else}<ArrowUpRight size={15} />{/if}</button
    >
  </div>
  {#if expanded}<div class="expanded">
      <label
        >Seek<input
          aria-label="Song position"
          type="range"
          min="0"
          max={duration || 1}
          step="0.1"
          value={position}
          disabled={!duration}
          oninput={(e) => seek(Number(e.currentTarget.value))}
        /></label
      >
      <div class="times"><span>{time(position)}</span><span>{time(duration)}</span></div>
      <label
        >Volume<input
          aria-label="Music volume"
          type="range"
          min="0"
          max="1"
          step="0.01"
          bind:value={volume}
        /></label
      >
    </div>{/if}
  {#if problem && track}<p role="status">{problem}</p>{/if}
</section>

<style>
  .music-player {
    padding: 11px;
    margin: 12px 0;
    border: 1px solid var(--border);
    border-radius: 9px;
    background: var(--panel);
    min-width: 0;
    color: var(--text);
  }
  .floating {
    position: fixed;
    width: 300px;
    max-width: calc(100vw - 16px);
    z-index: 100;
    margin: 0;
    box-shadow: 0 12px 36px #0006;
  }
  .track {
    display: flex;
    align-items: center;
    gap: 9px;
    min-width: 0;
  }
  .track > :global(svg) {
    flex-shrink: 0;
    color: var(--accent);
  }
  .track div {
    min-width: 0;
  }
  strong,
  .track span {
    display: block;
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
  }
  strong {
    font-size: 11px;
    font-weight: 500;
  }
  .track span {
    font-size: 9px;
    color: var(--muted);
    margin-top: 4px;
  }
  .controls {
    display: flex;
    gap: 5px;
    align-items: center;
    margin-top: 9px;
  }
  button {
    display: inline-flex;
    justify-content: center;
    align-items: center;
    padding: 5px;
    border-radius: 5px;
    color: var(--muted);
  }
  button:hover {
    color: var(--accent);
    background: var(--accent-wash);
  }
  button:disabled {
    opacity: 0.35;
    cursor: default;
  }
  .play {
    color: var(--accent);
    background: var(--accent-wash);
  }
  .elapsed {
    font-size: 9px;
    color: var(--muted);
    font-variant-numeric: tabular-nums;
    flex: 1;
    padding-left: 3px;
  }
  .expanded {
    margin-top: 9px;
  }
  label {
    display: block;
    font-size: 9px;
    color: var(--muted);
  }
  input[type='range'] {
    display: block;
    width: 100%;
    padding: 0;
    margin: 7px 0;
    accent-color: var(--accent);
  }
  .times {
    display: flex;
    justify-content: space-between;
    color: var(--muted);
    font-size: 9px;
    margin-bottom: 10px;
    font-variant-numeric: tabular-nums;
  }
  .drag-handle {
    width: 100%;
    justify-content: flex-start;
    gap: 6px;
    font-size: 9px;
    margin-bottom: 9px;
    cursor: grab;
    touch-action: none;
  }
  p {
    font-size: 9px;
    color: var(--muted);
    margin: 8px 0 0;
  }
</style>
