<script lang="ts">
  import type { Totals } from './api'
  import { EXPLAIN } from './explain'
  import { compact, percent } from './format'

  let { totals }: { totals: Totals } = $props()

  // Cheapest to priciest per token, matching the light-to-dark ramp.
  const kinds = $derived([
    { key: 'cache_read' as const, value: totals.tokens.cache_read, color: 'var(--kind-1)' },
    { key: 'input' as const, value: totals.tokens.input, color: 'var(--kind-2)' },
    { key: 'cache_write' as const, value: totals.tokens.cache_write + totals.tokens.cache_write_1h, color: 'var(--kind-3)' },
    { key: 'output' as const, value: totals.tokens.output, color: 'var(--kind-4)' },
  ])
  const sum = $derived(kinds.reduce((n, k) => n + k.value, 0))
  let focused = $state<string | null>(null)
</script>

<div class="bar" role="img" aria-label={kinds.map((k) => `${EXPLAIN[k.key].title} ${percent(k.value, sum)}`).join(', ')}>
  {#each kinds as k (k.key)}
    {#if k.value > 0}
      <div
        class="seg"
        class:dim={focused && focused !== k.key}
        style:flex-grow={k.value}
        style:background={k.color}
        title="{EXPLAIN[k.key].title}: {compact(k.value)} tokens ({percent(k.value, sum)})"
      ></div>
    {/if}
  {/each}
</div>

<ul>
  {#each kinds as k (k.key)}
    <li onmouseenter={() => (focused = k.key)} onmouseleave={() => (focused = null)}>
      <i style:background={k.color} aria-hidden="true"></i>
      <div>
        <div class="row">
          <strong>{EXPLAIN[k.key].title}</strong>
          <span class="num">{compact(k.value)} <span class="pct">{percent(k.value, sum)}</span></span>
        </div>
        <p>{EXPLAIN[k.key].what}</p>
      </div>
    </li>
  {/each}
</ul>
{#if totals.tokens.reasoning > 0}
  <p class="note">
    Of the output, {compact(totals.tokens.reasoning)} tokens ({percent(totals.tokens.reasoning, totals.tokens.output)}) were the
    model's hidden thinking.
  </p>
{/if}

<style>
  .bar {
    display: flex;
    gap: 2px;
    height: 18px;
    margin-bottom: 16px;
  }

  .seg {
    min-width: 3px;
    flex-basis: 0;
    border-radius: 4px;
    transition:
      flex-grow 0.8s var(--ease),
      opacity 0.25s;
  }

  .seg.dim {
    opacity: 0.3;
  }

  ul {
    list-style: none;
    margin: 0;
    padding: 0;
    display: grid;
    gap: 12px;
  }

  li {
    display: flex;
    gap: 10px;
  }

  i {
    flex: none;
    width: 10px;
    height: 10px;
    margin-top: 6px;
    border-radius: 3px;
  }

  li > div {
    flex: 1;
    min-width: 0;
  }

  .row {
    display: flex;
    justify-content: space-between;
    gap: 8px;
    font-size: 0.9rem;
  }

  .pct {
    display: inline-block;
    min-width: 3.2em;
    text-align: right;
    color: var(--text-muted);
  }

  p {
    margin: 2px 0 0;
    font-size: 0.82rem;
    color: var(--text-secondary);
  }

  .note {
    margin-top: 14px;
  }
</style>
