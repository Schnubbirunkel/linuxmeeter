<script lang="ts">
  import DeviceSelect from "./DeviceSelect.svelte";

  interface Option {
    value: string;
    label: string;
  }
  interface Props {
    label: string;
    busBadge?: string;
    deviceValue: string | null;
    deviceDisplay: string;
    deviceOptions: Option[];
    ondevicechange?: (value: string) => void;
    deviceDisabled?: boolean;
    /** Double-click the name to rename; omitted = not renamable. */
    onrename?: (label: string) => void;
    /** Extra tooltip line for the name (e.g. the id for shortcuts). */
    nameTitle?: string;
  }
  let { label, busBadge, deviceValue, deviceDisplay, deviceOptions, ondevicechange, deviceDisabled, onrename, nameTitle }: Props =
    $props();

  let editing = $state(false);
  let draft = $state("");

  function startEdit() {
    if (!onrename) return;
    draft = label;
    editing = true;
  }
  function commit() {
    if (!editing) return;
    editing = false;
    const v = draft.trim();
    if (v && v !== label) onrename?.(v);
  }
  function onkeydown(e: KeyboardEvent) {
    if (e.key === "Enter") commit();
    else if (e.key === "Escape") {
      e.stopPropagation();
      editing = false;
    }
  }
  function focusSelect(el: HTMLInputElement) {
    el.focus();
    el.select();
  }
</script>

<div class="head">
  <div class="nrow">
    {#if busBadge}<span class="busid">{busBadge}</span>{/if}
    {#if editing}
      <input
        class="name edit"
        maxlength="32"
        bind:value={draft}
        onblur={commit}
        {onkeydown}
        use:focusSelect
        aria-label="Name"
        data-no-drag
      />
    {:else}
      <!-- svelte-ignore a11y_no_static_element_interactions -->
      <div
        class="name"
        class:renamable={!!onrename}
        title={onrename ? `Double-click to rename${nameTitle ? `\n${nameTitle}` : ""}` : nameTitle}
        ondblclick={startEdit}
      >
        {label}
      </div>
    {/if}
  </div>
  <DeviceSelect
    value={deviceValue}
    display={deviceDisplay}
    options={deviceOptions}
    onchange={ondevicechange}
    disabled={deviceDisabled}
  />
</div>

<style>
  .head {
    display: flex;
    flex-direction: column;
    gap: 5px;
  }
  .nrow {
    display: flex;
    align-items: baseline;
    gap: 6px;
    min-width: 0;
  }
  .busid {
    font: 600 10px var(--mono);
    letter-spacing: 0.08em;
    color: var(--accent);
  }
  .name {
    font-size: 12px;
    font-weight: 600;
    color: var(--text-1);
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
  }
  .name.renamable {
    cursor: text;
  }
  .name.edit {
    flex: 1;
    min-width: 0;
    height: 18px;
    padding: 0 4px;
    margin: -2px 0 -2px -4px;
    background: var(--bg-0);
    border: 1px solid var(--accent);
    border-radius: 2px;
    outline: none;
    font: inherit;
    font-size: 12px;
    font-weight: 600;
    color: var(--text-1);
  }
</style>
