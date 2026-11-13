<script lang="ts">
  import { Copy } from '@lucide/svelte';
  import { formatTable, type TableColumn } from './table-copy';
  let {
    title,
    columns,
    rows,
    disabled = false,
  }: { title: string; columns: TableColumn[]; rows: string[][]; disabled?: boolean } = $props();
  let snapshot = $state.raw<{ full: string; parts: string[] } | null>(null);
  let part = $state(0),
    feedback = $state(''),
    showText = $state(false);
  async function write(text: string, label: string) {
    try {
      await navigator.clipboard.writeText(text);
      feedback = label;
    } catch {
      showText = true;
      feedback = 'Select the text below and press Ctrl+C to copy.';
    }
  }
  async function copy() {
    try {
      snapshot = formatTable(title, columns, rows);
      part = 0;
      showText = false;
      await write(
        snapshot.parts[0],
        snapshot.parts.length > 1
          ? `Copied part 1 of ${snapshot.parts.length}. Paste each part as a separate Discord message.`
          : 'Table copied for Discord.',
      );
    } catch (e) {
      feedback = String(e);
    }
  }
  async function next() {
    if (!snapshot) return;
    part = (part + 1) % snapshot.parts.length;
    await write(snapshot.parts[part], `Copied part ${part + 1} of ${snapshot.parts.length}.`);
  }
</script>

<div class="table-copy-tools">
  <div class="setting-actions">
    <button
      class="button small"
      disabled={disabled || !rows.length}
      onclick={copy}
      title="Copy this table as aligned text for Discord"><Copy size={14} />Copy table</button
    >
    {#if snapshot && snapshot.parts.length > 1}
      <span class="muted">Part {part + 1} / {snapshot.parts.length}</span>
      <button class="button small" onclick={next}
        >{part + 1 === snapshot.parts.length ? 'Copy first part' : 'Copy next part'}</button
      >
      <button
        class="text-button"
        onclick={() =>
          snapshot &&
          write(snapshot.full, 'Entire table copied. This is longer than one Discord message.')}
        >Copy entire table</button
      >
    {/if}
  </div>
  {#if feedback}<p class="muted" role="status">{feedback}</p>{/if}
  {#if showText && snapshot}<textarea
      class="table-copy-text"
      aria-label="Formatted table text"
      readonly
      value={snapshot.parts[part]}
      onfocus={(e) => e.currentTarget.select()}></textarea>{/if}
</div>

<style>
  .table-copy-tools {
    margin: 12px 0;
  }
  .table-copy-tools p {
    font-size: 12px;
    margin: 8px 0;
  }
  .table-copy-text {
    width: 100%;
    min-height: 160px;
    font: 12px monospace;
    white-space: pre;
  }
</style>
