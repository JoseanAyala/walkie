<script lang="ts">
import { onMount } from "svelte";
import { api, type Check, type ModelChoice } from "@/lib/api";
import { megabytes } from "@/lib/format";
import Select from "@/lib/Select.svelte";
import Field from "./Field.svelte";
import Status from "./Status.svelte";
import { settings } from "./state.svelte";

let { modelStatus, checks, version }: { modelStatus: string; checks: Check[]; version: string } =
  $props();
const cfg = $derived(settings.cfg);

let models = $state<ModelChoice[]>([]);
const chosen = $derived(models.find((m) => m.key === cfg?.model));
// a model the list doesn't know (set by hand in the config) stays shown
const modelOptions = $derived([
  ...models.map((m) => ({ value: m.key, text: `${m.key} · ${m.note}` })),
  ...(cfg && !models.some((m) => m.key === cfg.model)
    ? [{ value: cfg.model, text: cfg.model }]
    : []),
]);
const STRATEGIES = [
  { value: "paste", text: "Pasting (recommended)" },
  { value: "type", text: "Typing it out" },
];
const save = () => settings.save();

onMount(() => {
  api.listModels().then((m) => (models = m), settings.fail.bind(settings));
});
</script>

{#if cfg}
  <Field label="Speech model">
    <Select label="Speech model" bind:value={cfg.model} options={modelOptions} onchange={save} />
    <div class="hint">
      {#if chosen}about {megabytes(chosen.memory_mb)} of RAM while walkie is open ·{/if}
      {modelStatus}
    </div>
  </Field>
  <Field label="Insert text by">
    <Select
      label="Insert text by"
      bind:value={cfg.inject.strategy}
      options={STRATEGIES}
      onchange={save}
    />
  </Field>
  <Field label="Other audio">
    <label class="cb"
      ><input type="checkbox" bind:checked={cfg.audio.duck_while_recording} onchange={save}>
      {"Lower other audio while dictating"}</label
    >
    <div class="range">
      <input
        type="range"
        min="0"
        max="100"
        step="5"
        aria-label="Volume while recording"
        bind:value={cfg.audio.duck_percent}
        disabled={!cfg.audio.duck_while_recording}
        oninput={() => settings.saveSoon()}
      ><output>{cfg.audio.duck_percent}%</output>
    </div>
    <div class="hint">volume while recording, as % of the current level</div>
  </Field>
{/if}
<h2 class="caps">Checks</h2>
<Status {checks} {version} />

<style>
h2 {
  margin: 24px 0 4px;
}
</style>
