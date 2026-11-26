<script lang="ts">
  import { isHardwareBus, type BusId } from "../lib/types";
  import { mixer } from "../lib/state/mixer.svelte";

  interface Props {
    routes: Record<BusId, boolean>;
    ontoggle: (bus: BusId, on: boolean) => void;
  }
  let { routes, ontoggle }: Props = $props();

  // One row of A buses (hardware outputs), one of B buses (virtual mics).
  let rows = $derived(
    [mixer.buses.filter((b) => isHardwareBus(b.id)), mixer.buses.filter((b) => !isHardwareBus(b.id))].filter((r) => r.length > 0),
  );
</script>

<div class="routes">
  {#each rows as row}
    <div class="row">
      {#each row as bus (bus.id)}
        <button
          class="rbtn"
          class:active={routes[bus.id]}
          aria-pressed={!!routes[bus.id]}
          title="Send to {bus.id} {bus.label}"
          onclick={() => ontoggle(bus.id, !routes[bus.id])}
        >
          {bus.id}
        </button>
      {/each}
    </div>
  {/each}
</div>

<style>
  .routes {
    display: flex;
    flex-direction: column;
    gap: 4px;
  }
  .row {
    display: flex;
    gap: 4px;
  }
  .rbtn {
    flex: 1;
    min-width: 0;
    height: 22px;
    font: 500 10px var(--mono);
    letter-spacing: 0.06em;
    background: var(--bg-3);
    color: var(--text-3);
    border: 1px solid var(--border-0);
    border-radius: 2px;
    box-shadow: inset 0 1px 0 var(--hl-top);
  }
  .rbtn:hover {
    background: var(--bg-4);
    color: var(--text-2);
  }
  .rbtn.active {
    background: var(--accent-glow);
    color: var(--accent);
    border-color: var(--accent);
  }
</style>
