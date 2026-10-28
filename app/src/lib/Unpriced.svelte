<script lang="ts">
  import { aliases, priceModels, setAlias, type Unpriced } from './api'
  import { compact, nameOf } from './format'

  let { unpriced, onchange }: { unpriced: Unpriced[]; onchange: () => void } = $props()

  let models = $state<string[]>([])
  let mapped = $state<Record<string, string>>({})
  let choice = $state<Record<string, string>>({})
  let error = $state<string | null>(null)
  const listId = $props.id()

  async function load() {
    ;[models, mapped] = await Promise.all([priceModels(), aliases()])
  }
  $effect(() => {
    void unpriced
    load().catch((e) => (error = String(e)))
  })

  async function save(alias: string, target: string | null) {
    error = null
    try {
      await setAlias(alias, target)
      await load()
      onchange()
    } catch (e) {
      error = String(e)
    }
  }
</script>

{#if unpriced.length === 0 && Object.keys(mapped).length === 0}
  <p class="ok">Every model you used has a known price.</p>
{/if}

{#if unpriced.length > 0}
  <ul>
    {#each unpriced as u (u.provider + u.model)}
      <li>
        <div class="who">
          <strong>{u.model}</strong>
          <span>{nameOf(u.provider)} · {compact(u.total_tokens)} tokens · {compact(u.calls)} calls</span>
        </div>
        <form
          onsubmit={(e) => {
            e.preventDefault()
            if (choice[u.model]) save(u.model, choice[u.model])
          }}
        >
          <label class="visually-hidden" for="{listId}-{u.model}">Real model behind {u.model}</label>
          <input
            id="{listId}-{u.model}"
            list={listId}
            placeholder="Which real model is it?"
            bind:value={choice[u.model]}
            autocomplete="off"
          />
          <button type="submit" disabled={!models.includes(choice[u.model] ?? '')}>Price it</button>
        </form>
      </li>
    {/each}
  </ul>
  <datalist id={listId}>
    {#each models as m (m)}<option value={m}></option>{/each}
  </datalist>
{/if}

{#if Object.keys(mapped).length > 0}
  <h3>Your mappings</h3>
  <ul class="mapped">
    {#each Object.entries(mapped) as [alias, target] (alias)}
      <li>
        <span><strong>{alias}</strong> is priced as <strong>{target}</strong></span>
        <button class="link" onclick={() => save(alias, null)}>Undo</button>
      </li>
    {/each}
  </ul>
{/if}

{#if error}<p class="error" role="alert">{error}</p>{/if}

<style>
  ul {
    list-style: none;
    margin: 0;
    padding: 0;
    display: grid;
    gap: 14px;
  }

  li {
    display: grid;
    gap: 8px;
  }

  .who {
    display: flex;
    flex-wrap: wrap;
    align-items: baseline;
    gap: 4px 10px;
  }

  .who span {
    font-size: 0.82rem;
    color: var(--text-secondary);
  }

  form {
    display: flex;
    gap: 8px;
  }

  input {
    flex: 1;
    min-width: 0;
    padding: 8px 12px;
    border: 1px solid var(--border);
    border-radius: 10px;
    background: var(--surface-raised);
  }

  button[type='submit'] {
    padding: 8px 14px;
    border: 0;
    border-radius: 10px;
    background: var(--accent);
    color: #fff;
    font-weight: 600;
    transition: opacity 0.2s;
  }

  button[type='submit']:disabled {
    opacity: 0.4;
    cursor: default;
  }

  h3 {
    margin: 18px 0 10px;
    font-size: 0.85rem;
    color: var(--text-secondary);
  }

  .mapped li {
    display: flex;
    justify-content: space-between;
    gap: 10px;
    font-size: 0.88rem;
  }

  .link {
    border: 0;
    background: none;
    color: var(--accent);
    padding: 0;
  }

  .ok {
    margin: 0;
    color: var(--good);
  }

  .error {
    color: #d03b3b;
  }
</style>
