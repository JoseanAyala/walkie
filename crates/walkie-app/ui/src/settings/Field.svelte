<script lang="ts">
// A labelled row: chip on the left, controls on the right, hairline below.
// `restart` marks settings that only apply after restarting walkie.
import type { Snippet } from "svelte";

let {
  label,
  restart = false,
  children,
}: { label: string; restart?: boolean; children: Snippet } = $props();
</script>

<div class="f">
  <span class="chip k"
    >{label}
    {#if restart}
      <span class="rs" title="applies after a restart">*</span>
    {/if}</span
  >
  <div class="v">{@render children()}</div>
</div>

<style>
.f {
  display: grid;
  grid-template-columns: 112px 1fr;
  gap: 12px;
  padding: 8px 0;
  border-bottom: 1px solid var(--line);
  align-items: start;
}
.k {
  justify-self: start;
}
.v {
  min-width: 0;
}
.rs {
  opacity: 0.6;
  margin-left: 4px;
}
</style>
