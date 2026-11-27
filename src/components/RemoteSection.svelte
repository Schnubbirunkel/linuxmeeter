<script lang="ts">
  import { onMount } from "svelte";
  import { ipc, type RemoteInfo } from "../lib/ipc";

  let info = $state<RemoteInfo | null>(null);
  let busy = $state(false);
  let copied = $state(false);

  onMount(() => {
    void ipc.getRemote().then((r) => (info = r));
    // Keep the "phones connected" count fresh while the panel is open.
    const t = setInterval(() => {
      if (info?.enabled) void ipc.getRemote().then((r) => (info = r));
    }, 3000);
    return () => clearInterval(t);
  });

  async function toggle() {
    busy = true;
    try {
      info = await ipc.setRemote(!info?.enabled);
    } finally {
      busy = false;
    }
  }

  async function newKey() {
    busy = true;
    try {
      info = await ipc.newRemoteKey();
    } finally {
      busy = false;
    }
  }

  async function copyUrl() {
    if (!info) return;
    try {
      await navigator.clipboard.writeText(info.url);
    } catch {
      const ta = document.createElement("textarea");
      ta.value = info.url;
      document.body.appendChild(ta);
      ta.select();
      document.execCommand("copy");
      ta.remove();
    }
    copied = true;
    setTimeout(() => (copied = false), 1200);
  }
</script>

<div class="setting">
  <div>
    <div class="label">Phone remote</div>
    <div class="hint">
      Faders, mute, microphones and profiles from your phone's browser — scan the code. Only devices on your home
      network that have the code can connect.
    </div>
  </div>
  <button class="toggle" class:active={info?.enabled} aria-pressed={!!info?.enabled} disabled={busy} onclick={toggle}>
    {info?.enabled ? "ON" : "OFF"}
  </button>
</div>

{#if info?.error}
  <div class="hint warn">{info.error}</div>
{/if}

{#if info?.enabled}
  <div class="remote">
    <!-- The SVG comes from our own backend (qrcode crate), not from the network. -->
    <div class="qr" aria-label="QR code for the phone remote">{@html info.qrSvg}</div>
    <div class="side">
      <div class="clients">
        {info.clients === 0 ? "No phone connected" : info.clients === 1 ? "1 phone connected" : `${info.clients} phones connected`}
      </div>
      <button class="url" title="Copy the address" onclick={copyUrl}>{copied ? "copied" : info.url}</button>
      <div class="hint">
        Phone and computer need to be on the same network. If the page does not load, your firewall may block port
        {info.port}.
      </div>
      <button class="cmd" disabled={busy} title="Phones that scanned the old code are disconnected" onclick={newKey}>NEW CODE</button>
    </div>
  </div>
{/if}

<style>
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
    color: var(--mute);
  }
  .toggle,
  .cmd {
    height: 22px;
    padding: 0 8px;
    font: 500 10px var(--mono);
    letter-spacing: 0.06em;
    background: var(--bg-3);
    color: var(--text-3);
    border: 1px solid var(--border-0);
    border-radius: 2px;
    box-shadow: inset 0 1px 0 var(--hl-top);
  }
  .toggle {
    width: 48px;
  }
  .cmd {
    color: var(--text-2);
    align-self: flex-start;
  }
  .toggle:hover,
  .cmd:hover {
    background: var(--bg-4);
    color: var(--text-1);
  }
  .toggle.active {
    background: var(--accent-glow);
    color: var(--accent);
    border-color: var(--accent);
  }
  .remote {
    display: flex;
    gap: 14px;
    align-items: flex-start;
  }
  .qr {
    flex: none;
    width: 180px;
    height: 180px;
    background: #fff;
    border-radius: 4px;
    overflow: hidden;
  }
  .qr :global(svg) {
    display: block;
    width: 100%;
    height: 100%;
  }
  .side {
    display: flex;
    flex-direction: column;
    gap: 8px;
    min-width: 0;
  }
  .clients {
    font-size: 12px;
    font-weight: 600;
    color: var(--accent);
  }
  .url {
    font: 10px var(--mono);
    color: var(--text-1);
    background: var(--bg-0);
    border: 1px solid var(--border-0);
    border-radius: 2px;
    padding: 4px 6px;
    text-align: left;
    word-break: break-all;
  }
</style>
