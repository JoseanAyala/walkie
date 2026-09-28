<script lang="ts">
// What walkie still needs from macOS, at the top of every page: on the
// first launch that's the whole setup. Gone once everything is granted.
import { api, type Check } from "@/lib/api";
import { fixLabel, problems } from "./problems";

let { checks }: { checks: Check[] } = $props();
const list = $derived(problems(checks));
</script>

{#if list.length}
  <section class="needs" aria-label="Needs attention">
    <div class="caps">Walkie needs</div>
    {#each list as c (c.id)}
      <div class="row">
        <span class="chip warn">!!</span>
        <span>{c.label}<small>{c.detail}</small></span>
        {#if c.fix}
          <button onclick={() => api.openSettingsPane(c.fix ?? "")}>{fixLabel(c)}</button>
        {:else}
          <span></span>
        {/if}
      </div>
    {/each}
  </section>
{/if}

<style>
.needs {
  margin-bottom: 20px;
  padding: 10px 14px 4px;
  border: 1px solid var(--red);
  animation: enter 150ms ease both;
}
.row {
  display: grid;
  grid-template-columns: 38px 1fr auto;
  gap: 14px;
  align-items: start;
  padding: 8px 0;
  font-weight: 500;
}
.row + .row {
  border-top: 1px solid var(--rule);
}
.chip {
  text-align: center;
  margin-top: 1px;
}
small {
  display: block;
  font-size: 12px;
  font-weight: 400;
  line-height: 17px;
  color: var(--muted);
}
</style>
