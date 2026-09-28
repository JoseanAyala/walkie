<script lang="ts">
import { onMount } from "svelte";
import { api, type LoginItem, type ModelChoice } from "@/lib/api";
import { megabytes } from "@/lib/format";
import Select from "@/lib/Select.svelte";
import Field from "./Field.svelte";
import Shortcuts from "./Shortcuts.svelte";
import { settings } from "./state.svelte";

let { modelStatus }: { modelStatus: string } = $props();
const cfg = $derived(settings.cfg);

// ---- launch at login: macOS owns this, so it's applied on toggle and
// read back live (never saved in the config)
let login = $state<LoginItem | null>(null);
async function loadLogin() {
  try {
    login = await api.getLaunchAtLogin();
  } catch (e) {
    settings.fail(e);
  }
}
async function toggleLogin(e: Event) {
  const enabled = (e.currentTarget as HTMLInputElement).checked;
  try {
    login = await api.setLaunchAtLogin(enabled);
  } catch (err) {
    settings.fail(err);
    loadLogin();
  }
}

// ---- microphone: a saved device that's unplugged stays listed (and
// selected) instead of silently turning back into "System default"
let mics = $state<{ value: string; text: string }[]>([]);
async function loadMics() {
  if (!cfg) return;
  const { default: def, devices } = await api.listInputDevices();
  const want = cfg.audio.input_device;
  const list = [
    { value: "", text: `System default (${def || "none"})` },
    ...devices.map((d) => ({ value: d, text: d })),
  ];
  if (want && !devices.includes(want)) list.push({ value: want, text: `${want} (not connected)` });
  mics = list;
}

let models = $state<ModelChoice[]>([]);
const modelText = (m: ModelChoice) => `${m.key} · ${m.note}`;
const chosen = $derived(models.find((m) => m.key === cfg?.model));
// a model the list doesn't know (set by hand in the config) stays shown
const modelOptions = $derived([
  ...models.map((m) => ({ value: m.key, text: modelText(m) })),
  ...(cfg && !models.some((m) => m.key === cfg.model)
    ? [{ value: cfg.model, text: cfg.model }]
    : []),
]);
const LANGUAGES = [
  { value: "auto", text: "Auto-detect (en/es)" },
  { value: "en", text: "English" },
  { value: "es", text: "Español" },
];
const STRATEGIES = [
  { value: "paste", text: "Paste (recommended)" },
  { value: "type", text: "Type keystrokes" },
];
const save = () => settings.save();

onMount(() => {
  loadLogin();
  loadMics();
  api.listModels().then((m) => (models = m), settings.fail.bind(settings));
  // e.g. back from System Settings, or a mic plugged in meanwhile
  const focus = () => {
    loadLogin();
    loadMics();
  };
  addEventListener("focus", focus);
  return () => removeEventListener("focus", focus);
});
</script>

{#if cfg}
  <Field label="Startup">
    <label class="cb"
      ><input type="checkbox" checked={login?.on ?? false} onchange={toggleLogin}>
      {"Launch walkie at login"}</label
    >
    {#if login?.hint || login?.status === "requires_approval"}
      <div class="hint">
        {login?.hint ?? ""}
        {#if login?.status === "requires_approval"}
          <button onclick={() => api.openSettingsPane("loginitems")}>Open Login Items</button>
        {/if}
      </div>
    {/if}
  </Field>
  <Field label="Shortcuts"><Shortcuts /></Field>
  <Field label="Microphone">
    <div class="inline">
      <Select
        label="Microphone"
        bind:value={cfg.audio.input_device}
        options={mics.map((m) => ({ value: m.value, text: m.text }))}
        onchange={save}
      />
      <button title="Refresh the device list" onclick={loadMics}>↻</button>
    </div>
  </Field>
  <Field label="Language">
    <Select label="Language" bind:value={cfg.language} options={LANGUAGES} onchange={save} />
  </Field>
  <Field label="Model">
    <Select label="Model" bind:value={cfg.model} options={modelOptions} onchange={save} />
    {#if chosen}
      <div class="hint">about {megabytes(chosen.memory_mb)} of RAM while walkie is open</div>
    {/if}
    <div class="hint">model: {modelStatus}</div>
  </Field>
  <Field label="Injection">
    <Select label="Injection" bind:value={cfg.inject.strategy} options={STRATEGIES} onchange={save} />
  </Field>
  <Field label="Ducking">
    <label class="cb"
      ><input
        type="checkbox"
        bind:checked={cfg.audio.duck_while_recording}
        onchange={() => settings.save()}
      >
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
