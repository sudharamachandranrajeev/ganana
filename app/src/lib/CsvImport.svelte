<script lang="ts">
  import { slide } from 'svelte/transition'
  import * as api from './api'
  import type { CsvInspect, CsvMapping } from './api'
  import { nameOf } from './format'
  import { tweenOptions } from './motion'

  /** `account` adds the file to an existing imported account. */
  let { account = null, onclose }: { account?: { id: number; name: string } | null; onclose: () => void } = $props()

  const PROVIDERS = ['google', 'typesafe', 'alibaba', 'openai', 'anthropic', 'other']
  const FIELDS: [keyof CsvMapping, string][] = [
    ['date', 'Date'],
    ['model', 'Model'],
    ['input', 'Input tokens'],
    ['output', 'Output tokens'],
    ['cost', 'Cost (USD)'],
  ]
  const LIMIT = 50 * 1024 * 1024

  let text = $state<string | null>(null)
  let file = $state('')
  let inspect = $state<CsvInspect | null>(null)
  let mapping = $state<CsvMapping>({ date: null, model: null, input: null, output: null, cost: null })
  let provider = $state('google')
  let name = $state('')
  let busy = $state(false)
  let error = $state<string | null>(null)
  const id = $props.id()

  const ready = $derived(mapping.date !== null && (mapping.input !== null || mapping.output !== null || mapping.cost !== null))

  async function pick(e: Event) {
    const f = (e.currentTarget as HTMLInputElement).files?.[0]
    inspect = null
    error = null
    if (!f) return
    if (f.size > LIMIT) {
      error = 'The file is over 50 MB.'
      return
    }
    try {
      text = await f.text()
      file = f.name
      inspect = await api.csvInspect(text)
      mapping = { ...inspect.guess }
    } catch (err) {
      error = String(err)
    }
  }

  async function save(e: SubmitEvent) {
    e.preventDefault()
    if (!text) return
    busy = true
    try {
      await api.csvImport(account?.id ?? null, name.trim() || `${nameOf(provider)} (file)`, provider, text, $state.snapshot(mapping))
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
  <label class="file">
    {account ? `Another file for ${account.name}` : 'Usage file (CSV)'}
    <input type="file" accept=".csv,text/csv" onchange={pick} disabled={busy} aria-describedby="{id}-file" />
  </label>
  <p class="hint" id="{id}-file">
    Download a usage or billing export from the provider's console. Dates must be year first (2026-10-28) or a Unix
    time, so days and months are never mixed up. The file is read on this computer only.
  </p>

  {#if inspect}
    <p class="hint">{file}: {inspect.rows} rows. Check the columns Gaṇanā picked, and fix any that are wrong.</p>
    {#each FIELDS as [field, label] (field)}
      <label>
        {label}
        <select bind:value={mapping[field]} disabled={busy}>
          <option value={null}>{field === 'model' ? 'Not in this file (one model)' : 'Not in this file'}</option>
          {#each inspect.headers as h, i (i)}<option value={i}>{h || `Column ${i + 1}`}</option>{/each}
        </select>
      </label>
    {/each}
    {#if !account}
      <label>
        Billed by
        <select bind:value={provider} disabled={busy}>
          {#each PROVIDERS as p (p)}<option value={p}>{nameOf(p)}</option>{/each}
        </select>
      </label>
      <label>Name <input bind:value={name} placeholder="{nameOf(provider)} (file)" disabled={busy} /></label>
    {/if}

    {#if inspect.sample.length}
      <div class="preview" role="region" aria-label="First rows of the file" tabindex="-1">
        <table>
          <thead>
            <tr>{#each inspect.headers as h, i (i)}<th scope="col" class:used={Object.values(mapping).includes(i)}>{h}</th>{/each}</tr>
          </thead>
          <tbody>
            {#each inspect.sample as row, r (r)}
              <tr>{#each row as cell, i (i)}<td class:used={Object.values(mapping).includes(i)}>{cell}</td>{/each}</tr>
            {/each}
          </tbody>
        </table>
      </div>
    {/if}
    {#if !ready}<p class="hint">Choose the date column and at least one of tokens or cost.</p>{/if}
  {/if}

  {#if error}<p class="error" role="alert">{error}</p>{/if}
  <div class="buttons">
    <button type="submit" class="primary" disabled={busy || !inspect || !ready}>{busy ? 'Importing…' : 'Import'}</button>
    <button type="button" class="link" disabled={busy} onclick={onclose}>Cancel</button>
  </div>
</form>

<style>
  .file,
  .preview {
    grid-column: 1 / -1;
  }

  .file input {
    padding: 6px;
  }

  .preview {
    overflow-x: auto;
    border: 1px solid var(--border);
    border-radius: 10px;
    background: var(--surface-raised);
  }

  table {
    border-collapse: collapse;
    font-size: 0.78rem;
    white-space: nowrap;
  }

  th,
  td {
    padding: 5px 10px;
    text-align: left;
    color: var(--text-muted);
  }

  th {
    border-bottom: 1px solid var(--border);
    font-weight: 600;
  }

  .used {
    color: var(--text-primary);
  }
</style>
