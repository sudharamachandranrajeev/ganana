<script lang="ts">
  import * as api from './api'

  let {
    thresholds,
    resetAlerts,
    onchange,
  }: { thresholds: number[]; resetAlerts: boolean; onchange: () => void } = $props()

  const LEVELS = [50, 75, 90, 100]

  async function toggle(level: number) {
    const next = thresholds.includes(level) ? thresholds.filter((t) => t !== level) : [...thresholds, level]
    await api.setThresholds(next)
    onchange()
  }

  async function toggleReset() {
    await api.setResetAlerts(!resetAlerts)
    onchange()
  }
</script>

<p>Notify me when a usage window reaches:</p>
<div class="chips" role="group" aria-label="Alert levels">
  {#each LEVELS as level (level)}
    <button role="checkbox" aria-checked={thresholds.includes(level)} class:on={thresholds.includes(level)} onclick={() => toggle(level)}>
      <svg viewBox="0 0 16 16" aria-hidden="true"><path d="M3.5 8.5 6.5 11.5 12.5 4.5" /></svg>
      {level === 100 ? 'Full' : `${level}%`}
    </button>
  {/each}
</div>
<button class="reset" role="checkbox" aria-checked={resetAlerts} class:on={resetAlerts} onclick={toggleReset}>
  <svg viewBox="0 0 16 16" aria-hidden="true"><path d="M3.5 8.5 6.5 11.5 12.5 4.5" /></svg>
  Also tell me when a window I was warned about resets
</button>
<p class="note">
  {#if thresholds.length}
    One notification per level per window. Closing the window keeps Gaṇanā running in the tray, so alerts still arrive.
  {:else}
    Alerts are off.
  {/if}
</p>

<style>
  .reset {
    margin-bottom: 12px;
    border-radius: 14px;
    text-align: left;
  }

  p {
    margin: 0;
    font-size: 0.88rem;
    color: var(--text-secondary);
  }

  .chips {
    display: flex;
    flex-wrap: wrap;
    gap: 8px;
    margin: 10px 0 12px;
  }

  button {
    display: inline-flex;
    align-items: center;
    gap: 6px;
    padding: 6px 12px;
    border: 1px solid var(--border);
    border-radius: 999px;
    background: var(--surface-raised);
    color: var(--text-secondary);
    font-size: 0.88rem;
    transition:
      background 0.25s var(--ease),
      color 0.25s var(--ease);
  }

  button.on {
    background: var(--accent-soft);
    color: var(--text-primary);
    font-weight: 600;
  }

  svg {
    width: 14px;
    height: 14px;
    fill: none;
    stroke: var(--accent);
    stroke-width: 2;
    stroke-linecap: round;
    stroke-linejoin: round;
    opacity: 0;
    transform: scale(0.6);
    transition:
      opacity 0.25s var(--ease),
      transform 0.3s var(--ease);
  }

  .on svg {
    opacity: 1;
    transform: none;
  }

  .note {
    font-size: 0.8rem;
    color: var(--text-muted);
  }
</style>
