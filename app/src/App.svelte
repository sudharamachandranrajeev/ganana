<script lang="ts">
  import { onMount } from 'svelte'
  import * as api from './lib/api'
  import type { Overview, Status } from './lib/api'
  import AnimatedNumber from './lib/AnimatedNumber.svelte'
  import BarList from './lib/BarList.svelte'
  import Card from './lib/Card.svelte'
  import DailyChart from './lib/DailyChart.svelte'
  import { compact, money, pages, percent, relativeTime } from './lib/format'
  import { summarise } from './lib/summary'
  import TokenMix from './lib/TokenMix.svelte'
  import Unpriced from './lib/Unpriced.svelte'

  type Theme = 'system' | 'light' | 'dark'
  const THEME_NAMES: Record<Theme, string> = { system: 'match the system', light: 'light', dark: 'dark' }
  const RANGES = [7, 30, 90]

  const stored = (key: string) => {
    try {
      return localStorage.getItem(key)
    } catch {
      return null
    }
  }
  const store = (key: string, value: string) => {
    try {
      localStorage.setItem(key, value)
    } catch {
      /* private mode: the choice just isn't remembered */
    }
  }

  let days = $state(Number(stored('days')) || 30)
  let metric = $state<'cost' | 'tokens'>(stored('metric') === 'tokens' ? 'tokens' : 'cost')
  let theme = $state<Theme>((stored('theme') as Theme) || 'system')
  let data = $state<Overview | null>(null)
  let status = $state<Status | null>(null)
  let error = $state<string | null>(null)
  let now = $state(Date.now())

  $effect(() => {
    if (theme === 'system') document.documentElement.removeAttribute('data-theme')
    else document.documentElement.dataset.theme = theme
    store('theme', theme)
  })
  $effect(() => store('days', String(days)))
  $effect(() => store('metric', metric))

  // Reloads are coalesced: a burst of log writes triggers one query.
  let loading: Promise<void> | null = null
  let again = false
  async function load() {
    if (loading) {
      again = true
      return loading
    }
    loading = (async () => {
      try {
        data = await api.overview(days)
        error = null
      } catch (e) {
        error = String(e)
      }
    })()
    await loading
    loading = null
    if (again) {
      again = false
      await load()
    }
  }

  $effect(() => {
    void days
    load()
  })

  onMount(() => {
    const unlisten = [api.onUsageUpdated(load), api.onStatus((s) => (status = s))]
    api.status().then((s) => (status = s))
    const tick = setInterval(() => (now = Date.now()), 15_000)
    return () => {
      clearInterval(tick)
      unlisten.forEach((u) => u.then((f) => f()))
    }
  })

  const summary = $derived(data ? summarise(data) : [])
  const cycleTheme = () => (theme = theme === 'system' ? 'light' : theme === 'light' ? 'dark' : 'system')
  const errors = $derived(status?.last_report?.errors ?? [])
</script>

{#snippet unpricedNote(t: api.Totals)}
  {#if t.unpriced_tokens > 0}
    <a class="tile-note" href="#unpriced">+ {compact(t.unpriced_tokens)} tokens not priced yet</a>
  {/if}
{/snippet}

<main>
  <header class="top">
    <div class="brand">
      <img src="/favicon.svg" alt="" width="40" height="40" />
      <div>
        <h1>Gaṇanā</h1>
        <p>Your AI usage, explained</p>
      </div>
    </div>
    <div class="controls">
      <div class="seg" role="radiogroup" aria-label="Time range">
        {#each RANGES as r (r)}
          <button role="radio" aria-checked={days === r} class:on={days === r} onclick={() => (days = r)}>{r} days</button>
        {/each}
      </div>
      <div class="seg" role="radiogroup" aria-label="Measure">
        <button role="radio" aria-checked={metric === 'cost'} class:on={metric === 'cost'} onclick={() => (metric = 'cost')}>Cost</button>
        <button role="radio" aria-checked={metric === 'tokens'} class:on={metric === 'tokens'} onclick={() => (metric = 'tokens')}>Tokens</button>
      </div>
      <button class="icon" onclick={cycleTheme} title="Theme: {THEME_NAMES[theme]}" aria-label="Theme: {THEME_NAMES[theme]}. Change theme">
        <svg viewBox="0 0 24 24" aria-hidden="true">
          {#if theme === 'dark'}
            <path d="M20 14.5A8 8 0 0 1 9.5 4a8 8 0 1 0 10.5 10.5Z" />
          {:else if theme === 'light'}
            <circle cx="12" cy="12" r="4" />
            <path d="M12 2v2M12 20v2M4.9 4.9l1.4 1.4M17.7 17.7l1.4 1.4M2 12h2M20 12h2M4.9 19.1l1.4-1.4M17.7 6.3l1.4-1.4" />
          {:else}
            <circle cx="12" cy="12" r="8" />
            <path d="M12 4a8 8 0 0 1 0 16Z" class="fill" />
          {/if}
        </svg>
      </button>
      <button class="icon" onclick={() => api.rescan()} title="Check the logs again now" aria-label="Check the logs again now">
        <svg viewBox="0 0 24 24" aria-hidden="true" class:spin={status?.scanning}>
          <path d="M20 12a8 8 0 1 1-2.3-5.6M20 4v4.5h-4.5" />
        </svg>
      </button>
    </div>
  </header>

  <p class="status" aria-live="polite">
    {#if status?.scanning}
      Reading new usage…
    {:else if status?.last_scan}
      Up to date · checked {relativeTime(status.last_scan, now)} · watching {status.watched.length} log folders, updates by itself
    {:else}
      Starting…
    {/if}
    {#if errors.length}
      <span class="warn" title={errors.join('\n')}> · {errors.length} log{errors.length > 1 ? 's' : ''} could not be read</span>
    {/if}
  </p>

  {#if error}
    <p class="error" role="alert">Could not load your usage: {error}</p>
  {/if}

  {#if data && (data.events_stored > 0 || status?.last_scan)}
    <section class="hero" aria-label="Summary">
      <div class="hero-figure">
        <div class="big"><AnimatedNumber value={metric === 'cost' ? data.period.cost : data.period.total_tokens} format={metric === 'cost' ? money : compact} /></div>
        <div class="hero-label">
          {metric === 'cost' ? 'worth of AI use at public API prices' : 'tokens'} in the last {data.days} days
        </div>
      </div>
      <div class="hero-text">
        {#each summary as line, i (i)}<p>{line}</p>{/each}
      </div>
    </section>

    <div class="tiles">
      <Card title="Today" explain="cost" index={1}>
        <div class="tile-value"><AnimatedNumber value={data.today.cost} format={money} /></div>
        <div class="tile-sub">{compact(data.today.total_tokens)} tokens · {compact(data.today.calls)} calls</div>
        {@render unpricedNote(data.today)}
      </Card>
      <Card title="Last 7 days" explain="cost" index={2}>
        <div class="tile-value"><AnimatedNumber value={data.week.cost} format={money} /></div>
        <div class="tile-sub">{compact(data.week.total_tokens)} tokens · {compact(data.week.calls)} calls</div>
        {@render unpricedNote(data.week)}
      </Card>
      <Card title="Tokens, {data.days} days" explain="tokens" index={3}>
        <div class="tile-value"><AnimatedNumber value={data.period.total_tokens} format={compact} /></div>
        <div class="tile-sub">about {pages(data.period.total_tokens)} pages of text</div>
      </Card>
      <Card title="Model calls" explain="calls" index={4}>
        <div class="tile-value"><AnimatedNumber value={data.period.calls} format={compact} /></div>
        <div class="tile-sub">{compact(data.period.calls / data.days)} a day on average</div>
      </Card>
      <Card title="Saved by the cache" explain="savings" index={5}>
        <div class="tile-value good"><AnimatedNumber value={data.period.savings} format={money} /></div>
        <div class="tile-sub">{percent(data.period.tokens.cache_read, data.period.total_tokens)} of tokens were cheap re-reads</div>
      </Card>
    </div>

    <div class="grid">
      <div class="wide">
        <Card title="Day by day" explain="daily" index={6}>
          <DailyChart days={data.daily} {metric} />
        </Card>
      </div>
      <Card title="What kind of tokens" explain="mix" index={7}>
        <TokenMix totals={data.period} />
      </Card>
      <Card title="Models" explain="models" index={8}>
        <BarList slices={data.models} {metric} colorBy="provider" />
      </Card>
      <Card title="Projects" explain="projects" index={9}>
        <BarList slices={data.projects} {metric} />
      </Card>
      <Card title="Apps" explain="sources" index={10}>
        <BarList slices={data.sources} {metric} />
      </Card>
      <div id="unpriced">
        <Card title="Models without a price" explain="unpriced" index={11}>
          <Unpriced unpriced={data.unpriced} onchange={load} />
        </Card>
      </div>
    </div>

    <footer>
      Gaṇanā reads only token counts, model names, times and folder names from your AI apps' logs. Your
      conversations never leave those logs. Prices come from models.dev{#if status?.prices_fetched_at}, updated {relativeTime(status.prices_fetched_at, now)}{:else}{' '}(built-in
        copy){/if}. {compact(data.events_stored)} calls stored in total.
    </footer>
  {:else if !error}
    <div class="loading" aria-busy="true">
      Reading your AI usage history…
      <small>The first read goes through every log once. After that, only new lines are read.</small>
    </div>
  {/if}
</main>

<style>
  main {
    max-width: 1280px;
    margin: 0 auto;
    padding: 28px clamp(16px, 3vw, 36px) 40px;
  }

  .top {
    display: flex;
    flex-wrap: wrap;
    align-items: center;
    justify-content: space-between;
    gap: 16px;
  }

  .brand {
    display: flex;
    align-items: center;
    gap: 12px;
  }

  .brand img {
    border-radius: 12px;
    box-shadow: var(--shadow);
  }

  h1 {
    margin: 0;
    font-size: 1.5rem;
    letter-spacing: -0.01em;
  }

  .brand p {
    margin: 0;
    color: var(--text-secondary);
    font-size: 0.9rem;
  }

  .controls {
    display: flex;
    flex-wrap: wrap;
    gap: 8px;
  }

  .seg {
    display: inline-flex;
    padding: 3px;
    border: 1px solid var(--border);
    border-radius: 12px;
    background: var(--surface);
    backdrop-filter: blur(16px);
  }

  .seg button {
    padding: 6px 12px;
    border: 0;
    border-radius: 9px;
    background: transparent;
    color: var(--text-secondary);
    font-size: 0.88rem;
    transition:
      background 0.3s var(--ease),
      color 0.3s var(--ease),
      box-shadow 0.3s var(--ease);
  }

  .seg button.on {
    background: var(--surface-raised);
    color: var(--text-primary);
    box-shadow: var(--shadow);
    font-weight: 600;
  }

  .icon {
    width: 38px;
    height: 38px;
    border: 1px solid var(--border);
    border-radius: 12px;
    background: var(--surface);
    backdrop-filter: blur(16px);
    font-size: 1.05rem;
    transition: transform 0.4s var(--ease);
  }

  .icon:hover {
    transform: scale(1.06);
  }

  .icon {
    display: grid;
    place-items: center;
  }

  .icon svg {
    width: 18px;
    height: 18px;
    fill: none;
    stroke: currentColor;
    stroke-width: 1.8;
    stroke-linecap: round;
    stroke-linejoin: round;
  }

  .icon svg .fill {
    fill: currentColor;
    stroke: none;
  }

  .icon svg.spin {
    transform-box: fill-box;
    transform-origin: center;
    animation: spin 1s linear infinite;
  }

  @keyframes spin {
    to {
      transform: rotate(360deg);
    }
  }

  .status {
    margin: 12px 0 22px;
    font-size: 0.82rem;
    color: var(--text-muted);
  }

  .warn {
    color: var(--warning-ink);
  }

  .error {
    padding: 12px 16px;
    border-radius: 12px;
    background: rgba(208, 59, 59, 0.12);
  }

  .hero {
    display: grid;
    grid-template-columns: minmax(0, auto) minmax(0, 1fr);
    gap: 18px 36px;
    align-items: center;
    margin-bottom: 18px;
    padding: 26px 28px;
    border: 1px solid var(--border);
    border-radius: calc(var(--radius) + 4px);
    background:
      linear-gradient(135deg, var(--accent-soft), transparent 60%),
      var(--surface);
    backdrop-filter: blur(24px) saturate(1.4);
    box-shadow: var(--shadow);
    animation: rise 0.7s var(--ease) both;
  }

  @keyframes rise {
    from {
      opacity: 0;
      transform: translateY(14px);
    }
  }

  .big {
    font-size: clamp(2.6rem, 6vw, 3.8rem);
    font-weight: 650;
    letter-spacing: -0.03em;
    line-height: 1.05;
  }

  .hero-label {
    margin-top: 4px;
    max-width: 28ch;
    color: var(--text-secondary);
    font-size: 0.9rem;
  }

  .hero-text p {
    margin: 0 0 6px;
    color: var(--text-secondary);
  }

  .hero-text p:first-child {
    color: var(--text-primary);
    font-size: 1.05rem;
  }

  .tiles {
    display: grid;
    grid-template-columns: repeat(auto-fit, minmax(190px, 1fr));
    gap: 14px;
    margin-bottom: 14px;
  }

  .tile-value {
    font-size: 1.75rem;
    font-weight: 650;
    letter-spacing: -0.02em;
    line-height: 1.1;
  }

  .tile-value.good {
    color: var(--good);
  }

  .tile-sub {
    margin-top: 6px;
    font-size: 0.82rem;
    color: var(--text-secondary);
  }

  .tile-note {
    display: inline-block;
    margin-top: 4px;
    font-size: 0.8rem;
    color: var(--warning-ink);
    text-decoration: none;
  }

  .tile-note:hover {
    text-decoration: underline;
  }

  .grid {
    display: grid;
    grid-template-columns: repeat(auto-fit, minmax(min(100%, 360px), 1fr));
    gap: 14px;
  }

  .wide {
    grid-column: 1 / -1;
  }

  footer {
    margin-top: 28px;
    font-size: 0.8rem;
    color: var(--text-muted);
    max-width: 80ch;
  }

  .loading small {
    display: block;
    margin-top: 8px;
    color: var(--text-muted);
  }

  .loading {
    padding: 80px 0;
    text-align: center;
    color: var(--text-secondary);
    animation: pulse 1.6s ease-in-out infinite;
  }

  @keyframes pulse {
    50% {
      opacity: 0.5;
    }
  }

  @media (max-width: 640px) {
    .hero {
      grid-template-columns: 1fr;
    }
  }
</style>
