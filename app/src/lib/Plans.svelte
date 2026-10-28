<script lang="ts">
  import { slide } from 'svelte/transition'
  import * as api from './api'
  import type { Plan, PlanValue } from './api'
  import { compact, dayLabel, money, nameOf, ordinal } from './format'
  import { tweenOptions } from './motion'

  let { plans, onchange }: { plans: PlanValue[]; onchange: () => void } = $props()

  // Typical list prices to start from; the user corrects them to their bill.
  const PRESETS: Omit<Plan, 'id' | 'renews_day'>[] = [
    { provider: 'anthropic', name: 'Claude Pro', monthly_usd: 20 },
    { provider: 'anthropic', name: 'Claude Max 5x', monthly_usd: 100 },
    { provider: 'anthropic', name: 'Claude Max 20x', monthly_usd: 200 },
    { provider: 'openai', name: 'ChatGPT Plus', monthly_usd: 20 },
    { provider: 'openai', name: 'ChatGPT Pro', monthly_usd: 200 },
    { provider: 'google', name: 'Google AI Pro', monthly_usd: 19.99 },
  ]
  const PROVIDERS = ['anthropic', 'openai', 'google', 'alibaba', 'other']

  let editing = $state<Plan | null>(null)
  let error = $state<string | null>(null)
  const id = $props.id()

  const blank = (): Plan => ({ id: 0, provider: 'anthropic', name: '', monthly_usd: 0, renews_day: new Date().getDate() })

  function preset(e: Event) {
    const p = PRESETS[Number((e.currentTarget as HTMLSelectElement).value)]
    if (p && editing) editing = { ...editing, ...p }
  }

  async function save(e: SubmitEvent) {
    e.preventDefault()
    if (!editing) return
    try {
      await api.savePlan({ ...editing, monthly_usd: Number(editing.monthly_usd), renews_day: Number(editing.renews_day) })
      editing = null
      error = null
      onchange()
    } catch (err) {
      error = String(err)
    }
  }

  async function remove(p: PlanValue) {
    await api.deletePlan(p.id)
    onchange()
  }

  const daysBetween = (a: string, b: string) => Math.round((Date.parse(b) - Date.parse(a)) / 86_400_000)
  const today = () => {
    const d = new Date()
    return `${d.getFullYear()}-${String(d.getMonth() + 1).padStart(2, '0')}-${String(d.getDate()).padStart(2, '0')}`
  }
  /** Days of the billing period so far, today included. */
  const elapsed = (p: PlanValue) =>
    Math.min(daysBetween(p.period_start, p.period_end), daysBetween(p.period_start, today()) + 1)
</script>

{#snippet verdict(p: PlanValue)}
  {@const ratio = p.monthly_usd > 0 ? p.usage.cost / p.monthly_usd : 0}
  {#if p.monthly_usd <= 0}
    Add the price you pay to compare.
  {:else if ratio >= 1}
    That is <strong>{ratio >= 10 ? Math.round(ratio) : ratio.toFixed(1)}×</strong> what you pay, so far.
  {:else}
    That is less than you pay so far. At this rate, paying per use would cost about
    <strong>{money((p.usage.cost / elapsed(p)) * daysBetween(p.period_start, p.period_end))}</strong> this period.
  {/if}
{/snippet}

{#each plans as p (p.id)}
  {@const top = Math.max(p.monthly_usd, p.usage.cost, 0.01)}
  <article class="plan" transition:slide={tweenOptions(300)}>
    <header>
      <div>
        <h3>{p.name}</h3>
        <div class="meta">{money(p.monthly_usd)} a month · renews on the {ordinal(p.renews_day)}</div>
      </div>
      <div class="buttons">
        <button class="link" onclick={() => (editing = { id: p.id, provider: p.provider, name: p.name, monthly_usd: p.monthly_usd, renews_day: p.renews_day })}>Edit</button>
        <button class="link" onclick={() => remove(p)}>Remove</button>
      </div>
    </header>
    {#if p.source}
      <p>
        This period so far ({dayLabel(p.period_start)} – today, day {elapsed(p)} of {daysBetween(p.period_start, p.period_end)}),
        {nameOf(p.source)} did <strong>{money(p.usage.cost)}</strong> of work at API prices.
        {@render verdict(p)}
      </p>
      <div class="compare" aria-hidden="true">
        <span>You pay</span>
        <div class="bar"><div class="pay" style:width="{(p.monthly_usd / top) * 100}%"></div></div>
        <span class="num">{money(p.monthly_usd)}</span>
        <span>At API prices</span>
        <div class="bar"><div class="api" style:width="{(p.usage.cost / top) * 100}%"></div></div>
        <span class="num">{money(p.usage.cost)}</span>
      </div>
      {#if p.usage.unpriced_tokens > 0}
        <p class="note">Not counted: {compact(p.usage.unpriced_tokens)} tokens from models without a price.</p>
      {/if}
    {:else}
      <p>Gaṇanā cannot see this plan's usage on this computer, so there is nothing to compare yet.</p>
    {/if}
  </article>
{:else}
  {#if !editing}
    <p class="empty">Add the plans you pay for to see what each one is worth at API prices.</p>
  {/if}
{/each}

{#if editing}
  <form class="form" onsubmit={save} transition:slide={tweenOptions(300)}>
    {#if !editing.id}
      <label>
        Start from
        <select onchange={preset}>
          <option value="">Choose a plan…</option>
          {#each PRESETS as p, i (p.name)}<option value={i}>{p.name} (typically {money(p.monthly_usd)})</option>{/each}
        </select>
      </label>
    {/if}
    <label>Name <input required bind:value={editing.name} placeholder="Claude Max 5x" /></label>
    <label>
      Provider
      <select bind:value={editing.provider}>
        {#each PROVIDERS as pr (pr)}<option value={pr}>{pr === 'other' ? 'Other' : nameOf(pr)}</option>{/each}
      </select>
    </label>
    <label>
      Price a month (USD)
      <input type="number" min="0" step="0.01" required bind:value={editing.monthly_usd} aria-describedby="{id}-price" />
    </label>
    <label>Renews on day <input type="number" min="1" max="31" required bind:value={editing.renews_day} /></label>
    <p class="hint" id="{id}-price">Use the price on your own bill; the suggestions are typical prices and may be out of date.</p>
    {#if error}<p class="error" role="alert">{error}</p>{/if}
    <div class="buttons">
      <button type="submit" class="primary">Save plan</button>
      <button type="button" class="link" onclick={() => ((editing = null), (error = null))}>Cancel</button>
    </div>
  </form>
{:else}
  <button class="primary add" onclick={() => (editing = blank())}>Add a plan</button>
{/if}

<style>
  .plan {
    padding: 14px 0;
    border-bottom: 1px solid var(--border);
  }

  .plan:first-child {
    padding-top: 0;
  }

  header {
    display: flex;
    justify-content: space-between;
    gap: 12px;
  }

  h3 {
    margin: 0;
    font-size: 1.02rem;
  }

  .meta,
  .note,
  .hint,
  .empty {
    font-size: 0.82rem;
    color: var(--text-muted);
  }

  p {
    margin: 8px 0 0;
    font-size: 0.88rem;
    color: var(--text-secondary);
  }

  strong {
    color: var(--text-primary);
  }

  .compare {
    display: grid;
    grid-template-columns: auto 1fr auto;
    align-items: center;
    gap: 6px 10px;
    margin-top: 12px;
    font-size: 0.8rem;
    color: var(--text-secondary);
  }

  .bar {
    height: 8px;
    border-radius: 999px;
    background: var(--accent-soft);
    overflow: hidden;
  }

  .bar div {
    height: 100%;
    border-radius: 999px;
    transition: width 0.8s var(--ease);
  }

  .pay {
    background: var(--kind-1);
  }

  .api {
    background: var(--kind-3);
  }

  .add {
    margin-top: 14px;
  }

  .error {
    color: var(--text-primary);
  }
</style>
