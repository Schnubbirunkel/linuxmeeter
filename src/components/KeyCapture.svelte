<script lang="ts">
  import { keyEventToText, keyLabel } from "../lib/utils/keys";

  interface Props {
    /** Current keys ("Ctrl+Alt+M") or null. */
    value: string | null;
    /** Called with the new keys, or null to clear. Rejects with a message on failure. */
    onset: (keys: string | null) => Promise<void>;
    disabled?: boolean;
    title?: string;
  }
  let { value, onset, disabled = false, title = "" }: Props = $props();

  let listening = $state(false);
  let busy = $state(false);
  let error = $state<string | null>(null);
  let btn: HTMLButtonElement;

  function start() {
    if (disabled || busy) return;
    error = null;
    listening = true;
  }

  async function apply(keys: string | null) {
    listening = false;
    busy = true;
    try {
      await onset(keys);
      error = null;
    } catch (e) {
      error = String(e);
      setTimeout(() => (error = null), 3500);
    } finally {
      busy = false;
    }
  }

  // Capture phase on the window: the key must not reach anything else (Esc
  // would close the settings panel).
  function onkeydown(e: KeyboardEvent) {
    if (!listening) return;
    e.preventDefault();
    e.stopPropagation();
    const plain = !e.ctrlKey && !e.altKey && !e.shiftKey && !e.metaKey;
    if (plain && e.code === "Escape") {
      listening = false;
      return;
    }
    if (plain && (e.code === "Backspace" || e.code === "Delete")) {
      void apply(null);
      return;
    }
    const text = keyEventToText(e);
    if (text) void apply(text);
  }

  function onWindowPointerDown(e: PointerEvent) {
    if (listening && btn && !btn.contains(e.target as Node)) listening = false;
  }
</script>

<svelte:window onkeydowncapture={onkeydown} onpointerdown={onWindowPointerDown} />

<button
  bind:this={btn}
  class="cap"
  class:set={!!value}
  class:listening
  class:error={!!error}
  {disabled}
  title={error ??
    (listening
      ? "Press the keys · Esc cancels · Backspace clears"
      : `${title}${value ? `: ${value}` : ""} — click to set${value ? ", then Backspace to clear" : ""}`)}
  onclick={start}
>
  {#if error}
    {error.includes("used") ? "in use" : "failed"}
  {:else if listening}
    press…
  {:else if busy}
    …
  {:else}
    {value ? keyLabel(value) : "—"}
  {/if}
</button>

<style>
  .cap {
    width: 100%;
    height: 22px;
    padding: 0 4px;
    font: 500 10px var(--mono);
    background: var(--bg-3);
    color: var(--text-3);
    border: 1px solid var(--border-0);
    border-radius: 2px;
    box-shadow: inset 0 1px 0 var(--hl-top);
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
  }
  .cap:hover:not(:disabled) {
    background: var(--bg-4);
    color: var(--text-1);
  }
  .cap.set {
    color: var(--text-1);
  }
  .cap.listening {
    background: var(--accent-glow);
    color: var(--accent);
    border-color: var(--accent);
  }
  .cap.error {
    border-color: var(--mute);
    color: var(--mute);
  }
  .cap:disabled {
    opacity: 0.5;
  }
</style>
