<script lang="ts">
// A labelled row: the name on the left, controls on the right, a hairline
// above.
import type { Snippet } from "svelte";

let { label, children }: { label: string; children: Snippet } = $props();
</script>

<div class="f">
  <span class="k"
>{label}</span>
  <div class="v">{@render children()}</div>
</div>

<style>
.f {
  display: grid;
  grid-template-columns: 150px 1fr;
  gap: 16px;
  padding: 14px 0;
  border-top: 1px solid var(--rule);
  align-items: start;
}
.k {
  font-weight: 500;
  line-height: 28px;
}
.v {
  min-width: 0;
  /* one-line controls sit level with the name */
  line-height: 28px;
}
.v :global(.hint) {
  line-height: 17px;
}
</style>
