<script lang="ts">
  /** "+" beside a group label: adds directly, or opens a small menu of choices. */
  interface Choice {
    label: string;
    disabled?: boolean;
    onpick: () => void;
  }
  interface Props {
    title: string;
    choices?: Choice[];
    onadd?: () => void;
    disabled?: boolean;
  }
  let { title, choices, onadd, disabled = false }: Props = $props();

  let open = $state(false);
  let root: HTMLDivElement;

  function onWindowPointerDown(e: PointerEvent) {
    if (open && root && !root.contains(e.target as Node)) open = false;
  }
  function click() {
    if (choices) open = !open;
    else onadd?.();
  }
</script>

<svelte:window onpointerdown={onWindowPointerDown} />

<div class="wrap" bind:this={root}>
  <button class="add" {title} {disabled} aria-haspopup={choices ? "menu" : undefined} onclick={click}>
    <svg width="9" height="9" viewBox="0 0 10 10"><path d="M5 1v8M1 5h8" stroke="currentColor" stroke-width="1.4" /></svg>
  </button>
  {#if open && choices}
    <div class="menu" role="menu">
      {#each choices as c}
        <button
          class="item"
          role="menuitem"
          disabled={c.disabled}
          onclick={() => {
            open = false;
            c.onpick();
          }}
        >
          {c.label}
        </button>
      {/each}
    </div>
  {/if}
</div>

<style>
  .wrap {
    position: relative;
    display: inline-flex;
  }
  .add {
    width: 18px;
    height: 16px;
    display: flex;
    align-items: center;
    justify-content: center;
    color: var(--text-3);
    border: 1px solid var(--border-0);
    border-radius: 2px;
    background: var(--bg-3);
  }
  .add:hover:not(:disabled) {
    color: var(--accent);
    border-color: var(--accent);
  }
  .add:disabled {
    opacity: 0.4;
    cursor: default;
  }
  .menu {
    position: absolute;
    top: calc(100% + 2px);
    left: 0;
    z-index: 30;
    min-width: 190px;
    background: var(--bg-3);
    border: 1px solid var(--border-1);
    border-radius: 6px;
    box-shadow: 0 8px 24px rgba(0, 0, 0, 0.6);
    padding: 3px;
    display: flex;
    flex-direction: column;
  }
  .item {
    padding: 5px 8px;
    font-size: 11px;
    color: var(--text-2);
    text-align: left;
    border-radius: 2px;
    white-space: nowrap;
    text-transform: none;
    letter-spacing: 0;
  }
  .item:hover:not(:disabled) {
    background: var(--bg-4);
    color: var(--text-1);
  }
  .item:disabled {
    color: var(--text-3);
    cursor: default;
  }
</style>
