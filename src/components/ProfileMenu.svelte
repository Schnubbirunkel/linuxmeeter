<script lang="ts">
  import { mixer } from "../lib/state/mixer.svelte";

  let open = $state(false);
  let naming = $state(false);
  let draft = $state("");
  let root: HTMLDivElement;

  function onWindowPointerDown(e: PointerEvent) {
    if (open && root && !root.contains(e.target as Node)) close();
  }
  function close() {
    open = false;
    naming = false;
  }
  function pick(name: string) {
    close();
    mixer.loadProfile(name);
  }
  function startNaming() {
    draft = "";
    naming = true;
  }
  function saveAs() {
    const v = draft.trim();
    if (!v) return;
    close();
    mixer.saveProfileAs(v);
  }
  function onkeydown(e: KeyboardEvent) {
    if (e.key === "Enter") saveAs();
    else if (e.key === "Escape") {
      e.stopPropagation();
      naming = false;
    }
  }
  function focus(el: HTMLInputElement) {
    el.focus();
  }
</script>

<svelte:window onpointerdown={onWindowPointerDown} />

<div class="wrap" bind:this={root}>
  <button class="chip" title="Profile" aria-haspopup="menu" aria-expanded={open} onclick={() => (open ? close() : (open = true))}>
    {mixer.profileName}
    <svg width="10" height="10" viewBox="0 0 10 10" fill="none">
      <path d="M2.5 4l2.5 2.5L7.5 4" stroke="currentColor" stroke-width="1.25" />
    </svg>
  </button>
  {#if open}
    <div class="menu" role="menu">
      {#each mixer.profiles as name (name)}
        <div class="row">
          <button
            class="item"
            role="menuitemradio"
            aria-checked={name === mixer.profileName}
            class:selected={name === mixer.profileName}
            onclick={() => pick(name)}
          >
            {name}
          </button>
          {#if name !== mixer.profileName}
            <button class="del" title="Delete profile “{name}”" onclick={() => mixer.deleteProfile(name)}>
              <svg width="9" height="9" viewBox="0 0 12 12"><path d="M2.5 2.5l7 7M9.5 2.5l-7 7" stroke="currentColor" stroke-width="1.4" /></svg>
            </button>
          {/if}
        </div>
      {/each}
      <div class="sep"></div>
      {#if naming}
        <input class="name" placeholder="New profile name" maxlength="48" bind:value={draft} {onkeydown} use:focus data-no-drag />
      {:else}
        <button class="item add" role="menuitem" onclick={startNaming}>Save as new profile…</button>
      {/if}
    </div>
  {/if}
</div>

<style>
  .wrap {
    position: relative;
  }
  .chip {
    height: 24px;
    padding: 0 8px;
    display: flex;
    align-items: center;
    gap: 6px;
    background: var(--bg-3);
    border: 1px solid var(--border-0);
    border-radius: 2px;
    box-shadow: inset 0 1px 0 var(--hl-top);
    font-size: 12px;
    color: var(--text-1);
  }
  .chip:hover {
    background: var(--bg-4);
  }
  .chip svg {
    color: var(--text-3);
  }
  .menu {
    position: absolute;
    top: calc(100% + 2px);
    left: 0;
    min-width: 200px;
    z-index: 30;
    background: var(--bg-3);
    border: 1px solid var(--border-1);
    border-radius: 6px;
    box-shadow: 0 8px 24px rgba(0, 0, 0, 0.6);
    padding: 3px;
    display: flex;
    flex-direction: column;
  }
  .row {
    display: flex;
    align-items: center;
  }
  .item {
    flex: 1;
    min-width: 0;
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
  .item.add {
    color: var(--text-3);
  }
  .del {
    flex: none;
    width: 22px;
    height: 22px;
    display: flex;
    align-items: center;
    justify-content: center;
    color: var(--text-3);
    border-radius: 2px;
  }
  .del:hover {
    background: var(--mute);
    color: #fff;
  }
  .sep {
    height: 1px;
    margin: 3px 2px;
    background: var(--border-0);
  }
  .name {
    height: 24px;
    padding: 0 6px;
    margin: 1px;
    background: var(--bg-0);
    border: 1px solid var(--accent);
    border-radius: 2px;
    outline: none;
    font-size: 11px;
    color: var(--text-1);
  }
</style>
