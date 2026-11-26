<script lang="ts">
  import { onMount } from "svelte";
  import { ipc } from "../lib/ipc";
  import { mixer } from "../lib/state/mixer.svelte";
  import { ui } from "../lib/state/ui.svelte";

  let notifications = $state(true);
  let exe = $state("linuxmeeter");
  let copied = $state<string | null>(null);

  onMount(async () => {
    notifications = await ipc.getNotifications();
    exe = await ipc.getExePath();
  });

  function toggleNotifications() {
    notifications = !notifications;
    void ipc.setNotifications(notifications);
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
    ["toggle-mute", "Mute"],
    ["volume-up", "+5 %"],
    ["volume-down", "−5 %"],
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
    <div class="hint">
      Assign these commands to keys in your desktop's shortcut settings. KDE Plasma: System Settings → Keyboard →
      Shortcuts → Add New → Command or Script. GNOME: Settings → Keyboard → Custom Shortcuts. Click a button to copy
      its command. Ids (<code>strip:3</code>, <code>A1</code>) keep working after a rename.
    </div>

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
      <div class="name">Profiles</div>
      {#each [["profile-prev", "Previous"], ["profile-next", "Next"]] as [action, text]}
        {@const cmd = `${q(exe)} ctl ${action}`}
        <button class="cmd" class:done={copied === cmd} title={cmd} onclick={() => copy(cmd)}>
          {copied === cmd ? "copied" : text}
        </button>
      {/each}
    </div>
    <div class="hint">
      Also: <code>ctl profile &lt;name&gt;</code>, <code>ctl mute|unmute &lt;channel&gt;</code>,
      <code>ctl volume-up &lt;channel&gt; 10</code> for other step sizes, <code>ctl list</code>.
    </div>
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
    width: 480px;
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
    grid-template-columns: 1fr repeat(3, 72px);
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
</style>
