<script lang="ts">
  import type { StripState } from "../lib/types";
  import { mixer } from "../lib/state/mixer.svelte";
  import { ui } from "../lib/state/ui.svelte";
  import Knob from "./Knob.svelte";
  import { onMount } from "svelte";
  import { getMeter, markDirty, registerRenderer } from "../lib/state/meters";
  import { compReduction, gateOpen, peakOf } from "../lib/utils/dynamics";

  interface Props {
    strip: StripState;
  }
  let { strip }: Props = $props();

  // ---- compressor transfer curve ----
  const CW = 150;
  const CH = 150;
  const DB_MIN = -60; // both axes: -60..0 dBFS

  let canvas: HTMLCanvasElement;
  let dragging = $state(false);
  let hover = $state<"knee" | "slope" | null>(null);

  const xFor = (db: number) => ((db - DB_MIN) / -DB_MIN) * CW;
  const yFor = (db: number) => CH - ((db - DB_MIN) / -DB_MIN) * CH;
  const xDb = (x: number) => DB_MIN + (x / CW) * -DB_MIN;
  const yDb = (y: number) => DB_MIN + ((CH - y) / CH) * -DB_MIN;

  function transfer(inDb: number): number {
    const c = strip.comp;
    const out = inDb <= c.thresholdDb ? inDb : c.thresholdDb + (inDb - c.thresholdDb) / Math.max(1, c.ratio);
    return out + c.makeupDb;
  }

  // ---- live view: what the gate and compressor do to the signal now ----
  // One renderer on the strip's pre-dynamics meter draws all three canvases.
  const GR_RANGE = 24;
  const GATE_W = 330;
  const GATE_H = 16;
  const GATE_MIN = -70;
  let live: HTMLCanvasElement;
  let grBar: HTMLCanvasElement;
  let gateBar: HTMLCanvasElement;
  let liveText = $state({ comp: "—", gate: "—", open: true, level: "—" });
  // Plain copies for the draw callback (it runs outside Svelte).
  let params: { comp: StripState["comp"]; gate: StripState["gate"]; gainDb: number } | null = null;

  $effect(() => {
    params = { comp: { ...strip.comp }, gate: { ...strip.gate }, gainDb: strip.gainDb };
    markDirty(`sp:${strip.id}`);
  });

  onMount(() => {
    const dpr = window.devicePixelRatio || 1;
    const setup = (c: HTMLCanvasElement, w: number, h: number) => {
      c.width = w * dpr;
      c.height = h * dpr;
      const ctx = c.getContext("2d")!;
      ctx.setTransform(dpr, 0, 0, dpr, 0, 0);
      return ctx;
    };
    const lc = setup(live, CW, CH);
    const gc = setup(grBar, 8, CH);
    const tc = setup(gateBar, GATE_W, GATE_H);
    const css = getComputedStyle(document.documentElement);
    const col = (n: string) => css.getPropertyValue(n).trim();
    const c = {
      accent: col("--accent"),
      hi: col("--accent-hi"),
      trough: col("--bg-0"),
      green: col("--meter-green"),
      red: col("--meter-red"),
      dim: col("--text-3"),
      text: col("--text-1"),
    };
    const mono = col("--mono") || "monospace";
    const state = getMeter(`sp:${strip.id}`);
    let last = "";

    const draw = () => {
      if (!params) return;
      const { comp, gate, gainDb } = params;
      const pre = peakOf(state.peak);
      const level = pre <= -80 ? -90 : pre + gainDb;
      const gr = compReduction(level, comp.thresholdDb, comp.ratio, comp.enabled);
      const open = gateOpen(level, gate.thresholdDb, gate.enabled);
      const key = `${level.toFixed(1)}|${gr.toFixed(1)}|${open}|${comp.thresholdDb}|${comp.ratio}|${comp.makeupDb}|${gate.thresholdDb}|${comp.enabled}|${gate.enabled}`;
      if (key === last) return;
      last = key;

      // Dot on the transfer curve.
      lc.clearRect(0, 0, CW, CH);
      if (level > DB_MIN) {
        const x = xFor(Math.min(0, level));
        const y = yFor(Math.max(DB_MIN, Math.min(0, transfer(Math.min(0, level)))));
        lc.strokeStyle = "rgba(255,255,255,0.18)";
        lc.lineWidth = 1;
        lc.beginPath();
        lc.moveTo(Math.round(x) + 0.5, CH);
        lc.lineTo(Math.round(x) + 0.5, y);
        lc.stroke();
        lc.beginPath();
        lc.arc(x, y, 4, 0, Math.PI * 2);
        lc.fillStyle = comp.enabled ? c.hi : c.dim;
        lc.fill();
      }

      // Compressor gain reduction, top-down.
      gc.fillStyle = c.trough;
      gc.fillRect(0, 0, 8, CH);
      const h = Math.min(1, gr / GR_RANGE) * CH;
      if (h > 0) {
        gc.fillStyle = gr >= 12 ? c.red : c.accent;
        gc.fillRect(0, 0, 8, h);
      }

      // Gate: input level against the threshold.
      const gx = (db: number) => ((Math.max(GATE_MIN, Math.min(0, db)) - GATE_MIN) / -GATE_MIN) * GATE_W;
      tc.fillStyle = c.trough;
      tc.fillRect(0, 0, GATE_W, GATE_H);
      if (level > GATE_MIN) {
        tc.fillStyle = !gate.enabled ? c.dim : open ? c.green : c.red;
        tc.fillRect(0, 4, gx(level), GATE_H - 8);
      }
      const tx = Math.round(gx(gate.thresholdDb)) + 0.5;
      tc.strokeStyle = gate.enabled ? c.text : c.dim;
      tc.lineWidth = 2;
      tc.beginPath();
      tc.moveTo(tx, 1);
      tc.lineTo(tx, GATE_H - 1);
      tc.stroke();
      tc.font = `500 8px ${mono}`;

      liveText = {
        comp: comp.enabled ? (gr > 0.05 ? `-${gr.toFixed(1)} dB` : "0.0 dB") : "off",
        gate: gate.enabled ? (open ? "OPEN" : "CLOSED") : "off",
        open,
        level: level > -80 ? `${level.toFixed(1)} dBFS` : "silence",
      };
    };
    return registerRenderer(`sp:${strip.id}`, draw);
  });

  function setComp(patch: Partial<StripState["comp"]>, enable = true) {
    mixer.setCompParams(strip.id, { ...strip.comp, ...patch, ...(enable ? { enabled: true } : {}) });
  }
  function setGate(patch: Partial<StripState["gate"]>, enable = true) {
    mixer.setGateParams(strip.id, { ...strip.gate, ...patch, ...(enable ? { enabled: true } : {}) });
  }

  $effect(() => {
    const c = strip.comp;
    void c; // track
    const ctx = canvas?.getContext("2d");
    if (!ctx) return;
    const dpr = window.devicePixelRatio || 1;
    canvas.width = CW * dpr;
    canvas.height = CH * dpr;
    ctx.setTransform(dpr, 0, 0, dpr, 0, 0);
    const css = getComputedStyle(document.documentElement);
    const col = (n: string) => css.getPropertyValue(n).trim();

    ctx.fillStyle = col("--bg-0");
    ctx.fillRect(0, 0, CW, CH);

    // grid every 12 dB + unity diagonal
    ctx.strokeStyle = "rgba(255,255,255,0.05)";
    ctx.lineWidth = 1;
    for (let db = -48; db < 0; db += 12) {
      const x = Math.round(xFor(db)) + 0.5;
      const y = Math.round(yFor(db)) + 0.5;
      ctx.beginPath();
      ctx.moveTo(x, 0);
      ctx.lineTo(x, CH);
      ctx.moveTo(0, y);
      ctx.lineTo(CW, y);
      ctx.stroke();
    }
    ctx.strokeStyle = "rgba(255,255,255,0.12)";
    ctx.setLineDash([3, 3]);
    ctx.beginPath();
    ctx.moveTo(xFor(DB_MIN), yFor(DB_MIN));
    ctx.lineTo(xFor(0), yFor(0));
    ctx.stroke();
    ctx.setLineDash([]);

    // transfer curve — slope segment gets a hover/drag highlight
    const curveCol = c.enabled ? col("--accent") : col("--text-3");
    const slopeHot = hover === "slope" || (dragging && dragMode === "slope");
    ctx.lineWidth = 2;
    ctx.strokeStyle = curveCol;
    ctx.beginPath();
    for (let px = 0; px <= Math.min(CW, xFor(c.thresholdDb)); px += 2) {
      const y = yFor(Math.max(DB_MIN, Math.min(0, transfer(xDb(px)))));
      if (px === 0) ctx.moveTo(px, y);
      else ctx.lineTo(px, y);
    }
    ctx.stroke();
    ctx.lineWidth = slopeHot ? 3.5 : 2;
    ctx.strokeStyle = slopeHot ? col("--accent-hi") : curveCol;
    ctx.beginPath();
    let first = true;
    for (let px = Math.max(0, Math.floor(xFor(c.thresholdDb))); px <= CW; px += 2) {
      const y = yFor(Math.max(DB_MIN, Math.min(0, transfer(xDb(px)))));
      if (first) {
        ctx.moveTo(px, y);
        first = false;
      } else ctx.lineTo(px, y);
    }
    ctx.stroke();

    // knee point (threshold, threshold+makeup)
    const kneeHot = hover === "knee" || (dragging && dragMode === "knee");
    const kx = xFor(c.thresholdDb);
    const ky = yFor(Math.max(DB_MIN, Math.min(0, c.thresholdDb + c.makeupDb)));
    ctx.beginPath();
    ctx.arc(kx, ky, kneeHot ? 7 : 5, 0, Math.PI * 2);
    ctx.fillStyle = col("--bg-2");
    ctx.fill();
    ctx.lineWidth = 2;
    ctx.strokeStyle = c.enabled ? (kneeHot ? col("--accent-hi") : col("--accent")) : col("--text-2");
    ctx.stroke();
    if (kneeHot) {
      ctx.beginPath();
      ctx.arc(kx, ky, 11, 0, Math.PI * 2);
      ctx.strokeStyle = col("--accent-glow");
      ctx.lineWidth = 3;
      ctx.stroke();
    }
  });

  function canvasPos(e: PointerEvent): [number, number] {
    const r = canvas.getBoundingClientRect();
    return [((e.clientX - r.left) / r.width) * CW, ((e.clientY - r.top) / r.height) * CH];
  }

  /** Direct manipulation: the grabbed point of the curve stays under the
   *  pointer. "knee" carries threshold+makeup (with grab offset so nothing
   *  jumps); "slope" solves the ratio so the curve passes through the pointer. */
  let dragMode: "knee" | "slope" = "knee";
  let grabOffset = { x: 0, y: 0 };

  function hitTest(mx: number, my: number): "knee" | "slope" | null {
    const c = strip.comp;
    const kx = xFor(c.thresholdDb);
    const ky = yFor(Math.max(DB_MIN, Math.min(0, c.thresholdDb + c.makeupDb)));
    if (Math.hypot(kx - mx, ky - my) < 14) return "knee";
    // Near the curve, right of the knee?
    if (mx > kx + 6) {
      const curveY = yFor(Math.max(DB_MIN, Math.min(0, transfer(xDb(mx)))));
      if (Math.abs(curveY - my) < 14) return "slope";
    }
    return null;
  }

  function onpointerdown(e: PointerEvent) {
    if (e.button !== 0) return;
    const [mx, my] = canvasPos(e);
    const mode = hitTest(mx, my);
    if (!mode) return;
    const c = strip.comp;
    dragMode = mode;
    grabOffset = {
      x: xFor(c.thresholdDb) - mx,
      y: yFor(Math.max(DB_MIN, Math.min(0, c.thresholdDb + c.makeupDb))) - my,
    };
    dragging = true;
    canvas.setPointerCapture(e.pointerId);
  }
  function onpointermove(e: PointerEvent) {
    const [mx, my] = canvasPos(e);
    if (!dragging) {
      hover = hitTest(mx, my);
      return;
    }
    const c = strip.comp;
    if (dragMode === "knee") {
      const thresholdDb = Math.round(Math.max(-60, Math.min(0, xDb(mx + grabOffset.x))));
      const makeupDb = Math.round(Math.max(-12, Math.min(24, yDb(my + grabOffset.y) - thresholdDb)) * 2) / 2;
      setComp({ thresholdDb, makeupDb });
    } else {
      // Solve ratio so the curve passes through the pointer:
      // out = T + (in - T)/ratio + makeup  =>  ratio = (in - T) / (out - T - makeup)
      const inDb = Math.max(c.thresholdDb + 2, xDb(mx));
      const outAboveKnee = yDb(my) - c.thresholdDb - c.makeupDb;
      const ratio = Math.max(1, Math.min(20, (inDb - c.thresholdDb) / Math.max(0.15, outAboveKnee)));
      setComp({ ratio: Math.round(ratio * 10) / 10 });
    }
  }
  function onpointerup() {
    dragging = false;
  }
  function onpointerleave() {
    if (!dragging) hover = null;
  }
  function ondblclick() {
    setComp({ thresholdDb: -18, makeupDb: 0, ratio: 4 }, false);
  }

  const COMP_DEFAULTS = { thresholdDb: -18, ratio: 4, attackMs: 20, releaseMs: 100, makeupDb: 0 };
  const GATE_DEFAULTS = { thresholdDb: -40, attackMs: 20, releaseMs: 100, holdMs: 50 };

  function resetComp() {
    mixer.setCompParams(strip.id, { ...strip.comp, ...COMP_DEFAULTS });
  }
  function resetGate() {
    mixer.setGateParams(strip.id, { ...strip.gate, ...GATE_DEFAULTS });
  }

  function close() {
    ui.dynStrip = null;
  }
  function onkeydown(e: KeyboardEvent) {
    if (e.key === "Escape") close();
  }

  const fmtMs = (v: number) => (v < 10 ? `${v.toFixed(1)} ms` : `${Math.round(v)} ms`);
</script>

<svelte:window {onkeydown} />

<!-- scrim click closes; Esc handled on window -->
<!-- svelte-ignore a11y_click_events_have_key_events, a11y_no_static_element_interactions -->
<div class="scrim" onclick={(e) => e.target === e.currentTarget && close()}>
  <div class="panel" role="dialog" aria-label="Dynamics">
    <header>
      <div class="title">DYNAMICS <span class="sub">{strip.label}</span></div>
      <button class="closebtn" title="Close" onclick={close}>
        <svg width="11" height="11" viewBox="0 0 12 12"><path d="M2.5 2.5l7 7M9.5 2.5l-7 7" stroke="currentColor" stroke-width="1.25" /></svg>
      </button>
    </header>

    <section>
      <div class="sechead">
        <span class="secname">COMPRESSOR</span>
        <span class="secbtns">
          <button class="enbtn" title="Reset compressor to defaults" onclick={resetComp}>RESET</button>
          <button
            class="enbtn"
            class:active={strip.comp.enabled}
            aria-pressed={strip.comp.enabled}
            onclick={() => mixer.setCompParams(strip.id, { ...strip.comp, enabled: !strip.comp.enabled })}
          >
            {strip.comp.enabled ? "ON" : "OFF"}
          </button>
        </span>
      </div>
      <div class="comprow">
        <div class="curvewrap">
        <canvas
          bind:this={canvas}
          style:width="{CW}px"
          style:height="{CH}px"
          {onpointerdown}
          {onpointermove}
          {onpointerup}
          {onpointerleave}
          {ondblclick}
          style:cursor={hover === "knee" ? "move" : hover === "slope" ? "ns-resize" : "default"}
          title="Drag knee: threshold + makeup · drag slope: ratio · double-click: reset"
        ></canvas>
        <canvas class="overlay" bind:this={live} style:width="{CW}px" style:height="{CH}px"></canvas>
        </div>
        <div class="grcol" title="Compressor gain reduction right now">
          <canvas class="gr" bind:this={grBar} style:width="8px" style:height="{CH}px"></canvas>
          <span class="grval">{liveText.comp}</span>
        </div>
        <div class="knobs">
          <Knob label="THRESH" min={-60} max={0} value={strip.comp.thresholdDb} defaultValue={-18} onchange={(v) => setComp({ thresholdDb: v })} />
          <Knob label="RATIO" min={1} max={20} taper="log" value={strip.comp.ratio} defaultValue={4} fmt={(v) => `${v.toFixed(1)}:1`} onchange={(v) => setComp({ ratio: Math.round(v * 10) / 10 })} />
          <Knob label="MAKEUP" min={-12} max={24} value={strip.comp.makeupDb} defaultValue={0} onchange={(v) => setComp({ makeupDb: Math.round(v * 2) / 2 })} />
          <Knob label="ATTACK" min={0.1} max={500} taper="log" value={strip.comp.attackMs} defaultValue={20} fmt={fmtMs} onchange={(v) => setComp({ attackMs: Math.round(v * 10) / 10 })} />
          <Knob label="RELEASE" min={10} max={2000} taper="log" value={strip.comp.releaseMs} defaultValue={100} fmt={fmtMs} onchange={(v) => setComp({ releaseMs: Math.round(v) })} />
        </div>
      </div>
    </section>

    <div class="sep"></div>

    <section>
      <div class="sechead">
        <span class="secname">GATE</span>
        <span class="secbtns">
          <button class="enbtn" title="Reset gate to defaults" onclick={resetGate}>RESET</button>
          <button
            class="enbtn"
            class:active={strip.gate.enabled}
            aria-pressed={strip.gate.enabled}
            onclick={() => mixer.setGateParams(strip.id, { ...strip.gate, enabled: !strip.gate.enabled })}
          >
            {strip.gate.enabled ? "ON" : "OFF"}
          </button>
        </span>
      </div>
      <div class="gatelive" title="Input level (after the strip fader) against the gate threshold">
        <canvas class="gatebar" bind:this={gateBar} style:width="{GATE_W}px" style:height="{GATE_H}px"></canvas>
        <span class="gatestate" class:closed={!liveText.open}>{liveText.gate}</span>
      </div>
      <div class="levelnote">input {liveText.level}</div>
      <div class="knobs gaterow">
        <Knob label="THRESH" min={-70} max={0} value={strip.gate.thresholdDb} defaultValue={-40} onchange={(v) => setGate({ thresholdDb: v })} />
        <Knob label="ATTACK" min={0.1} max={500} taper="log" value={strip.gate.attackMs} defaultValue={20} fmt={fmtMs} onchange={(v) => setGate({ attackMs: Math.round(v * 10) / 10 })} />
        <Knob label="RELEASE" min={10} max={2000} taper="log" value={strip.gate.releaseMs} defaultValue={100} fmt={fmtMs} onchange={(v) => setGate({ releaseMs: Math.round(v) })} />
        <Knob label="HOLD" min={1} max={1000} taper="log" value={strip.gate.holdMs} defaultValue={50} fmt={fmtMs} onchange={(v) => setGate({ holdMs: Math.round(v) })} />
      </div>
    </section>
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
    width: 420px;
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
  .title .sub {
    color: var(--text-2);
    font-weight: 500;
    margin-left: 6px;
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
  .sep {
    height: 1px;
    background: var(--border-0);
    margin: 0 -12px;
  }
  section {
    display: flex;
    flex-direction: column;
    gap: 8px;
  }
  .sechead {
    display: flex;
    align-items: center;
    justify-content: space-between;
  }
  .secname {
    font-size: 10px;
    font-weight: 600;
    letter-spacing: 0.08em;
    color: var(--text-2);
  }
  .secbtns {
    display: flex;
    gap: 4px;
  }
  .enbtn {
    height: 20px;
    padding: 0 10px;
    font: 500 10px var(--mono);
    letter-spacing: 0.06em;
    background: var(--bg-3);
    color: var(--text-3);
    border: 1px solid var(--border-0);
    border-radius: 2px;
    box-shadow: inset 0 1px 0 var(--hl-top);
  }
  .enbtn.active {
    background: var(--accent-glow);
    color: var(--accent);
    border-color: var(--accent);
  }
  canvas {
    display: block;
    border: 1px solid var(--border-0);
    border-radius: 2px;
    cursor: crosshair;
    touch-action: none;
    flex: none;
  }
  .curvewrap {
    position: relative;
    flex: none;
  }
  canvas.overlay {
    position: absolute;
    inset: 0;
    border-color: transparent;
    pointer-events: none;
  }
  .grcol {
    display: flex;
    flex-direction: column;
    align-items: center;
    gap: 4px;
    flex: none;
  }
  canvas.gr,
  canvas.gatebar {
    cursor: default;
  }
  .grval,
  .gatestate,
  .levelnote {
    font: 500 9px var(--mono);
    color: var(--text-2);
    white-space: nowrap;
  }
  .grval {
    width: 44px;
    text-align: center;
  }
  .gatelive {
    display: flex;
    align-items: center;
    gap: 8px;
  }
  .gatestate {
    color: var(--meter-green);
  }
  .gatestate.closed {
    color: var(--meter-red);
  }
  .levelnote {
    color: var(--text-3);
    margin-top: -6px;
  }
  .comprow {
    display: flex;
    gap: 14px;
    align-items: flex-start;
  }
  .knobs {
    display: grid;
    grid-template-columns: repeat(3, 1fr);
    gap: 10px 6px;
    flex: 1;
  }
  .gaterow {
    grid-template-columns: repeat(4, 1fr);
  }
</style>
