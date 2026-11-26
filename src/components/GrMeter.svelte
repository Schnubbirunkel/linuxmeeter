<script lang="ts">
  /** Gain-reduction bar: grows right-to-left from 0 to RANGE dB, value on
   *  the right. Driven by the shared meter loop through a pre-DSP meter
   *  key; `reduction` turns that level (dBFS peak) into dB of reduction. */
  import { onMount } from "svelte";
  import type { MeterKey } from "../lib/types";
  import { getMeter, markDirty, registerRenderer } from "../lib/state/meters";
  import { peakOf } from "../lib/utils/dynamics";

  interface Props {
    key: MeterKey;
    reduction: (peakDb: number) => number;
    label?: string;
    width?: number;
    /** Changes whenever the inputs of `reduction` change (threshold, gain…). */
    deps?: unknown;
  }
  let { key, reduction, label = "GR", width = 80, deps }: Props = $props();

  const H = 14;
  const RANGE = 12;
  let canvas: HTMLCanvasElement;
  let lastDrawn = -1;
  // Plain (non-reactive) copy for the draw callback; refreshed by the effect.
  let compute: (peakDb: number) => number = () => 0;

  $effect(() => {
    void deps;
    compute = reduction;
    lastDrawn = -1;
    markDirty(key);
  });

  onMount(() => {
    const dpr = window.devicePixelRatio || 1;
    canvas.width = width * dpr;
    canvas.height = H * dpr;
    const ctx = canvas.getContext("2d")!;
    ctx.setTransform(dpr, 0, 0, dpr, 0, 0);
    const css = getComputedStyle(document.documentElement);
    const col = (n: string) => css.getPropertyValue(n).trim();
    const c = { trough: col("--bg-0"), bar: col("--accent"), hot: col("--meter-red"), text: col("--text-2"), dim: col("--text-3") };
    const mono = col("--mono") || "monospace";
    const state = getMeter(key);

    const barX = 16;
    const barW = width - barX - 30;
    const draw = () => {
      const gr = Math.max(0, compute(peakOf(state.peak)));
      const shown = Math.round(gr * 10) / 10;
      if (shown === lastDrawn) return; // nothing visible changed
      lastDrawn = shown;
      ctx.clearRect(0, 0, width, H);
      ctx.font = `500 8px ${mono}`;
      ctx.textBaseline = "middle";
      ctx.fillStyle = c.dim;
      ctx.fillText(label, 0, H / 2);
      ctx.fillStyle = c.trough;
      ctx.fillRect(barX, 4, barW, H - 8);
      const w = Math.min(1, gr / RANGE) * barW;
      if (w > 0) {
        ctx.fillStyle = gr >= 6 ? c.hot : c.bar;
        ctx.fillRect(barX + barW - w, 4, w, H - 8);
      }
      ctx.fillStyle = gr > 0.05 ? c.text : c.dim;
      ctx.textAlign = "right";
      ctx.fillText(gr > 0.05 ? `-${shown.toFixed(1)}` : "0.0", width, H / 2);
      ctx.textAlign = "left";
    };
    return registerRenderer(key, draw);
  });
</script>

<canvas bind:this={canvas} style:width="{width}px" style:height="{H}px" title="Gain reduction (dB): how much is being taken off right now"></canvas>

<style>
  canvas {
    display: block;
  }
</style>
