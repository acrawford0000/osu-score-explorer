<script lang="ts">
  import type { DownloadStatus } from './backend';
  let { automatic = $bindable(false), songs = $bindable(''), clientId = $bindable(''),
    clientSecret = $bindable(''), folders, player, status, disabled, desktop,
    choose, start, pause }: {
      automatic: boolean; songs: string; clientId: string; clientSecret: string;
      folders: string[]; player: string; status: DownloadStatus | null; disabled: boolean;
      desktop: boolean; choose: () => void; start: () => void; pause: () => void;
    } = $props();
</script>

<div class="download-options">
  <h3>Missing beatmaps</h3>
  <label class="download-toggle"><input type="checkbox" bind:checked={automatic} />Automatically download missing beatmaps after import · no video</label>
  <p class="muted">For {player || 'all players'}. Downloads include music and artwork. Scores and PP update as each mapset finishes.</p>
  <p class="muted">PP requires the exact version played. A mirror may only have a newer revision; those scores stay marked unavailable.</p>
  <label class="form-label">Download to Songs folder
    <div class="path-input"><input aria-label="Beatmap download Songs folder" bind:value={songs} placeholder="Choose a stable Songs folder" disabled={status?.running} /><button class="button small" disabled={!desktop || status?.running} onclick={choose}>Choose folder</button></div>
  </label>
  {#if folders.length}<label class="form-label">Use a connected Songs folder
    <select aria-label="Connected download destination" value="" disabled={status?.running} onchange={(e) => { if(e.currentTarget.value) songs = e.currentTarget.value; }}><option value="">Select a folder…</option>{#each folders as folder}<option value={folder}>{folder}</option>{/each}</select>
  </label>{/if}
  <details>
    <summary>osu! API lookup · optional when you have their osu!.db</summary>
    <p class="muted">Catalog IDs are used first. For missing IDs, enter your osu! OAuth application's client ID and secret. Credentials are kept for this session only.</p>
    <a href="https://osu.ppy.sh/home/account/edit#oauth" target="_blank" rel="noreferrer">Manage osu! OAuth applications</a>
    <div class="download-api-fields"><label class="form-label">Client ID<input aria-label="osu API client ID" bind:value={clientId} inputmode="numeric" autocomplete="off" disabled={status?.running} /></label><label class="form-label">Client secret<input aria-label="osu API client secret" type="password" bind:value={clientSecret} autocomplete="off" disabled={status?.running} /></label></div>
  </details>
  <div class="setting-actions">
    <button class="button" disabled={!desktop || disabled || status?.running || !songs.trim()} onclick={start}>{status?.phase === 'paused' ? 'Resume downloads' : 'Download missing beatmaps'}</button>
    {#if status?.running}<button class="button" onclick={pause}>Pause downloads</button>{/if}
  </div>
  {#if status?.message}<p role="status">{status.message}</p>{/if}
  {#if status?.total}<p class="muted">{status.completed.toLocaleString()} ready · {status.pending.toLocaleString()} queued · {status.unavailable.toLocaleString()} unavailable or need API lookup</p>{/if}
  {#if status?.bytes}<p class="muted">{(status.bytes / 1048576).toFixed(1)} MiB{status.totalBytes ? ` / ${(status.totalBytes / 1048576).toFixed(1)} MiB` : ''}</p>{/if}
  {#if status?.issues.length}<ul class="download-issues">{#each status.issues as issue}<li>{issue}</li>{/each}</ul>{/if}
</div>
