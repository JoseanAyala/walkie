<script lang="ts">
// A pixel icon (lib/icons.ts) in the text color; `scale` 1 is one icon pixel
// per CSS pixel, the size of the font's own pixels.
import { ICONS, type IconName, pixels } from "./icons";

let { name, scale = 1 }: { name: IconName; scale?: number } = $props();
const d = $derived(pixels(ICONS[name]));
</script>

<svg
  class="icon"
  width={11 * scale}
  height={11 * scale}
  viewBox="0 0 11 11"
  shape-rendering="crispEdges"
  aria-hidden="true"
><path {d} fill="currentColor" /></svg>

<style>
.icon {
  display: inline-block;
  flex: none;
  vertical-align: -1px;
}
</style>
