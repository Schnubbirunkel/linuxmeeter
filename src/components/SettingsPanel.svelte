<script lang="ts">
  import { onMount } from "svelte";
  import { ipc } from "../lib/ipc";
  import { mixer } from "../lib/state/mixer.svelte";
  import { ui } from "../lib/state/ui.svelte";
  import KeyCapture from "./KeyCapture.svelte";

  let notifications = $state(true);
  let exe = $state("linuxmeeter");
  let copied = $state<string | null>(null);
  let available = $state(true);
  let bindings = $state<Record<string, string>>({});

  async function loadShortcuts() {
    const info = await ipc.getShortcuts();
    available = info.available;
    bindings = info.bindings;
  }

  onMount(() => {
    void (async () => {
      notifications = await ipc.getNotifications();
      exe = await ipc.getExePath();
      await loadShortcuts();
    })();
    return ipc.onShortcutsChanged(() => void loadShortcuts());
  });

  function toggleNotifications() {
    notifications = !notifications;
    void ipc.setNotifications(notifications);
  }

  /** Bind keys to a ctl action; the backend frees the keys from any other action. */
  async function bind(action: string, name: string, keys: string | null) {
    const stored = await ipc.setShortcut(action, name, keys);
    await loadShortcuts();
    if (keys && !stored) throw new Error("not accepted");
  }

  /** Quote for a shell command line only when needed. */
  function q(s: string): string {
    return /^[\w.:/+-]+$/.test(s) ? s : `'${s.replace(/'/g, `'\\''`)}'`;
  }

  interface Row {
    id: string;
    name: string;
  }
  let rows = $derived<Row[]>([
    ...mixer.strips.map((s) => ({ id: `strip:${s.id}`, name: s.label })),
    ...mixer.buses.map((b) => ({ id: b.id, name: `${b.id} ${b.label}` })),
  ]);

  const ACTIONS = [
    ["toggle-mute", "Mute", "Mute"],
    ["volume-up", "+5 %", "Volume up"],
    ["volume-down", "−5 %", "Volume down"],
  ] as const;
  const GLOBAL = [
    ["toggle-mute-mics", "Mute all microphones"],
    ["profile-next", "Next profile"],
    ["profile-prev", "Previous profile"],
  ] as const;

  async function copy(text: string) {
    try {
      await navigator.clipboard.writeText(text);
    } catch {
      // WebKitGTK may refuse clipboard access; fall back to a hidden textarea.
      const ta = document.createElement("textarea");
      ta.value = text;
      document.body.appendChild(ta);
      ta.select();
      document.execCommand("copy");
      ta.remove();
    }
    copied = text;
    setTimeout(() => {
      if (copied === text) copied = null;
    }, 1200);
  }

  function close() {
    ui.settingsOpen = false;
  }
  function onkeydown(e: KeyboardEvent) {
    if (e.key === "Escape") close();
  }
</script>

<svelte:window {onkeydown} />

<!-- svelte-ignore a11y_click_events_have_key_events, a11y_no_static_element_interactions -->
<div class="scrim" onclick={(e) => e.target === e.currentTarget && close()}>
  <div class="panel" role="dialog" aria-label="Settings">
    <header>
      <div class="title">SETTINGS</div>
      <button class="closebtn" title="Close" onclick={close}>
        <svg width="11" height="11" viewBox="0 0 12 12"><path d="M2.5 2.5l7 7M9.5 2.5l-7 7" stroke="currentColor" stroke-width="1.25" /></svg>
      </button>
    </header>

    <div class="setting">
      <div>
        <div class="label">Desktop notifications</div>
        <div class="hint">Profile switches and shortcut actions, also while the window is hidden.</div>
      </div>
      <button class="toggle" class:active={notifications} aria-pressed={notifications} onclick={toggleNotifications}>
        {notifications ? "ON" : "OFF"}
      </button>
    </div>

    <div class="sep"></div>

    <div class="label">Global shortcuts</div>
    {#if available}
      <div class="hint">
        Click a field and press the keys. They work everywhere, also while the window is hidden. Esc cancels, Backspace
        clears. They also appear in System Settings → Shortcuts.
      </div>

      <div class="table">
        {#each GLOBAL as [action, name]}
          <div class="name wide">{name}</div>
          <KeyCapture value={bindings[action] ?? null} title={name} onset={(k) => bind(action, name, k)} />
        {/each}
      </div>

      <div class="table">
        <div></div>
        {#each ACTIONS as [, text]}<div class="colhead">{text}</div>{/each}
        {#each rows as row (row.id)}
          <div class="name" title={row.id}>{row.name}</div>
          {#each ACTIONS as [action, , verb]}
            {@const id = `${action} ${row.id}`}
            {@const name = `${verb}: ${row.name}`}
            <KeyCapture value={bindings[id] ?? null} title={name} onset={(k) => bind(id, name, k)} />
          {/each}
        {/each}
      </div>
    {:else}
      <div class="hint warn">
        In-app shortcuts need KDE Plasma's shortcut service, which is not running. Use the commands below in your
        desktop's shortcut settings instead (GNOME: Settings → Keyboard → Custom Shortcuts).
      </div>
    {/if}

    <details open={!available}>
      <summary>Commands for scripts and other desktops</summary>
      <div class="table">
        {#each rows as row (row.id)}
          <div class="name" title={row.id}>{row.name}</div>
          {#each ACTIONS as [action, text]}
            {@const cmd = `${q(exe)} ctl ${action} ${row.id}`}
            <button class="cmd" class:done={copied === cmd} title={cmd} onclick={() => copy(cmd)}>
              {copied === cmd ? "copied" : text}
            </button>
          {/each}
        {/each}
        {#each GLOBAL as [action, name]}
          {@const cmd = `${q(exe)} ctl ${action}`}
          <div class="name wide">{name}</div>
          <button class="cmd" class:done={copied === cmd} title={cmd} onclick={() => copy(cmd)}>
            {copied === cmd ? "copied" : "copy"}
          </button>
        {/each}
      </div>
      <div class="hint">
        Click a button to copy its command. Also: <code>ctl profile &lt;name&gt;</code>,
        <code>ctl volume-up &lt;channel&gt; 10</code>, <code>ctl list</code>.
      </div>
    </details>
  </div>
</div>

<style>
  .scrim {
    position: fixed;
    inset: 0;
    z-index: 40;
    background: rgba(10, 11, 13, 0.7);
    display: flex;
    align-items: center;
    justify-content: center;
  }
  .panel {
    width: 500px;
    max-height: calc(100% - 32px);
    overflow-y: auto;
    background: var(--bg-2);
    border: 1px solid var(--border-1);
    border-radius: 6px;
    box-shadow: inset 0 1px 0 var(--hl-top), 0 8px 24px rgba(0, 0, 0, 0.6);
    padding: 12px;
    display: flex;
    flex-direction: column;
    gap: 10px;
  }
  header {
    display: flex;
    align-items: center;
    gap: 8px;
  }
  .title {
    flex: 1;
    font-size: 12px;
    font-weight: 600;
    letter-spacing: 0.08em;
  }
  .closebtn {
    width: 24px;
    height: 22px;
    display: flex;
    align-items: center;
    justify-content: center;
    color: var(--text-2);
    border-radius: 2px;
  }
  .closebtn:hover {
    background: var(--mute);
    color: #fff;
  }
  .setting {
    display: flex;
    align-items: center;
    gap: 12px;
  }
  .setting > div {
    flex: 1;
  }
  .label {
    font-size: 12px;
    font-weight: 600;
    color: var(--text-1);
  }
  .hint {
    margin-top: 2px;
    font-size: 11px;
    line-height: 1.45;
    color: var(--text-2);
  }
  .hint.warn {
    color: var(--accent);
  }
  code {
    font: 10px var(--mono);
    color: var(--text-1);
  }
  .sep {
    height: 1px;
    background: var(--border-0);
  }
  .toggle,
  .cmd {
    height: 22px;
    padding: 0 8px;
    font: 500 10px var(--mono);
    letter-spacing: 0.06em;
    background: var(--bg-3);
    color: var(--text-2);
    border: 1px solid var(--border-0);
    border-radius: 2px;
    box-shadow: inset 0 1px 0 var(--hl-top);
  }
  .toggle {
    width: 48px;
    color: var(--text-3);
  }
  .toggle:hover,
  .cmd:hover {
    background: var(--bg-4);
    color: var(--text-1);
  }
  .toggle.active,
  .cmd.done {
    background: var(--accent-glow);
    color: var(--accent);
    border-color: var(--accent);
  }
  .table {
    display: grid;
    grid-template-columns: 1fr repeat(3, 84px);
    gap: 4px 6px;
    align-items: center;
  }
  .name {
    font-size: 11px;
    color: var(--text-1);
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
  }
  .name.wide {
    grid-column: 1 / 4;
  }
  .colhead {
    font: 500 9px var(--mono);
    letter-spacing: 0.06em;
    color: var(--text-3);
    text-align: center;
  }
  details {
    display: flex;
    flex-direction: column;
    gap: 8px;
  }
  summary {
    cursor: pointer;
    font-size: 11px;
    color: var(--text-2);
    margin-bottom: 6px;
  }
  summary:hover {
    color: var(--text-1);
  }
</style>
