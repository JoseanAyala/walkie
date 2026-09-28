<script lang="ts">
// A labelled row: the name on the left, controls on the right, a hairline
// above. `restart` marks settings that only apply after restarting walkie.
import type { Snippet } from "svelte";

let {
  label,
  restart = false,
  children,
}: { label: string; restart?: boolean; children: Snippet } = $props();
</script>

<div class="f">
  <span class="k"
    >{label}{#if restart}<span class="req" title="applies after a restart">*</span>{/if}</span
  >
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
.req {
  margin-left: 3px;
}
</style>
