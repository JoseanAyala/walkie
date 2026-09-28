<script lang="ts">
import { api, type Check } from "@/lib/api";

let { checks, version }: { checks: Check[]; version: string } = $props();

const fix = (c: Check) =>
  c.fix === "restart" ? api.restartApp() : api.openSettingsPane(c.fix ?? "");
</script>

{#each checks as c (c.id)}
  <div class="chk">
    <span class="chip" class:warn={!c.ok}>{c.ok ? "OK" : "!!"}</span>
    <span>{c.label}<small>{c.detail}</small></span>
    {#if c.fix}
      <button onclick={() => fix(c)}>{c.fix === "restart" ? "Restart walkie" : "Fix"}</button>
    {:else}
      <span></span>
    {/if}
  </div>
{/each}
<p class="hint">Walkie {version}. Refreshes every 2 seconds. Details are also in ~/Library/Logs/walkie/walkie.log.</p>

<style>
.chk {
  display: grid;
  grid-template-columns: 42px 1fr auto;
  gap: 10px;
  align-items: start;
  padding: 8px 0;
  border-bottom: 1px solid var(--line);
}
.chip {
  text-align: center;
}
small {
  display: block;
  color: var(--muted);
  line-height: var(--lh-tight);
}
</style>
