<script lang="ts">
  import * as api from './api'
  import type { Relay } from './api'

  let { relay, waiting, onchange }: { relay: Relay; waiting: boolean; onchange: (r: Relay) => void } = $props()

  let busy = $state(false)
  let error = $state<string | null>(null)

  async function run(action: () => Promise<Relay>) {
    busy = true
    try {
      onchange(await action())
      error = null
    } catch (e) {
      error = String(e)
    } finally {
      busy = false
    }
  }
</script>

{#if relay.state === 'off'}
  <p>
    Claude Code reports your Pro/Max limits only to its status line. Connect Gaṇanā as that status line to see them
    here. It keeps only the two limit numbers.
  </p>
  <p class="detail">
    This changes the <code>statusLine</code> setting in <code>{relay.settings}</code>, after saving a backup.
    {#if relay.existing}
      Your current status line (<code>{relay.existing}</code>) keeps working: Gaṇanā runs it and shows its output.
    {/if}
  </p>
  <button class="primary" disabled={busy} onclick={() => run(api.relayInstall)}>Connect Claude</button>
{:else}
  <p>
    {#if waiting}
      Connected. Your Claude limits appear after Claude Code's next reply.
    {:else}
      Connected through Claude Code's status line.
    {/if}
    {#if relay.wraps}Your own status line (<code>{relay.wraps}</code>) still runs.{/if}
  </p>
  <button class="link" disabled={busy} onclick={() => run(api.relayUninstall)}>Disconnect and restore the old setting</button>
{/if}
{#if error}<p class="error" role="alert">Could not change the setting: {error}</p>{/if}

<style>
  p {
    max-width: 75ch;
    margin: 0 0 10px;
    font-size: 0.88rem;
    color: var(--text-secondary);
  }

  .detail {
    font-size: 0.82rem;
    color: var(--text-muted);
  }

  code {
    padding: 1px 5px;
    border-radius: 6px;
    background: var(--accent-soft);
    font-size: 0.8rem;
    overflow-wrap: anywhere;
  }

  .error {
    margin-top: 10px;
    color: var(--text-primary);
  }
</style>
