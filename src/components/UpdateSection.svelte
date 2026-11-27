<script lang="ts">
  import { onMount } from "svelte";
  import { ipc, type UpdateChannel, type UpdateInfo } from "../lib/ipc";

  let version = $state("");
  let sha = $state("");
  let channel = $state<UpdateChannel>("stable");
  let info = $state<UpdateInfo | null>(null);
  let checking = $state(false);
  let installing = $state(false);
  let progress = $state("");
  let error = $state<string | null>(null);

  onMount(() => {
    const off = ipc.onUpdateProgress((t) => (progress = t));
    void (async () => {
      [version, sha] = await ipc.getBuildInfo();
      channel = await ipc.getUpdateChannel();
      await check();
    })();
    return off;
  });

  async function check() {
    checking = true;
    error = null;
    try {
      info = await ipc.checkUpdate(channel);
    } catch (e) {
      error = String(e);
      info = null;
    } finally {
      checking = false;
    }
  }

  function setChannel(c: UpdateChannel) {
    if (c === channel || installing) return;
    channel = c;
    void check();
  }

  async function install() {
    if (!info?.asset) return;
    installing = true;
    error = null;
    progress = "Starting…";
    try {
      await ipc.installUpdate(info.platform, info.asset);
      // On success the app restarts before this returns.
    } catch (e) {
      error = String(e);
      installing = false;
      progress = "";
    }
  }
</script>

<div class="setting">
  <div>
    <div class="label">Updates</div>
    <div class="hint">
      This is version {version}{sha ? ` (${sha.slice(0, 7)})` : ""}.
    </div>
  </div>
  <div class="seg" role="radiogroup" aria-label="Update channel">
    <button class:active={channel === "stable"} role="radio" aria-checked={channel === "stable"} title="Numbered releases" onclick={() => setChannel("stable")}>
      RELEASES
    </button>
    <button class:active={channel === "edge"} role="radio" aria-checked={channel === "edge"} title="Every push to main, as soon as it is built (newest, least tested)" onclick={() => setChannel("edge")}>
      EVERY PUSH
    </button>
  </div>
</div>
<div class="status">
  {#if installing}
    <span class="msg">{progress}</span>
  {:else if checking}
    <span class="msg dim">Checking…</span>
  {:else if error}
    <span class="msg warn">{error}</span>
    <button class="cmd" onclick={check}>RETRY</button>
  {:else if info}
    {#if info.platform === "unsupported"}
      <span class="msg">Automatic updates work on Arch and Debian. Newest: {info.latest || "—"}</span>
    {:else if info.available}
      <span class="msg new">{info.latest} is available</span>
      <button class="cmd" onclick={() => ipc.openReleasePage(info!.releaseUrl)}>NOTES</button>
      <button class="cmd primary" onclick={install}>UPDATE</button>
    {:else if !info.latest}
      <span class="msg dim">{channel === "edge" ? "No build from main yet." : "No release yet."}</span>
      <button class="cmd" onclick={check}>CHECK</button>
    {:else}
      <span class="msg dim">Up to date ({info.latest}).</span>
      <button class="cmd" onclick={check}>CHECK</button>
    {/if}
  {/if}
</div>

<style>
  .setting {
    display: flex;
    align-items: center;
    gap: 12px;
  }
  .setting > div:first-child {
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
    color: var(--text-2);
  }
  .seg {
    display: flex;
  }
  .seg button,
  .cmd {
    height: 22px;
    padding: 0 8px;
    font: 500 10px var(--mono);
    letter-spacing: 0.06em;
    background: var(--bg-3);
    color: var(--text-3);
    border: 1px solid var(--border-0);
    box-shadow: inset 0 1px 0 var(--hl-top);
  }
  .seg button:first-child {
    border-radius: 2px 0 0 2px;
  }
  .seg button:last-child {
    border-radius: 0 2px 2px 0;
    border-left: none;
  }
  .seg button.active {
    background: var(--accent-glow);
    color: var(--accent);
    border-color: var(--accent);
  }
  .cmd {
    border-radius: 2px;
    color: var(--text-2);
  }
  .cmd:hover,
  .seg button:hover {
    background: var(--bg-4);
    color: var(--text-1);
  }
  .cmd.primary {
    background: var(--accent);
    color: var(--on-accent);
    border-color: var(--accent);
  }
  .cmd.primary:hover {
    background: var(--accent-hi);
    color: var(--on-accent);
  }
  .status {
    display: flex;
    align-items: center;
    gap: 6px;
    margin-top: -4px;
    min-height: 22px;
  }
  .msg {
    flex: 1;
    font-size: 11px;
    color: var(--text-1);
  }
  .msg.dim {
    color: var(--text-3);
  }
  .msg.new {
    color: var(--accent);
    font-weight: 600;
  }
  .msg.warn {
    color: var(--mute);
  }
</style>
