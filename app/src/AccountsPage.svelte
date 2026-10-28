<script lang="ts">
  import * as api from './lib/api'
  import type { AccountsReport } from './lib/api'
  import AccountCard from './lib/AccountCard.svelte'
  import { summariseAccounts } from './lib/accountsSummary'
  import AddAccount from './lib/AddAccount.svelte'
  import AnimatedNumber from './lib/AnimatedNumber.svelte'
  import Card from './lib/Card.svelte'
  import CsvImport from './lib/CsvImport.svelte'
  import DailyChart from './lib/DailyChart.svelte'
  import Sharing from './lib/Sharing.svelte'
  import { money } from './lib/format'

  let { days, now }: { days: number; now: number } = $props()

  let data = $state<AccountsReport | null>(null)
  let error = $state<string | null>(null)
  let adding = $state<'key' | 'file' | null>(null)
  let syncing = $state(false)

  async function load() {
    try {
      data = await api.accountsReport(days)
      error = null
    } catch (e) {
      error = String(e)
    }
  }

  $effect(() => {
    void days
    load()
  })

  $effect(() => {
    const unlisten = api.onAccountsUpdated(load)
    return () => void unlisten.then((f) => f())
  })

  async function sync() {
    syncing = true
    await api.syncNow()
    syncing = false
  }

  const summary = $derived(data ? summariseAccounts(data) : [])
  const synced = $derived(data?.accounts.some((a) => a.kind !== 'csv') ?? false)
</script>

{#if error}
  <p class="error" role="alert">Could not load your accounts: {error}</p>
{/if}

{#if data}
  <section class="hero" class:empty={!data.accounts.length} aria-label="Summary">
    {#if data.accounts.length}
      <div>
        <div class="big"><AnimatedNumber value={data.billed} format={money} /></div>
        <div class="hero-label">billed by your providers in the last {data.days} days</div>
      </div>
    {/if}
    <div class="hero-text">
      {#each summary as line, i (i)}<p>{line}</p>{/each}
      {#if synced}
        <button class="link" disabled={syncing} onclick={sync}>{syncing ? 'Reading…' : 'Read the accounts again now'}</button>
      {/if}
    </div>
  </section>

  <div class="grid">
    {#if data.accounts.length}
      <div class="wide">
        <Card title="Billed day by day" explain="billed" index={1}>
          <DailyChart days={data.daily} metric="cost" />
        </Card>
      </div>
    {/if}
    {#each data.accounts as a, i (a.id)}
      <div class="wide">
        <Card title={a.name} explain={a.credit_total !== null ? 'credit' : a.kind === 'csv' ? 'csv' : 'billed'} index={i + 2}>
          <AccountCard {a} days={data.days} {now} />
        </Card>
      </div>
    {/each}
    <div class="wide">
      <Card title="Add an account" explain="accounts" index={data.accounts.length + 2}>
        {#if adding === 'key'}
          <AddAccount onclose={() => (adding = null)} />
        {:else if adding === 'file'}
          <CsvImport onclose={() => (adding = null)} />
        {:else}
          <p class="intro">
            Read what a provider actually billed, straight from its own reports. This counts every app and computer on the
            account, not just this one.
          </p>
          <div class="buttons">
            <button class="primary" onclick={() => (adding = 'key')}>Add with a key</button>
            <button class="link" onclick={() => (adding = 'file')}>Import a usage file (Gemini, TypeSafe, Qwen Cloud…)</button>
          </div>
        {/if}
      </Card>
    </div>
    <div class="wide">
      <Card title="Your other computers" explain="devices" index={data.accounts.length + 3}>
        <Sharing {now} />
      </Card>
    </div>
  </div>
{:else if !error}
  <div class="loading" aria-busy="true">Reading your accounts…</div>
{/if}

<style>
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

  .grid {
    display: grid;
    grid-template-columns: repeat(auto-fit, minmax(min(100%, 360px), 1fr));
    gap: 14px;
  }

  .wide {
    grid-column: 1 / -1;
  }

  .intro {
    max-width: 75ch;
    margin: 0 0 12px;
    font-size: 0.88rem;
    color: var(--text-secondary);
  }

  .loading {
    padding: 80px 0;
    text-align: center;
    color: var(--text-secondary);
  }

  .hero.empty {
    grid-template-columns: 1fr;
  }

  @media (max-width: 640px) {
    .hero {
      grid-template-columns: 1fr;
    }
  }
</style>
