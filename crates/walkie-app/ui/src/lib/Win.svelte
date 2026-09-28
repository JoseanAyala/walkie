<script lang="ts">
// A window: black (or pink, in dark mode) title bar over a grey panel.
import type { Snippet } from "svelte";

let {
  title,
  extra,
  children,
  class: cls = "",
  bodyClass = "",
}: {
  /** no title, no title bar */
  title?: string;
  extra?: Snippet;
  children: Snippet;
  class?: string;
  bodyClass?: string;
} = $props();
</script>

<div class="win {cls}">
  {#if title}
    <div class="bar"><span>{title}</span><span class="grow"></span>{@render extra?.()}</div>
  {/if}
  <div class="body {bodyClass}">{@render children()}</div>
</div>
