<script lang="ts">
  import type { Snippet } from 'svelte'
  import { slide } from 'svelte/transition'
  import { EXPLAIN, type ExplainKey } from './explain'
  import { tweenOptions } from './motion'

  let {
    title,
    explain,
    index = 0,
    actions,
    children,
  }: { title: string; explain?: ExplainKey; index?: number; actions?: Snippet; children: Snippet } = $props()

  let open = $state(false)
  const id = $props.id()
  const e = $derived(explain ? EXPLAIN[explain] : null)
</script>

<section class="card" style:--i={index} aria-labelledby="{id}-title">
  <header>
    <h2 id="{id}-title">{title}</h2>
    <div class="actions">
      {@render actions?.()}
      {#if e}
        <button
          class="explain"
          class:open
          aria-expanded={open}
          aria-controls="{id}-explain"
          onclick={() => (open = !open)}
          title="What does this mean?"
        >
          <span aria-hidden="true">?</span>
          <span class="visually-hidden">What does {title} mean?</span>
        </button>
      {/if}
    </div>
  </header>
  {#if e && open}
    <div class="explanation" id="{id}-explain" transition:slide={tweenOptions(320)}>
      <p><strong>What it is.</strong> {e.what}</p>
      <p><strong>Why it matters.</strong> {e.why}</p>
      {#if 'like' in e && e.like}<p class="like">{e.like}</p>{/if}
    </div>
  {/if}
  {@render children()}
</section>

<style>
  .card {
    position: relative;
    min-width: 0;
    padding: 20px 22px 22px;
    border: 1px solid var(--border);
    border-radius: var(--radius);
    background: var(--surface);
    backdrop-filter: blur(24px) saturate(1.4);
    box-shadow: var(--shadow);
    animation: rise 0.7s var(--ease) both;
    animation-delay: calc(var(--i) * 60ms);
    transition:
      transform 0.4s var(--ease),
      box-shadow 0.4s var(--ease);
  }

  .card:hover {
    transform: translateY(-2px);
  }

  @keyframes rise {
    from {
      opacity: 0;
      transform: translateY(14px) scale(0.985);
    }
  }

  header {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 12px;
    margin-bottom: 14px;
  }

  h2 {
    margin: 0;
    font-size: 0.95rem;
    font-weight: 600;
    color: var(--text-secondary);
  }

  .actions {
    display: flex;
    align-items: center;
    gap: 8px;
  }

  .explain {
    display: grid;
    place-items: center;
    width: 28px;
    height: 28px;
    border: 1px solid var(--border);
    border-radius: 50%;
    background: var(--surface-raised);
    color: var(--text-secondary);
    font-weight: 600;
    font-size: 0.85rem;
    transition:
      background 0.25s,
      color 0.25s,
      transform 0.3s var(--ease);
  }

  .explain:hover,
  .explain.open {
    background: var(--accent-soft);
    color: var(--accent);
  }

  .explain.open {
    transform: rotate(360deg);
  }

  .explanation {
    margin: -4px 0 16px;
    padding: 12px 14px;
    border-radius: 14px;
    background: var(--accent-soft);
    font-size: 0.9rem;
    color: var(--text-primary);
  }

  .explanation p {
    margin: 0 0 6px;
  }

  .explanation p:last-child {
    margin: 0;
  }

  .like {
    color: var(--text-secondary);
    font-style: italic;
  }
</style>
