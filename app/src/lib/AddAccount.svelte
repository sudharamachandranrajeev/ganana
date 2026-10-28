<script lang="ts">
  import { slide } from 'svelte/transition'
  import * as api from './api'
  import { tweenOptions } from './motion'

  let { onclose }: { onclose: () => void } = $props()

  type Kind = 'anthropic' | 'openai' | 'openrouter'
  const KINDS: Record<Kind, { label: string; name: string; prefix: string; steps: string[] }> = {
    anthropic: {
      label: 'Anthropic API (Claude)',
      name: 'Anthropic API',
      prefix: 'sk-ant-admin01-',
      steps: [
        'Sign in to the Claude Console as an organisation admin.',
        'Open Settings, then Admin keys, and create a key.',
        'Copy it and paste it below. It starts with sk-ant-admin01-.',
      ],
    },
    openai: {
      label: 'OpenAI API',
      name: 'OpenAI API',
      prefix: 'sk-admin-',
      steps: [
        'Sign in to platform.openai.com as an organisation owner.',
        'Open Settings, then Organization, then Admin keys, and create a key.',
        'Copy it and paste it below. It starts with sk-admin-.',
      ],
    },
    openrouter: {
      label: 'OpenRouter',
      name: 'OpenRouter',
      prefix: '',
      steps: [
        'Sign in to openrouter.ai.',
        'Open Settings, then Management keys, and create a key.',
        'Copy it and paste it below. A normal API key cannot read activity.',
      ],
    },
  }

  let kind = $state<Kind>('anthropic')
  let name = $state('')
  let key = $state('')
  let busy = $state(false)
  let error = $state<string | null>(null)
  const id = $props.id()
  const k = $derived(KINDS[kind])
  const wrongPrefix = $derived(k.prefix !== '' && key.trim().length > 6 && !key.trim().startsWith(k.prefix))

  async function save(e: SubmitEvent) {
    e.preventDefault()
    busy = true
    try {
      await api.addAccount(kind, name.trim() || k.name, key)
      key = ''
      error = null
      onclose()
    } catch (err) {
      error = String(err)
    } finally {
      busy = false
    }
  }
</script>

<form class="form" onsubmit={save} transition:slide={tweenOptions(300)}>
  <label>
    Provider
    <select bind:value={kind} disabled={busy}>
      {#each Object.entries(KINDS) as [value, item] (value)}<option {value}>{item.label}</option>{/each}
    </select>
  </label>
  <label>Name <input bind:value={name} placeholder={k.name} disabled={busy} /></label>
  <ol class="hint">
    {#each k.steps as step (step)}<li>{step}</li>{/each}
  </ol>
  <label class="key">
    Key
    <input
      type="password"
      required
      autocomplete="off"
      spellcheck="false"
      bind:value={key}
      disabled={busy}
      aria-describedby="{id}-key"
    />
  </label>
  <p class="hint" id="{id}-key">
    {#if wrongPrefix}
      This does not look like an admin key: those start with {k.prefix}.
    {:else}
      The key goes into your system keyring, never into Gaṇanā's files, and is only used to read usage and cost.
      Gaṇanā shows its last four characters so you can tell keys apart.
    {/if}
  </p>
  {#if error}<p class="error" role="alert">{error}</p>{/if}
  <div class="buttons">
    <button type="submit" class="primary" disabled={busy}>{busy ? 'Checking the key…' : 'Add account'}</button>
    <button type="button" class="link" disabled={busy} onclick={onclose}>Cancel</button>
  </div>
</form>

<style>
  ol {
    margin: 0;
    padding-left: 20px;
  }

  li + li {
    margin-top: 2px;
  }

  .key {
    grid-column: 1 / -1;
  }
</style>
