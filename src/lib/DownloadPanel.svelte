<script lang="ts">
  import type { DownloadStatus } from './backend';
  let {
    automatic = $bindable(false),
    songs = $bindable(''),
    connectApi,
    folders,
    player,
    status,
    disabled,
    desktop,
    choose,
    start,
    pause,
  }: {
    automatic: boolean;
    songs: string;
    connectApi: () => void;
    folders: string[];
    player: string;
    status: DownloadStatus | null;
    disabled: boolean;
    desktop: boolean;
    choose: () => void;
    start: () => void;
    pause: () => void;
  } = $props();
</script>

<details class="download-options">
  <summary>Missing beatmaps</summary>
  <label class="download-toggle"
    ><input type="checkbox" bind:checked={automatic} />Automatically download missing beatmaps after
    import · no video</label
  >
  <p class="muted">
    For {player || 'all players'}. Downloads include music and artwork. Scores and PP update as each
    mapset finishes.
  </p>
  <p class="muted">
    PP requires the exact version played. A mirror may only have a newer revision; those scores stay
    marked unavailable.
  </p>
  <label class="form-label"
    >Download to Songs folder
    <div class="path-input">
      <input
        aria-label="Beatmap download Songs folder"
        bind:value={songs}
        placeholder="Choose a stable Songs folder"
        disabled={status?.running}
      /><button class="button small" disabled={!desktop || status?.running} onclick={choose}
        >Choose folder</button
      >
    </div>
  </label>
  {#if folders.length}<label class="form-label"
      >Use a connected Songs folder
      <select
        aria-label="Connected download destination"
        value=""
        disabled={status?.running}
        onchange={(e) => {
          if (e.currentTarget.value) songs = e.currentTarget.value;
        }}
        ><option value="">Select a folder…</option>{#each folders as folder}<option value={folder}
            >{folder}</option
          >{/each}</select
      >
    </label>{/if}
  <p class="muted">
    Your osu!.db catalog is checked first. Connect the API to find beatmap IDs missing from the
    catalog.
  </p>
  <a
    href="#osu-api-settings"
    onclick={(e) => {
      e.preventDefault();
      connectApi();
    }}>Connect API</a
  >

  <div class="setting-actions">
    <button
      class="button"
      disabled={!desktop || disabled || status?.running || !songs.trim()}
      onclick={start}
      >{status?.phase === 'paused' ? 'Resume downloads' : 'Download missing beatmaps'}</button
    >
    {#if status?.running}<button class="button" onclick={pause}>Pause downloads</button>{/if}
  </div>
  {#if status?.message}<p role="status">{status.message}</p>{/if}
  {#if status?.running || status?.downloadedSets}<p class="muted">
      {status?.activeDownloads ?? 0} / 3 downloads active · {status?.downloadedSets ?? 0} downloaded ·
      {status?.installedSets ?? 0} installed this session
    </p>{/if}
  {#if status?.total}<p class="muted">
      {status.completed.toLocaleString()} ready · {status.pending.toLocaleString()} queued · {status.unavailable.toLocaleString()}
      unavailable or need API lookup
    </p>{/if}
  {#if status?.bytes}<p class="muted">
      {(status.bytes / 1048576).toFixed(1)} MiB{status.totalBytes
        ? ` / ${(status.totalBytes / 1048576).toFixed(1)} MiB`
        : ''}
    </p>{/if}
  {#if status?.issues.length}<ul class="download-issues">
      {#each status.issues as issue}<li>{issue}</li>{/each}
    </ul>{/if}
</details>
