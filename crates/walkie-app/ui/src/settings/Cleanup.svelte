<script lang="ts">
import { words } from "@/lib/format";
import Field from "./Field.svelte";
import { settings } from "./state.svelte";

const cfg = $derived(settings.cfg);
// Edited as text; the config keeps the parsed list.
let en = $state(settings.cfg?.cleanup.fillers_en.join(", ") ?? "");
let es = $state(settings.cfg?.cleanup.fillers_es.join(", ") ?? "");

function edit() {
  if (!cfg) return;
  cfg.cleanup.fillers_en = words(en);
  cfg.cleanup.fillers_es = words(es);
  settings.saveSoon();
}
</script>

{#if cfg}
  <Field label="Cleanup" restart>
    <label class="cb"
      ><input type="checkbox" bind:checked={cfg.cleanup.enabled} onchange={() => settings.save()}>
      {"Enable cleanup"}</label
    >
  </Field>
  <Field label="English" restart>
    <input
      type="text"
      aria-label="English fillers (comma-separated)"
      bind:value={en}
      oninput={edit}
    >
    <div class="tags">
      {#each words(en) as w, i (i)}
        <span class="tag">{w}</span>
      {/each}
    </div>
  </Field>
  <Field label="Spanish" restart>
    <input
      type="text"
      aria-label="Spanish fillers (comma-separated)"
      bind:value={es}
      oninput={edit}
    >
    <div class="tags">
      {#each words(es) as w, i (i)}
        <span class="tag">{w}</span>
      {/each}
    </div>
  </Field>
  <p class="hint">Comma-separated. <span class="req">*</span> applies after restarting walkie.</p>
{/if}

<style>
.tags {
  margin-top: 6px;
  min-height: 22px;
}
</style>
