<script lang="ts">
  import type { LimitWindow } from './api'
  import { clock } from './format'

  let { w, now }: { w: LimitWindow; now: number } = $props()

  const HEIGHT = 120
  const PAD = { top: 10, right: 10, bottom: 22, left: 34 }

  let width = $state(400)
  let hover = $state<number | null>(null)

  const end = $derived(w.resets_at * 1000)
  const start = $derived(end - w.window_minutes * 60_000)
  const plotW = $derived(Math.max(0, width - PAD.left - PAD.right))
  const plotH = HEIGHT - PAD.top - PAD.bottom
  const x = (t: number) => PAD.left + ((Math.min(Math.max(t, start), end) - start) / (end - start)) * plotW
  const y = (p: number) => PAD.top + plotH - (Math.min(Math.max(p, 0), 100) / 100) * plotH

  const points = $derived(w.history.filter(([t]) => t >= start - 60_000))
  const last = $derived(points.at(-1))
  const nowX = $derived(x(now))

  /** Step line: a reading holds until the next one, and the last until now. */
  const line = $derived.by(() => {
    if (!points.length) return ''
    let d = `M${x(points[0][0])},${y(points[0][1])}`
    for (const [t, p] of points.slice(1)) d += `H${x(t)}V${y(p)}`
    return d + `H${nowX}`
  })
  const area = $derived(line && `${line}V${y(0)}H${x(points[0][0])}Z`)

  /** Recent pace carried forward, to the full time or the reset. */
  const projection = $derived.by(() => {
    if (!last || !w.rate_per_hour || w.rate_per_hour <= 0) return null
    const to = w.full_at ? w.full_at * 1000 : end
    const p = w.full_at ? 100 : last[1] + (w.rate_per_hour * (to - now)) / 3_600_000
    return { x1: nowX, y1: y(last[1]), x2: x(to), y2: y(p) }
  })

  const span = $derived(end - start)
  // Over more than a day, a bare time is ambiguous: always name the day.
  const days = $derived(w.window_minutes > 1440)
  const ticks = $derived([start, start + span / 2, end])

  function nearest(px: number): number | null {
    if (!points.length) return null
    let best = 0
    points.forEach(([t], i) => {
      if (Math.abs(x(t) - px) < Math.abs(x(points[best][0]) - px)) best = i
    })
    return best
  }

  function onMove(e: PointerEvent) {
    const r = (e.currentTarget as SVGElement).getBoundingClientRect()
    hover = nearest(e.clientX - r.left)
  }

  function onKey(e: KeyboardEvent) {
    if (!points.length) return
    if (e.key === 'ArrowRight') hover = Math.min(points.length - 1, (hover ?? -1) + 1)
    else if (e.key === 'ArrowLeft') hover = Math.max(0, (hover ?? points.length) - 1)
    else return
    e.preventDefault()
  }

  const readout = (i: number) => (points[i] ? `${clock(points[i][0], now, days)}: ${Math.round(points[i][1])}% used` : '')
</script>

<div class="chart" bind:clientWidth={width}>
  <svg
    {width}
    height={HEIGHT}
    role="slider"
    aria-roledescription="chart"
    aria-label="How this window filled up. Use left and right arrows to read each change."
    aria-valuemin={1}
    aria-valuemax={Math.max(1, points.length)}
    aria-valuenow={(hover ?? points.length - 1) + 1}
    aria-valuetext={readout(hover ?? points.length - 1)}
    tabindex="0"
    onkeydown={onKey}
    onpointermove={onMove}
    onpointerleave={() => (hover = null)}
    onblur={() => (hover = null)}
  >
    {#each [0, 50, 100] as p (p)}
      <line class="grid" class:base={p === 0} x1={PAD.left} x2={width - PAD.right} y1={y(p)} y2={y(p)} />
      <text class="tick" x={PAD.left - 6} y={y(p)} dy="0.32em" text-anchor="end">{p}%</text>
    {/each}
    {#each ticks as t, i (i)}
      <text class="tick" x={x(t)} y={HEIGHT - 6} text-anchor={i === 0 ? 'start' : i === 2 ? 'end' : 'middle'}>
        {i === 2 ? `resets ${clock(t, now, days)}` : clock(t, now, days)}
      </text>
    {/each}
    {#if area}
      <path class="area" d={area} />
      <path class="line" d={line} pathLength="1" />
    {/if}
    {#if projection}
      <line class="projection" x1={projection.x1} y1={projection.y1} x2={projection.x2} y2={projection.y2} />
    {/if}
    {#if now > start && now < end}
      <line class="now" x1={nowX} x2={nowX} y1={PAD.top} y2={y(0)} />
    {/if}
    {#if hover !== null && points[hover]}
      {@const [t, p] = points[hover]}
      <line class="cross" x1={x(t)} x2={x(t)} y1={PAD.top} y2={y(0)} />
      <circle class="dot" cx={x(t)} cy={y(p)} r="4" />
    {/if}
  </svg>
  {#if hover !== null && points[hover]}
    {@const [t, p] = points[hover]}
    <div class="tooltip" style:left="{Math.min(Math.max(x(t), 70), width - 70)}px" role="status">
      <strong class="num">{Math.round(p)}%</strong> used
      <div class="muted">{clock(t, now, days)}</div>
    </div>
  {/if}
</div>

<style>
  .chart {
    position: relative;
    width: 100%;
    margin-top: 12px;
  }

  svg {
    display: block;
    overflow: visible;
    touch-action: pan-y;
  }

  svg:focus-visible {
    outline-offset: 6px;
    border-radius: 8px;
  }

  .grid {
    stroke: var(--grid);
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

  .area {
    fill: var(--accent-soft);
  }

  .line {
    fill: none;
    stroke: var(--accent);
    stroke-width: 2;
    stroke-linejoin: round;
    stroke-dasharray: 1;
    animation: draw 1.1s var(--ease) both;
  }

  @keyframes draw {
    from {
      stroke-dashoffset: 1;
    }
  }

  .projection {
    stroke: var(--accent);
    stroke-width: 2;
    stroke-dasharray: 4 4;
    opacity: 0.6;
  }

  .now {
    stroke: var(--text-muted);
    stroke-dasharray: 2 3;
  }

  .cross {
    stroke: var(--axis);
  }

  .dot {
    fill: var(--accent);
    stroke: var(--surface-solid);
    stroke-width: 2;
  }

  .tooltip {
    position: absolute;
    top: 0;
    transform: translate(-50%, -12px);
    padding: 6px 10px;
    border: 1px solid var(--border);
    border-radius: 10px;
    background: var(--surface-raised);
    backdrop-filter: blur(16px);
    box-shadow: var(--shadow);
    font-size: 0.82rem;
    white-space: nowrap;
    pointer-events: none;
  }

  .muted {
    color: var(--text-muted);
  }
</style>
