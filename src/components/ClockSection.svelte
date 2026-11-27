<script lang="ts">
  import { onMount } from "svelte";
  import { ipc, type ClockInfo } from "../lib/ipc";

  let info = $state<ClockInfo | null>(null);
  let busy = $state(false);

  onMount(() => {
    void ipc.getClock().then((c) => (info = c));
  });

  async function follow(card: string) {
    busy = true;
    try {
      await ipc.clockFix(card, null);
    } finally {
      busy = false;
    }
  }
</script>

<div class="label">Clock</div>
<div class="hint">
  When devices play or record together, one sets the clock and the others follow it. Some (wireless USB headsets
  above all) cannot follow cleanly and crackle or sound like a robot; linuxmeeter watches for that and offers to let
  them set the clock — or, when what they follow sets the clock on purpose, a larger buffer.
  {#if info && !info.available}<span class="warn">Detection is off: <code>pw-top</code> was not found.</span>{/if}
</div>
{#if info && info.leaders.length + info.buffered.length}
  <div class="list">
    {#each [...info.leaders.map((c) => ({ card: c, tag: "sets the clock" })), ...info.buffered.map((c) => ({ card: c, tag: "larger buffer" }))] as { card, tag } (card)}
      <div class="row">
        <span class="card" title={card}>{card}</span>
        <span class="tag">{tag}</span>
        <button class="cmd" disabled={busy} title="Remove linuxmeeter's rule; restarts PipeWire and linuxmeeter" onclick={() => follow(card)}>
          UNDO
        </button>
      </div>
    {/each}
  </div>
{/if}

<style>
  .label {
    font-size: 12px;
    font-weight: 600;
    color: var(--text-1);
  }
  .hint {
    margin-top: -6px;
    font-size: 11px;
    line-height: 1.45;
    color: var(--text-2);
  }
  .warn {
    color: var(--accent);
  }
  code {
    font: 10px var(--mono);
  }
  .list {
    display: flex;
    flex-direction: column;
    gap: 4px;
  }
  .row {
    display: flex;
    align-items: center;
    gap: 8px;
  }
  .card {
    flex: 1;
    min-width: 0;
    font: 10px var(--mono);
    color: var(--text-1);
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
  }
  .tag {
    font-size: 10px;
    color: var(--accent);
  }
  .cmd {
    height: 22px;
    padding: 0 8px;
    font: 500 10px var(--mono);
    letter-spacing: 0.06em;
    background: var(--bg-3);
    color: var(--text-2);
    border: 1px solid var(--border-0);
    border-radius: 2px;
  }
  .cmd:hover {
    background: var(--bg-4);
    color: var(--text-1);
  }
</style>
