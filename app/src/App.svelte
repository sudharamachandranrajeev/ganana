<script lang="ts">
  import { onMount } from 'svelte'
  import * as api from './lib/api'
  import type { Status } from './lib/api'
  import { relativeTime } from './lib/format'
  import AccountsPage from './AccountsPage.svelte'
  import LimitsPage from './LimitsPage.svelte'
  import OverviewPage from './OverviewPage.svelte'

  type Theme = 'system' | 'light' | 'dark'
  type Page = 'overview' | 'limits' | 'accounts'
  const THEME_NAMES: Record<Theme, string> = { system: 'match the system', light: 'light', dark: 'dark' }
  const RANGES = [7, 30, 90]
  const PAGES: [Page, string][] = [
    ['overview', 'Overview'],
    ['limits', 'Plans & limits'],
    ['accounts', 'Accounts'],
  ]

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

  let page = $state<Page>(PAGES.find(([id]) => id === stored('page'))?.[0] ?? 'overview')
  let days = $state(Number(stored('days')) || 30)
  let metric = $state<'cost' | 'tokens'>(stored('metric') === 'tokens' ? 'tokens' : 'cost')
  let theme = $state<Theme>((stored('theme') as Theme) || 'system')
  let status = $state<Status | null>(null)
  let now = $state(Date.now())

  $effect(() => {
    if (theme === 'system') document.documentElement.removeAttribute('data-theme')
    else document.documentElement.dataset.theme = theme
    store('theme', theme)
  })
  $effect(() => store('page', page))
  $effect(() => store('days', String(days)))
  $effect(() => store('metric', metric))

  onMount(() => {
    const unlisten = api.onStatus((s) => (status = s))
    api.status().then((s) => (status = s))
    const tick = setInterval(() => (now = Date.now()), 15_000)
    return () => {
      clearInterval(tick)
      unlisten.then((f) => f())
    }
  })

  const cycleTheme = () => (theme = theme === 'system' ? 'light' : theme === 'light' ? 'dark' : 'system')
  const errors = $derived(status?.last_report?.errors ?? [])
</script>

<main>
  <header class="top">
    <div class="brand">
      <img src="/favicon.svg" alt="" width="40" height="40" />
      <div>
        <h1>Gaṇanā</h1>
        <p>Your AI usage, explained</p>
      </div>
    </div>
    <nav class="seg pages" aria-label="Pages">
      {#each PAGES as [id, label] (id)}
        <button aria-current={page === id ? 'page' : undefined} class:on={page === id} onclick={() => (page = id)}>{label}</button>
      {/each}
    </nav>
    <div class="controls">
      {#if page !== 'limits'}
      <div class="seg" role="radiogroup" aria-label="Time range">
        {#each RANGES as r (r)}
          <button role="radio" aria-checked={days === r} class:on={days === r} onclick={() => (days = r)}>{r} days</button>
        {/each}
      </div>
      {/if}
      {#if page === 'overview'}
      <div class="seg" role="radiogroup" aria-label="Measure">
        <button role="radio" aria-checked={metric === 'cost'} class:on={metric === 'cost'} onclick={() => (metric = 'cost')}>Cost</button>
        <button role="radio" aria-checked={metric === 'tokens'} class:on={metric === 'tokens'} onclick={() => (metric = 'tokens')}>Tokens</button>
      </div>
      {/if}
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

  {#if page === 'overview'}
    <OverviewPage {days} {metric} {status} {now} />
  {:else if page === 'limits'}
    <LimitsPage {now} />
  {:else}
    <AccountsPage {days} {now} />
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

  .pages {
    margin-right: auto;
    margin-left: 12px;
  }
</style>
