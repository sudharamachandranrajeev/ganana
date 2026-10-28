<script lang="ts">
  import * as api from './api'
  import type { AccountView } from './api'
  import BarList from './BarList.svelte'
  import CsvImport from './CsvImport.svelte'
  import { compact, money, nameOf, relativeTime } from './format'

  let { a, days, now }: { a: AccountView; days: number; now: number } = $props()

  const KINDS: Record<string, string> = {
    anthropic: 'Anthropic admin key',
    openai: 'OpenAI admin key',
    openrouter: 'OpenRouter management key',
  }

  let confirming = $state(false)
  let importing = $state(false)
  let error = $state<string | null>(null)

  const left = $derived(a.credit_total !== null && a.credit_used !== null ? Math.max(0, a.credit_total - a.credit_used) : null)
  // OpenRouter and file imports bill per model, so their lines repeat the models list.
  const showItems = $derived(a.items.length > 1 && a.items.some((i) => !a.models.some((m) => m.name === i.name)))
  const low = $derived(left !== null && a.credit_total !== null && left < Math.max(1, a.credit_total / 10))

  async function remove() {
    try {
      await api.removeAccount(a.id)
    } catch (err) {
      error = String(err)
      confirming = false
    }
  }
</script>

<div class="meta">
  {a.kind === 'csv' ? `Imported from files · ${nameOf(a.provider)}` : `${KINDS[a.kind]} …${a.key_hint ?? ''}`}
  · {a.synced_at ? `${a.kind === 'csv' ? 'imported' : 'read'} ${relativeTime(a.synced_at, now)}` : 'not read yet'}
</div>
{#if a.error}<p class="problem" role="alert">{a.error}</p>{/if}

<div class="figures">
  <div>
    <div class="value">{money(a.totals.cost)}</div>
    <div class="label">billed in {days} days</div>
  </div>
  <div>
    <div class="value">{compact(a.totals.total_tokens)}</div>
    <div class="label">tokens</div>
  </div>
  {#if a.totals.calls > 0}
    <div>
      <div class="value">{compact(a.totals.calls)}</div>
      <div class="label">requests</div>
    </div>
  {/if}
</div>

{#if left !== null && a.credit_total}
  <div class="credit">
    <div class="credit-row">
      <span>Credit left</span>
      <span class="num"><strong>{money(left)}</strong> of {money(a.credit_total)}</span>
    </div>
    <div
      class="track"
      role="meter"
      aria-label="Credit left"
      aria-valuemin={0}
      aria-valuemax={a.credit_total}
      aria-valuenow={left}
      aria-valuetext="{money(left)} of {money(a.credit_total)} left"
    >
      <div class="fill" class:low style:width="{(left / a.credit_total) * 100}%"></div>
    </div>
    {#if low}<p class="note warn">Running low. Requests fail when it reaches zero.</p>{/if}
  </div>
{/if}

{#if a.local_cost > 0}
  <p class="compare">
    Your local logs show {nameOf(a.provider)} work worth <strong>{money(a.local_cost)}</strong> at list prices over the
    same days. The two are not meant to match: the logs include subscription use that no API bill covers, and the bill
    includes use from other computers and apps.
  </p>
{/if}

{#if showItems || a.models.length}
  <div class="lists">
    {#if showItems}
      <div>
        <h3>Billed for</h3>
        <BarList slices={a.items} metric="cost" limit={5} />
      </div>
    {/if}
    {#if a.models.length}
      <div>
        <h3>Models, by tokens</h3>
        <BarList slices={a.models} metric="tokens" limit={5} />
      </div>
    {/if}
  </div>
{/if}

{#if importing}
  <CsvImport account={a} onclose={() => (importing = false)} />
{:else}
  <div class="buttons actions">
    {#if a.kind === 'csv'}<button class="link" onclick={() => (importing = true)}>Import another file</button>{/if}
    {#if confirming}
      <span class="ask">Remove {a.name}{a.kind === 'csv' ? '' : ' and its key'}?</span>
      <button class="link danger" onclick={remove}>Remove</button>
      <button class="link" onclick={() => (confirming = false)}>Keep</button>
    {:else}
      <button class="link" onclick={() => (confirming = true)}>Remove account</button>
    {/if}
  </div>
{/if}
{#if error}<p class="problem" role="alert">Could not remove it: {error}</p>{/if}

<style>
  .meta,
  .label,
  .note {
    font-size: 0.8rem;
    color: var(--text-muted);
  }

  .problem {
    margin: 10px 0 0;
    padding: 8px 12px;
    border-radius: 10px;
    background: color-mix(in srgb, var(--status-warning) 16%, transparent);
    font-size: 0.85rem;
  }

  .figures {
    display: flex;
    flex-wrap: wrap;
    gap: 12px 32px;
    margin-top: 14px;
  }

  .value {
    font-size: 1.6rem;
    font-weight: 650;
    letter-spacing: -0.02em;
    line-height: 1.1;
  }

  .credit {
    margin-top: 16px;
  }

  .credit-row {
    display: flex;
    justify-content: space-between;
    gap: 12px;
    font-size: 0.85rem;
    color: var(--text-secondary);
  }

  .track {
    height: 8px;
    margin-top: 6px;
    border-radius: 999px;
    background: var(--accent-soft);
    overflow: hidden;
  }

  .fill {
    height: 100%;
    border-radius: 999px;
    background: var(--good);
    transition: width 0.8s var(--ease);
  }

  .fill.low {
    background: var(--status-warning);
  }

  .warn {
    margin: 6px 0 0;
    color: var(--warning-ink);
  }

  .compare {
    max-width: 75ch;
    margin: 14px 0 0;
    font-size: 0.85rem;
    color: var(--text-secondary);
  }

  strong {
    color: var(--text-primary);
  }

  .lists {
    display: grid;
    grid-template-columns: repeat(auto-fit, minmax(min(100%, 260px), 1fr));
    gap: 16px 28px;
    margin-top: 16px;
  }

  h3 {
    margin: 0 0 8px;
    font-size: 0.82rem;
    font-weight: 600;
    color: var(--text-secondary);
  }

  .actions {
    margin-top: 16px;
  }

  .ask {
    font-size: 0.85rem;
    color: var(--text-secondary);
  }

  .danger {
    color: var(--warning-ink);
    font-weight: 600;
  }
</style>
