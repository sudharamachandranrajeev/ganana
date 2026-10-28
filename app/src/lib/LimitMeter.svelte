<script lang="ts">
  import { Tween } from 'svelte/motion'
  import type { LimitWindow } from './api'
  import { clock, duration, relativeTime, windowName } from './format'
  import { tweenOptions } from './motion'
  import WindowChart from './WindowChart.svelte'

  let { w, now }: { w: LimitWindow; now: number } = $props()

  const pct = $derived(Math.min(100, Math.max(0, w.used_pct)))
  const shown = Tween.of(() => pct, tweenOptions(900))
  const level = $derived(pct >= 100 ? 'full' : pct >= 90 ? 'critical' : pct >= 75 ? 'warning' : 'ok')
  const LEVELS = { full: 'Limit reached', critical: 'Almost out', warning: 'Getting close', ok: '' }
  const resetsMs = $derived(w.resets_at * 1000)

  const forecast = $derived.by(() => {
    if (w.reset_passed || pct >= 100) return ''
    if (w.full_at) {
      return `At your recent pace it fills around ${clock(w.full_at * 1000, now)}, ${duration(resetsMs - w.full_at * 1000)} before it resets.`
    }
    if (w.rate_per_hour === null) return ''
    if (w.rate_per_hour <= 0) return 'It is not filling right now.'
    // Not full before the reset (no full_at), so this stays below 100.
    const atReset = Math.min(99, pct + (w.rate_per_hour * (resetsMs - now)) / 3_600_000)
    return `At your recent pace you stay under it: about ${Math.floor(atReset)}% by the reset.`
  })
</script>

<div class="window {level}">
  <div class="head">
    <span class="name">{windowName(w.window_minutes)}</span>
    {#if level !== 'ok'}
      <span class="state">
        <svg viewBox="0 0 16 16" aria-hidden="true"><path d="M8 1.5 15 14H1Z" /><path class="mark" d="M8 6v3.5M8 11.5v.5" /></svg>
        {LEVELS[level]}
      </span>
    {/if}
  </div>
  <div class="figure"><span class="pct">{Math.round(shown.current)}%</span> used</div>
  <div
    class="meter"
    role="meter"
    aria-label="{windowName(w.window_minutes)} used"
    aria-valuemin={0}
    aria-valuemax={100}
    aria-valuenow={Math.round(pct)}
  >
    <div class="fill" style:width="{shown.current}%"></div>
  </div>
  <p>
    {#if w.reset_passed}
      Reset {clock(resetsMs, now)}. It starts again from zero with your next request.
    {:else}
      Resets in <strong>{duration(resetsMs - now)}</strong>, at {clock(resetsMs, now)}.
    {/if}
    {forecast}
  </p>
  {#if !w.reset_passed && w.history.length}
    <WindowChart {w} {now} />
  {/if}
  <p class="seen">Last reading {relativeTime(w.seen_at, now)}</p>
</div>

<style>
  .window {
    --fill: var(--accent);
    min-width: 0;
  }

  .window.warning {
    --fill: var(--status-warning);
  }

  .window.critical,
  .window.full {
    --fill: var(--status-critical);
  }

  .head {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 8px;
    font-size: 0.88rem;
    color: var(--text-secondary);
  }

  .name {
    font-weight: 600;
  }

  .state {
    display: inline-flex;
    align-items: center;
    gap: 5px;
    padding: 2px 8px;
    border-radius: 999px;
    background: color-mix(in srgb, var(--fill) 16%, transparent);
    color: var(--text-primary);
    font-size: 0.8rem;
    font-weight: 600;
  }

  .state svg {
    width: 14px;
    height: 14px;
    fill: var(--fill);
  }

  .state .mark {
    fill: none;
    stroke: var(--surface-solid);
    stroke-width: 1.6;
    stroke-linecap: round;
  }

  .figure {
    margin: 6px 0 8px;
    color: var(--text-secondary);
    font-size: 0.9rem;
  }

  .pct {
    color: var(--text-primary);
    font-size: 2rem;
    font-weight: 650;
    letter-spacing: -0.02em;
  }

  /* The track is a light step of the fill's own colour, so the state reads
     across the whole bar. */
  .meter {
    height: 10px;
    border-radius: 999px;
    background: color-mix(in srgb, var(--fill) 18%, transparent);
    overflow: hidden;
  }

  .fill {
    height: 100%;
    border-radius: 999px;
    background: var(--fill);
    transition: background 0.4s var(--ease);
  }

  p {
    margin: 10px 0 0;
    font-size: 0.88rem;
    color: var(--text-secondary);
  }

  strong {
    color: var(--text-primary);
  }

  .seen {
    margin-top: 6px;
    font-size: 0.78rem;
    color: var(--text-muted);
  }
</style>
