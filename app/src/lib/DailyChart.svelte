<script lang="ts">
  import { untrack } from 'svelte'
  import { Tween } from 'svelte/motion'
  import type { Day } from './api'
  import { providerColor, sortProviders } from './colors'
  import { compact, dayLabel, money, nameOf } from './format'
  import { tweenOptions } from './motion'

  type Metric = 'cost' | 'tokens'
  let { days, metric }: { days: Day[]; metric: Metric } = $props()

  const HEIGHT = 240
  const PAD = { top: 12, right: 8, bottom: 26, left: 52 }
  const GAP = 2

  let width = $state(600)
  let hover = $state<number | null>(null)
  let table = $state(false)

  const fmt = $derived(metric === 'cost' ? money : compact)
  const valuesOf = (d: Day) => (metric === 'cost' ? d.cost_by_provider : d.tokens_by_provider)
  const providers = $derived(sortProviders(days.flatMap((d) => Object.keys(valuesOf(d)).filter((p) => valuesOf(d)[p] > 0))))
  // Flattened [day][provider] values, tweened together so bars glide.
  const target = $derived(days.flatMap((d) => providers.map((p) => valuesOf(d)[p] ?? 0)))
  const shown = new Tween<number[]>([], tweenOptions(750))
  $effect(() => {
    const t = target
    // Only `target` is a dependency: reading the tween here would restart
    // it on every frame.
    untrack(() => {
      if (shown.current.length !== t.length) shown.set(t.map(() => 0), { duration: 0 })
      shown.target = t
    })
  })

  const totals = $derived(days.map((d) => (metric === 'cost' ? d.cost : d.total_tokens)))
  const max = $derived(niceMax(Math.max(0, ...totals)))
  const ticks = $derived([0, max / 4, max / 2, (3 * max) / 4, max])
  const plotW = $derived(Math.max(0, width - PAD.left - PAD.right))
  const plotH = HEIGHT - PAD.top - PAD.bottom
  const band = $derived(days.length ? plotW / days.length : 0)
  const barW = $derived(Math.max(2, Math.min(24, band * 0.62)))
  const y = (v: number) => PAD.top + plotH - (max ? (v / max) * plotH : 0)
  const labelEvery = $derived(Math.max(1, Math.ceil(days.length / Math.max(1, Math.floor(plotW / 64)))))

  function niceMax(v: number): number {
    if (v <= 0) return 1
    const step = 10 ** Math.floor(Math.log10(v))
    for (const m of [1, 1.5, 2, 2.5, 3, 4, 5, 6, 8, 10]) if (m * step >= v) return m * step
    return 10 * step
  }

  /** Stacked segments for one day, bottom up, with a surface gap between. */
  function segments(i: number) {
    const out: { provider: string; y: number; h: number; top: boolean }[] = []
    let base = 0
    const vals = providers.map((_, j) => shown.current[i * providers.length + j] ?? 0)
    const last = vals.findLastIndex((v) => v > 0)
    vals.forEach((v, j) => {
      if (v <= 0) return
      const y0 = y(base)
      const y1 = y(base + v)
      base += v
      const h = y0 - y1 - (j === last ? 0 : GAP)
      if (h > 0.3) out.push({ provider: providers[j], y: y1 + (j === last ? 0 : GAP), h, top: j === last })
    })
    return out
  }

  /** A bar with 4px rounded top corners and a square base. */
  function topRounded(x: number, yTop: number, w: number, h: number) {
    const r = Math.min(4, w / 2, h)
    return `M${x},${yTop + h}V${yTop + r}Q${x},${yTop} ${x + r},${yTop}H${x + w - r}Q${x + w},${yTop} ${x + w},${yTop + r}V${yTop + h}Z`
  }

  function valueText(i: number): string {
    const d = days[i]
    if (!d) return ''
    const parts = providers.filter((p) => (valuesOf(d)[p] ?? 0) > 0).map((p) => `${nameOf(p)} ${fmt(valuesOf(d)[p])}`)
    return `${dayLabel(d.date, 'long')}: ${fmt(metric === 'cost' ? d.cost : d.total_tokens)} in total. ${parts.join(', ')}`
  }

  function onKey(e: KeyboardEvent) {
    if (!days.length) return
    if (e.key === 'ArrowRight') hover = Math.min(days.length - 1, (hover ?? -1) + 1)
    else if (e.key === 'ArrowLeft') hover = Math.max(0, (hover ?? days.length) - 1)
    else return
    e.preventDefault()
  }
</script>

<div class="legend" aria-label="Providers">
  {#each providers as p (p)}
    <span><i style:background={providerColor(p)}></i>{nameOf(p)}</span>
  {/each}
  <button class="link" onclick={() => (table = !table)}>{table ? 'Show as chart' : 'Show as table'}</button>
</div>

{#if table}
  <div class="table-wrap">
    <table>
      <thead>
        <tr><th>Day</th>{#each providers as p (p)}<th>{nameOf(p)}</th>{/each}<th>Total</th></tr>
      </thead>
      <tbody>
        {#each days.toReversed() as d (d.date)}
          <tr>
            <td>{dayLabel(d.date)}</td>
            {#each providers as p (p)}<td class="num">{fmt(valuesOf(d)[p] ?? 0)}</td>{/each}
            <td class="num"><strong>{fmt(metric === 'cost' ? d.cost : d.total_tokens)}</strong></td>
          </tr>
        {/each}
      </tbody>
    </table>
  </div>
{:else}
  <div class="chart" bind:clientWidth={width}>
    <svg
      width={width}
      height={HEIGHT}
      role="slider"
      aria-roledescription="chart"
      aria-label="{metric === 'cost' ? 'Cost' : 'Tokens'} per day, by provider. Use left and right arrows to read each day."
      aria-valuemin={1}
      aria-valuemax={days.length}
      aria-valuenow={(hover ?? days.length - 1) + 1}
      aria-valuetext={valueText(hover ?? days.length - 1)}
      tabindex="0"
      onkeydown={onKey}
      onmouseleave={() => (hover = null)}
      onblur={() => (hover = null)}
    >
      {#each ticks as t, i (i)}
        <line class="grid" class:base={i === 0} x1={PAD.left} x2={width - PAD.right} y1={y(t)} y2={y(t)} />
        <text class="tick" x={PAD.left - 8} y={y(t)} dy="0.32em" text-anchor="end">{fmt(t)}</text>
      {/each}
      {#each days as d, i (d.date)}
        {@const cx = PAD.left + band * i + band / 2}
        <g class="col" class:dim={hover !== null && hover !== i}>
          {#each segments(i) as s (s.provider)}
            {#if s.top}
              <path d={topRounded(cx - barW / 2, s.y, barW, s.h)} fill={providerColor(s.provider)} />
            {:else}
              <rect x={cx - barW / 2} y={s.y} width={barW} height={s.h} fill={providerColor(s.provider)} />
            {/if}
          {/each}
        </g>
        {#if i % labelEvery === (days.length - 1) % labelEvery}
          <text class="tick" x={cx} y={HEIGHT - 8} text-anchor="middle">{dayLabel(d.date)}</text>
        {/if}
        <rect
          class="hit"
          x={PAD.left + band * i}
          y={PAD.top}
          width={band}
          height={plotH}
          role="presentation"
          onmouseenter={() => (hover = i)}
        />
      {/each}
    </svg>
    {#if hover !== null && days[hover]}
      {@const d = days[hover]}
      {@const cx = PAD.left + band * hover + band / 2}
      <div class="tooltip" style:left="{Math.min(Math.max(cx, 110), width - 110)}px" role="status">
        <div class="tt-date">{dayLabel(d.date, 'long')}</div>
        {#each providers.filter((p) => (valuesOf(d)[p] ?? 0) > 0).toReversed() as p (p)}
          <div class="tt-row"><i style:background={providerColor(p)}></i>{nameOf(p)}<span class="num">{fmt(valuesOf(d)[p])}</span></div>
        {:else}
          <div class="tt-row muted">No usage</div>
        {/each}
        <div class="tt-total">Total<span class="num">{fmt(metric === 'cost' ? d.cost : d.total_tokens)}</span></div>
      </div>
    {/if}
  </div>
{/if}

<style>
  .legend {
    display: flex;
    flex-wrap: wrap;
    align-items: center;
    gap: 6px 16px;
    margin-bottom: 10px;
    font-size: 0.82rem;
    color: var(--text-secondary);
  }

  .legend span {
    display: inline-flex;
    align-items: center;
    gap: 6px;
  }

  i {
    display: inline-block;
    width: 10px;
    height: 10px;
    border-radius: 3px;
  }

  .link {
    margin-left: auto;
    border: 0;
    background: none;
    color: var(--accent);
    font-size: 0.82rem;
    padding: 2px 0;
  }

  .chart {
    position: relative;
    width: 100%;
  }

  svg {
    display: block;
    overflow: visible;
  }

  svg:focus-visible {
    outline-offset: 6px;
    border-radius: 8px;
  }

  .grid {
    stroke: var(--grid);
    stroke-width: 1;
    shape-rendering: crispEdges;
  }

  .grid.base {
    stroke: var(--axis);
  }

  .tick {
    fill: var(--text-muted);
    font-size: 11px;
    font-variant-numeric: tabular-nums;
  }

  .col {
    transition: opacity 0.25s var(--ease);
  }

  .col.dim {
    opacity: 0.35;
  }

  .hit {
    fill: transparent;
  }

  .tooltip {
    position: absolute;
    top: 0;
    transform: translate(-50%, -8px);
    min-width: 200px;
    padding: 10px 12px;
    border: 1px solid var(--border);
    border-radius: 12px;
    background: var(--surface-raised);
    backdrop-filter: blur(16px);
    box-shadow: var(--shadow);
    font-size: 0.82rem;
    pointer-events: none;
    animation: pop 0.18s var(--ease);
  }

  @keyframes pop {
    from {
      opacity: 0;
      transform: translate(-50%, -2px);
    }
  }

  .tt-date {
    font-weight: 600;
    margin-bottom: 6px;
  }

  .tt-row,
  .tt-total {
    display: flex;
    align-items: center;
    gap: 6px;
    color: var(--text-secondary);
  }

  .tt-row span,
  .tt-total span {
    margin-left: auto;
    padding-left: 12px;
    color: var(--text-primary);
  }

  .tt-total {
    margin-top: 6px;
    padding-top: 6px;
    border-top: 1px solid var(--border);
    font-weight: 600;
    color: var(--text-primary);
  }

  .muted {
    color: var(--text-muted);
  }

  .table-wrap {
    max-height: 320px;
    overflow: auto;
  }

  table {
    width: 100%;
    border-collapse: collapse;
    font-size: 0.85rem;
  }

  th,
  td {
    padding: 6px 8px;
    text-align: right;
    border-bottom: 1px solid var(--border);
    white-space: nowrap;
  }

  th:first-child,
  td:first-child {
    text-align: left;
  }

  th {
    position: sticky;
    top: 0;
    background: var(--surface-solid);
    color: var(--text-secondary);
    font-weight: 600;
  }
</style>
