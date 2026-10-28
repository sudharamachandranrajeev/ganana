<script lang="ts">
  import type { Slice } from './api'
  import { providerColor } from './colors'
  import { compact, money, nameOf, percent } from './format'

  let {
    slices,
    metric,
    limit = 6,
    colorBy,
  }: { slices: Slice[]; metric: 'cost' | 'tokens'; limit?: number; colorBy?: 'provider' } = $props()

  let expanded = $state(false)
  const value = (s: Slice) => (metric === 'cost' ? s.cost : s.total_tokens)
  const sorted = $derived(slices.toSorted((a, b) => value(b) - value(a)))
  const rows = $derived(expanded ? sorted : sorted.slice(0, limit))
  const max = $derived(Math.max(0, ...sorted.map(value)))
  const total = $derived(sorted.reduce((n, s) => n + value(s), 0))
</script>

{#if sorted.length === 0}
  <p class="empty">Nothing yet.</p>
{:else}
  <ul>
    {#each rows as s (s.name + (s.provider ?? ''))}
      {@const v = value(s)}
      <li title="{compact(s.total_tokens)} tokens · {compact(s.calls)} calls · {money(s.cost)}">
        <div class="label">
          {#if colorBy === 'provider' && s.provider}<i style:background={providerColor(s.provider)} aria-hidden="true"></i>{/if}
          <span class="name">{nameOf(s.name)}</span>
          <span class="value num">
            {#if metric === 'cost' && s.cost === 0 && s.unpriced_tokens > 0}
              <span class="unpriced">no price</span>
            {:else}
              {metric === 'cost' ? money(v) : compact(v)}
            {/if}
            <span class="share">{percent(v, total)}</span>
          </span>
        </div>
        <div class="track">
          <div
            class="bar"
            style:width="{max ? Math.max(v > 0 ? 1.5 : 0, (v / max) * 100) : 0}%"
            style:background={colorBy === 'provider' && s.provider ? providerColor(s.provider) : 'var(--accent)'}
          ></div>
        </div>
      </li>
    {/each}
  </ul>
  {#if sorted.length > limit}
    <button class="more" onclick={() => (expanded = !expanded)}>
      {expanded ? 'Show fewer' : `Show all ${sorted.length}`}
    </button>
  {/if}
{/if}

<style>
  ul {
    list-style: none;
    margin: 0;
    padding: 0;
    display: grid;
    gap: 12px;
  }

  .label {
    display: flex;
    align-items: baseline;
    gap: 8px;
    font-size: 0.88rem;
    margin-bottom: 5px;
  }

  i {
    flex: none;
    width: 10px;
    height: 10px;
    border-radius: 3px;
    align-self: center;
  }

  .name {
    min-width: 0;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .value {
    margin-left: auto;
    white-space: nowrap;
    font-weight: 600;
  }

  .share {
    display: inline-block;
    min-width: 3.2em;
    text-align: right;
    font-weight: 400;
    color: var(--text-muted);
  }

  .unpriced {
    font-weight: 400;
    color: var(--warning-ink);
  }

  .track {
    height: 8px;
    border-radius: 4px;
    background: var(--grid);
    overflow: hidden;
  }

  .bar {
    height: 100%;
    border-radius: 4px;
    transition: width 0.8s var(--ease);
    animation: grow 0.9s var(--ease);
  }

  @keyframes grow {
    from {
      width: 0;
    }
  }

  .empty {
    margin: 0;
    color: var(--text-muted);
  }

  .more {
    margin-top: 12px;
    border: 0;
    background: none;
    color: var(--accent);
    font-size: 0.85rem;
    padding: 0;
  }
</style>
