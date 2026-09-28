<script lang="ts">
// The dithered desktop: a faint dot grid plus a few soft clouds of pixels.
// Seeded, so a window looks the same every time it opens; redrawn on
// resize and when macOS switches between light and dark.
import { onMount } from "svelte";
import { dither } from "./dither";

/** [x, y, radius] as fractions of the window; radius of its larger side */
let { seed = 7, clouds }: { seed?: number; clouds: [number, number, number][] } = $props();
let canvas: HTMLCanvasElement;

function draw() {
  const css = getComputedStyle(document.documentElement);
  dither(canvas, {
    seed,
    clouds,
    desk: css.getPropertyValue("--desk").trim(),
    dot: css.getPropertyValue("--desk-dot").trim(),
  });
}

onMount(() => {
  let t: ReturnType<typeof setTimeout> | undefined;
  const later = () => {
    clearTimeout(t);
    t = setTimeout(draw, 80);
  };
  const dark = matchMedia("(prefers-color-scheme: dark)");
  addEventListener("resize", later);
  dark.addEventListener("change", draw);
  draw();
  return () => {
    removeEventListener("resize", later);
    dark.removeEventListener("change", draw);
  };
});
</script>

<canvas bind:this={canvas}></canvas>

<style>
canvas {
  position: fixed;
  inset: 0;
  z-index: -1;
  image-rendering: pixelated;
}
</style>
