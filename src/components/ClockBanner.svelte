<script lang="ts">
  /** "<device> keeps dropping out" — with a one-click fix. */
  import { onMount } from "svelte";
  import { ipc, type ClockProblem } from "../lib/ipc";

  let problems = $state<ClockProblem[]>([]);
  let confirming = $state<string | null>(null);
  let busy = $state(false);
  let error = $state<string | null>(null);

  onMount(() => {
    void ipc.getClock().then((c) => (problems = c?.problems ?? []));
    return ipc.onClockProblem((p) => {
      if (!problems.some((q) => q.card === p.card)) problems = [...problems, p];
    });
  });

  async function fix(p: ClockProblem) {
    busy = true;
    error = null;
    const card = p.card;
    try {
      await ipc.clockFix(card, p.remedy);
      // linuxmeeter restarts; nothing after this normally runs.
      problems = problems.filter((p) => p.card !== card);
    } catch (e) {
      error = String(e);
    } finally {
      busy = false;
      confirming = null;
    }
  }

  function dismiss(card: string) {
    problems = problems.filter((p) => p.card !== card);
    void ipc.dismissClock(card);
  }
</script>

{#each problems as p (p.card)}
  <div class="banner" role="alert">
    <svg width="14" height="14" viewBox="0 0 16 16" fill="none" aria-hidden="true">
      <path d="M8 1.5l6.5 12h-13z" stroke="currentColor" stroke-width="1.3" stroke-linejoin="round" />
      <path d="M8 6v3.5M8 11.5v.5" stroke="currentColor" stroke-width="1.4" stroke-linecap="round" />
    </svg>
    <div class="text">
      {#if confirming === p.card}
        {p.remedy === "buffer" ? "Give" : "Let"} <b>{p.description}</b>
        {p.remedy === "buffer" ? "a larger buffer" : "set the clock"}? PipeWire and linuxmeeter restart — audio stops for
        a few seconds.
      {:else if p.remedy === "buffer"}
        <b>{p.description}</b> keeps dropping out ({p.errors} errors in 10 s) following the clock of
        <b>{p.follows ?? "another device"}</b>, which sets the clock on purpose. A larger buffer lets it follow
        cleanly (a little more delay on this device only).
      {:else}
        <b>{p.description}</b> keeps dropping out ({p.errors} errors in 10 s): it follows
        {p.follows ? `the clock of ${p.follows}` : "another device's clock"}. Letting it set the clock usually fixes
        crackling and robot voice.
      {/if}
      {#if error}<span class="err">{error}</span>{/if}
    </div>
    {#if confirming === p.card}
      <button class="btn primary" disabled={busy} onclick={() => fix(p)}>RESTART AUDIO</button>
      <button class="btn" disabled={busy} onclick={() => (confirming = null)}>CANCEL</button>
    {:else}
      <button class="btn primary" onclick={() => (confirming = p.card)}>
        {p.remedy === "buffer" ? "LARGER BUFFER" : "LET IT SET THE CLOCK"}
      </button>
      <button class="btn" onclick={() => dismiss(p.card)}>NOT NOW</button>
    {/if}
  </div>
{/each}

<style>
  .banner {
    flex: none;
    display: flex;
    align-items: center;
    gap: 10px;
    padding: 6px 12px;
    background: var(--accent-glow);
    border-bottom: 1px solid var(--accent);
    color: var(--accent);
  }
  .text {
    flex: 1;
    min-width: 0;
    font-size: 11px;
    line-height: 1.4;
    color: var(--text-1);
  }
  .text b {
    color: var(--accent);
  }
  .err {
    margin-left: 6px;
    color: var(--mute);
  }
  .btn {
    flex: none;
    height: 22px;
    padding: 0 8px;
    font: 500 10px var(--mono);
    letter-spacing: 0.06em;
    background: var(--bg-3);
    color: var(--text-2);
    border: 1px solid var(--border-0);
    border-radius: 2px;
  }
  .btn:hover {
    background: var(--bg-4);
    color: var(--text-1);
  }
  .btn.primary {
    background: var(--accent);
    color: var(--on-accent);
    border-color: var(--accent);
  }
  .btn.primary:hover {
    background: var(--accent-hi);
  }
</style>
