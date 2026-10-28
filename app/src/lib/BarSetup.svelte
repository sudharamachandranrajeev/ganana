<script lang="ts">
  import * as api from './api'

  let widget = $state<api.BarWidget | null>(null)
  let busy = $state(false)
  let error = $state<string | null>(null)

  $effect(() => {
    api.barWidgetStatus().then((w) => (widget = w), (e) => (error = String(e)))
  })

  async function run(action: () => Promise<api.BarWidget>) {
    busy = true
    try {
      widget = await action()
      error = null
    } catch (e) {
      error = String(e)
    } finally {
      busy = false
    }
  }

  const WAYBAR = `"custom/ganana": {
  "exec": "ganana bar --format waybar",
  "return-type": "json",
  "interval": 30,
  "on-click": "ganana"
}`
</script>

{#if widget?.available}
  <p>
    {#if widget.installed}
      Gaṇanā is in your Omarchy bar. The ring fills as your fullest usage window fills and turns red when it is nearly
      full. Click it for every window and its reset time; middle-click opens this window.
    {:else}
      Add a small ring to your Omarchy bar that shows how full your fullest usage window is. Click it for every window
      and its reset time; middle-click opens this window.
    {/if}
  </p>
  <div class="buttons">
    {#if widget.installed}
      <button class="link" disabled={busy} onclick={() => run(api.barWidgetRemove)}>
        {busy ? 'Removing…' : 'Remove from bar'}
      </button>
    {:else}
      <button class="primary" disabled={busy} onclick={() => run(api.barWidgetInstall)}>
        {busy ? 'Adding…' : 'Add to bar'}
      </button>
    {/if}
  </div>
  <p class="note">
    Gaṇanā adds its own widget and nothing else. You can move it with <code>omarchy bar move ganana.usage --section left</code>.
  </p>
{:else if widget}
  <p>
    Any status bar that runs a command can show your usage. <code>ganana bar --format plain</code> prints one short
    line, like “Claude 62%”, and <code>--format json</code> prints everything. For Waybar, add this module:
  </p>
  <pre>{WAYBAR}</pre>
  <p class="note">The class is ok, warning or critical, so your Waybar style can colour it.</p>
{/if}
{#if error}<p class="error" role="alert">{error}</p>{/if}

<style>
  p {
    margin: 0 0 10px;
    font-size: 0.88rem;
    color: var(--text-secondary);
  }

  .note {
    margin: 10px 0 0;
    font-size: 0.8rem;
    color: var(--text-muted);
  }

  .error {
    margin-top: 10px;
    color: var(--status-critical);
  }

  pre {
    margin: 0;
    padding: 12px 14px;
    overflow-x: auto;
    border: 1px solid var(--border);
    border-radius: 10px;
    background: var(--surface-raised);
    font-size: 0.8rem;
  }

  code {
    font-size: 0.85em;
  }
</style>
