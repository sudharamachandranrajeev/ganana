<script lang="ts">
  import { slide } from 'svelte/transition'
  import * as api from './api'
  import { compact, relativeTime, whole } from './format'
  import { tweenOptions } from './motion'

  let { now }: { now: number } = $props()

  let data = $state<api.Sharing | null>(null)
  let editing = $state(false)
  let folder = $state('')
  let device = $state('')
  let busy = $state(false)
  let error = $state<string | null>(null)

  async function load() {
    try {
      data = await api.sharing()
    } catch (e) {
      error = String(e)
    }
  }

  $effect(() => {
    load()
    const unlisten = api.onUsageUpdated(load)
    return () => void unlisten.then((f) => f())
  })

  function edit() {
    folder = data?.folder ?? ''
    device = data?.device ?? ''
    editing = true
  }

  async function save(next: string | null, e?: SubmitEvent) {
    e?.preventDefault()
    busy = true
    try {
      data = await api.setSharing(next, device || data?.device || '')
      editing = false
      error = null
    } catch (err) {
      error = String(err)
    } finally {
      busy = false
    }
  }

  const others = $derived(data?.devices.filter((d) => !d.this) ?? [])
  const problems = $derived(data?.last_report?.errors ?? [])
</script>

{#if data && (editing || !data.folder)}
  {#if !data.folder && !editing}
    <p>
      Gaṇanā only sees this computer. To count all of them, pick a folder that Syncthing, Dropbox or a similar app keeps the
      same on every computer, and turn this on with that folder on each one.
    </p>
    <div class="buttons"><button class="primary" onclick={edit}>Share usage between computers</button></div>
  {:else}
    <form class="form" onsubmit={(e) => save(folder, e)} transition:slide={tweenOptions(300)}>
      <label class="span">
        Synced folder
        <input bind:value={folder} required placeholder="The full path of a synced folder" disabled={busy} />
      </label>
      <label>This computer's name <input bind:value={device} disabled={busy} /></label>
      <p class="hint span">
        Only numbers go into the folder: token counts, model names, times, project folder names and limit readings.
        Never your prompts or keys.
      </p>
      {#if error}<p class="error span" role="alert">{error}</p>{/if}
      <div class="buttons span">
        <button type="submit" class="primary" disabled={busy}>{busy ? 'Sharing…' : 'Share'}</button>
        <button type="button" class="link" disabled={busy} onclick={() => (editing = false)}>Cancel</button>
      </div>
    </form>
  {/if}
{:else if data?.folder}
  <p>
    This computer shares its usage as <strong>{data.device}</strong> through <code>{data.folder}</code>.
    {#if data.last_sync}Last checked {relativeTime(data.last_sync, now)}.{/if}
  </p>
  {#if others.length}
    <ul>
      {#each data.devices as d (d.name + d.this)}
        <li>
          <span class="name">{d.name}{d.this ? ' (this computer)' : ''}</span>
          <span>{whole(d.calls)} calls · {compact(d.tokens)} tokens · last {relativeTime(d.last_ts, now)}</span>
        </li>
      {/each}
    </ul>
  {:else}
    <p class="note">No other computer has written to the folder yet. Turn sharing on there with the same folder.</p>
  {/if}
  {#each problems as p (p)}<p class="error" role="alert">{p}</p>{/each}
  {#if error}<p class="error" role="alert">{error}</p>{/if}
  <div class="buttons">
    <button class="link" disabled={busy} onclick={edit}>Change</button>
    <button class="link" disabled={busy} onclick={() => save(null)}>Stop sharing</button>
  </div>
  <p class="note">Stopping removes the other computers' usage from this one. Their files stay in the folder.</p>
{/if}

<style>
  p {
    margin: 0 0 10px;
    font-size: 0.88rem;
    color: var(--text-secondary);
  }

  .note {
    font-size: 0.8rem;
    color: var(--text-muted);
  }

  .error {
    color: var(--status-critical);
  }

  .span {
    grid-column: 1 / -1;
  }

  code {
    font-size: 0.85em;
    word-break: break-all;
  }

  ul {
    margin: 0 0 12px;
    padding: 0;
    list-style: none;
  }

  li {
    display: flex;
    flex-wrap: wrap;
    justify-content: space-between;
    gap: 4px 16px;
    padding: 8px 0;
    border-bottom: 1px solid var(--border);
    font-size: 0.88rem;
    color: var(--text-secondary);
  }

  .name {
    color: var(--text-primary);
    font-weight: 600;
  }
</style>
