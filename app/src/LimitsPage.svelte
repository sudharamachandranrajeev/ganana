<script lang="ts">
  import * as api from './lib/api'
  import type { Limits, Relay } from './lib/api'
  import Alerts from './lib/Alerts.svelte'
  import Card from './lib/Card.svelte'
  import ConnectClaude from './lib/ConnectClaude.svelte'
  import { meterName } from './lib/format'
  import LimitMeter from './lib/LimitMeter.svelte'
  import { summariseLimits } from './lib/limitsSummary'
  import Plans from './lib/Plans.svelte'

  let { now }: { now: number } = $props()

  let data = $state<Limits | null>(null)
  let relay = $state<Relay | null>(null)
  let error = $state<string | null>(null)

  async function load() {
    try {
      ;[data, relay] = await Promise.all([api.limits(), api.relayStatus()])
      error = null
    } catch (e) {
      error = String(e)
    }
  }

  $effect(() => {
    load()
    const unlisten = api.onUsageUpdated(load)
    return () => void unlisten.then((f) => f())
  })

  const meters = $derived([...new Set((data?.windows ?? []).map((w) => w.meter))].sort())
  const hasClaude = $derived(meters.includes('claude'))
  const summary = $derived(data ? summariseLimits(data.windows, now) : [])
</script>

{#if error}
  <p class="error" role="alert">Could not load your limits: {error}</p>
{/if}

{#if data && relay}
  <section class="hero" aria-label="Summary">
    {#each summary as line, i (i)}<p>{line}</p>{/each}
  </section>

  <div class="grid">
    {#if relay.state === 'off' || !hasClaude}
      <div class="wide">
        <Card title="Connect Claude Pro/Max" explain="connect" index={1}>
          <ConnectClaude {relay} waiting={!hasClaude} onchange={(r) => (relay = r)} />
        </Card>
      </div>
    {/if}
    {#each meters as meter, i (meter)}
      <div class="wide">
        <Card title={meterName(meter)} explain="limits" index={i + 2}>
          <div class="windows">
            {#each data.windows.filter((w) => w.meter === meter) as w (w.window_minutes)}
              <LimitMeter {w} {now} />
            {/each}
          </div>
          {#if meter === 'claude' && relay.state === 'on'}
            <div class="relay-on"><ConnectClaude {relay} waiting={false} onchange={(r) => (relay = r)} /></div>
          {/if}
        </Card>
      </div>
    {/each}
    <div class="span2">
      <Card title="Your plans" explain="plans" index={meters.length + 2}>
        <Plans plans={data.plans} onchange={load} />
      </Card>
    </div>
    <Card title="Alerts" explain="alerts" index={meters.length + 3}>
      <Alerts thresholds={data.thresholds} onchange={load} />
    </Card>
  </div>
{:else if !error}
  <div class="loading" aria-busy="true">Reading your limits…</div>
{/if}

<style>
  .error {
    padding: 12px 16px;
    border-radius: 12px;
    background: rgba(208, 59, 59, 0.12);
  }

  .hero {
    margin-bottom: 18px;
    padding: 22px 28px;
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

  .hero p {
    margin: 0 0 6px;
    color: var(--text-secondary);
  }

  .hero p:first-child {
    color: var(--text-primary);
    font-size: 1.1rem;
  }

  .hero p:last-child {
    margin-bottom: 0;
  }

  .grid {
    display: grid;
    grid-template-columns: repeat(auto-fit, minmax(min(100%, 360px), 1fr));
    gap: 14px;
  }

  .wide {
    grid-column: 1 / -1;
  }

  /* Plans take two of three columns where there are three. */
  @media (min-width: 1150px) {
    .span2 {
      grid-column: span 2;
    }
  }

  .windows {
    display: grid;
    grid-template-columns: repeat(auto-fit, minmax(min(100%, 300px), 1fr));
    gap: 20px 32px;
  }

  .relay-on {
    margin-top: 16px;
    padding-top: 12px;
    border-top: 1px solid var(--border);
  }

  .loading {
    padding: 80px 0;
    text-align: center;
    color: var(--text-secondary);
  }
</style>
