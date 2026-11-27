<script lang="ts">
  interface Option {
    value: string;
    label: string;
  }
  interface Props {
    value: string | null;
    display: string;
    options: Option[];
    onchange?: (value: string) => void;
    disabled?: boolean;
    /** An on/off option shown under the devices (e.g. stable buffer). */
    toggle?: { label: string; title: string; on: boolean; ontoggle: (on: boolean) => void };
  }
  let { value, display, options, onchange, disabled = false, toggle }: Props = $props();

  let open = $state(false);
  let root: HTMLDivElement;

  function onWindowPointerDown(e: PointerEvent) {
    if (open && root && !root.contains(e.target as Node)) open = false;
  }
</script>

<svelte:window onpointerdown={onWindowPointerDown} />

<div class="wrap" bind:this={root}>
  <button
    class="dev"
    {disabled}
    aria-haspopup="listbox"
    aria-expanded={open}
    onclick={() => {
      if (!disabled) open = !open;
    }}
  >
    <span>{display}</span>
    {#if toggle?.on}<span class="badge" title={toggle.title}>1024</span>{/if}
    <svg width="10" height="10" viewBox="0 0 10 10" fill="none">
      <path d="M2.5 4l2.5 2.5L7.5 4" stroke="currentColor" stroke-width="1.25" />
    </svg>
  </button>
  {#if open}
    <div class="menu" role="listbox">
      {#each options as opt}
        <button
          class="item"
          role="option"
          aria-selected={opt.value === value}
          class:selected={opt.value === value}
          onclick={() => {
            open = false;
            onchange?.(opt.value);
          }}
        >
          {opt.label}
        </button>
      {/each}
      {#if toggle}
        <div class="sep"></div>
        <button
          class="item check"
          role="menuitemcheckbox"
          aria-checked={toggle.on}
          title={toggle.title}
          onclick={() => {
            open = false;
            toggle.ontoggle(!toggle.on);
          }}
        >
          <span class="box" class:on={toggle.on}>{toggle.on ? "✓" : ""}</span>{toggle.label}
        </button>
      {/if}
    </div>
  {/if}
</div>

<style>
  .wrap {
    position: relative;
  }
  .dev {
    width: 100%;
    height: 22px;
    padding: 0 6px;
    display: flex;
    align-items: center;
    gap: 4px;
    background: var(--bg-3);
    border: 1px solid var(--border-0);
    border-radius: 2px;
    box-shadow: inset 0 1px 0 var(--hl-top);
    font-size: 11px;
    color: var(--text-2);
    text-align: left;
  }
  .dev:hover:not(:disabled) {
    background: var(--bg-4);
    color: var(--text-1);
  }
  .dev:disabled {
    opacity: 0.5;
    cursor: default;
  }
  .dev span {
    flex: 1;
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
  }
  .dev svg {
    flex: none;
    color: var(--text-3);
  }
  .menu {
    position: absolute;
    top: calc(100% + 2px);
    left: 0;
    min-width: 100%;
    max-width: 220px;
    z-index: 30;
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
    overflow: hidden;
    text-overflow: ellipsis;
  }
  .item:hover {
    background: var(--bg-4);
    color: var(--text-1);
  }
  .item.selected {
    color: var(--accent);
  }
  .sep {
    height: 1px;
    margin: 3px 2px;
    background: var(--border-0);
  }
  .item.check {
    display: flex;
    align-items: center;
    gap: 6px;
  }
  .box {
    width: 12px;
    height: 12px;
    flex: none;
    display: flex;
    align-items: center;
    justify-content: center;
    border: 1px solid var(--border-1);
    border-radius: 2px;
    font-size: 9px;
    color: var(--on-accent);
  }
  .box.on {
    background: var(--accent);
    border-color: var(--accent);
  }
  .dev .badge {
    flex: none;
    font: 500 8px var(--mono);
    color: var(--accent);
    border: 1px solid var(--accent);
    border-radius: 2px;
    padding: 0 2px;
  }
</style>
