<script lang="ts">
  import type { TrendPoint } from './model';
  let { points, unit }: { points: TrendPoint[]; unit: string } = $props();
  let active = $state<number | null>(null);
  const width = 1000,
    height = 280;
  let max = $derived(Math.max(1, ...points.map((p) => p.value || 0)) * 1.15);
  let step = $derived((width - 80) / Math.max(1, points.length - 1));
  let segments = $derived.by(() => {
    const lines: string[] = [];
    let current = '';
    points.forEach((p, i) => {
      if (p.value === null) {
        if (current) lines.push(current);
        current = '';
      } else
        current += `${current ? ' L' : 'M'}${50 + i * step},${height - 30 - (p.value / max) * (height - 55)}`;
    });
    if (current) lines.push(current);
    return lines;
  });
</script>

<div
  class="chart-wrap"
  role="group"
  aria-label="Interactive progression chart"
  onpointerleave={() => (active = null)}
>
  <svg
    viewBox={`0 0 ${width} ${height}`}
    role="group"
    aria-label={`Average ${unit} over time. Detailed data is in the table below.`}
  >
    {#each [0, 1, 2, 3, 4] as tick}
      {@const y = height - 30 - (tick / 4) * (height - 55)}
      <line x1="50" x2={width - 25} y1={y} y2={y} stroke="var(--border)" stroke-dasharray="3 5" />
      <text x="4" y={y + 4} fill="var(--muted)" font-size="11">{((max * tick) / 4).toFixed(1)}</text
      >
    {/each}
    {#each segments as path}<path
        d={path}
        fill="none"
        stroke="var(--accent)"
        stroke-width="3"
        stroke-linecap="round"
      />{/each}
    {#each points as p, i}
      <g
        role="button"
        tabindex="0"
        aria-label={`${p.period}: ${p.value === null ? 'No average available' : p.value.toFixed(4) + ' ' + unit}; ${p.count} included; ${p.missing} unavailable`}
        aria-describedby={active === i ? 'progression-tooltip' : undefined}
        onpointerenter={() => (active = i)}
        onfocus={() => (active = i)}
        onblur={() => (active = null)}
        onclick={() => (active = i)}
        onkeydown={(e) => {
          if (e.key === 'Enter' || e.key === ' ') {
            e.preventDefault();
            active = i;
          }
        }}
      >
        <circle
          cx={50 + i * step}
          cy={p.value === null ? height - 30 : height - 30 - (p.value / max) * (height - 55)}
          r="12"
          fill="transparent"
        />
        {#if p.value !== null}<circle
            cx={50 + i * step}
            cy={height - 30 - (p.value / max) * (height - 55)}
            r="4"
            fill="var(--accent)"
          />{/if}</g
      >
      {#if points.length <= 15 || i % Math.ceil(points.length / 10) === 0}<text
          x={50 + i * step}
          y={height - 5}
          text-anchor="middle"
          fill="var(--muted)"
          font-size="11">{p.period}</text
        >{/if}
    {/each}
  </svg>
  {#if active !== null && points[active]}
    {@const point = points[active]}
    <div
      id="progression-tooltip"
      role="tooltip"
      class="chart-tooltip"
      style={`left:clamp(115px,${((50 + active * step) / width) * 100}%,calc(100% - 115px));top:${point.value === null ? 78 : Math.max(4, ((height - 30 - (point.value / max) * (height - 55)) / height) * 100)}%`}
    >
      <strong>{point.period}</strong><b
        >{point.value === null ? 'No average available' : `${point.value.toFixed(4)} ${unit}`}</b
      ><span
        >{point.count.toLocaleString()} included · {point.missing.toLocaleString()} unavailable</span
      >
    </div>
  {/if}
</div>
