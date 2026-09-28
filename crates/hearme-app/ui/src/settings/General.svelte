<script lang="ts">
import { onMount } from "svelte";
import { api, type LoginItem, type ModelChoice } from "../lib/api";
import { megabytes } from "../lib/format";
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
      {"Launch hearme at login"}</label
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
      <select bind:value={cfg.audio.input_device} onchange={() => settings.save()}>
        {#each mics as m (m.value)}
          <option value={m.value}>{m.text}</option>
        {/each}
      </select>
      <button title="Refresh the device list" onclick={loadMics}>↻</button>
    </div>
  </Field>
  <Field label="Language" restart>
    <select bind:value={cfg.language} onchange={() => settings.save()}>
      <option value="auto">Auto-detect (en/es)</option>
      <option value="en">English</option>
      <option value="es">Español</option>
    </select>
  </Field>
  <Field label="Model" restart>
    <select bind:value={cfg.model} onchange={() => settings.save()}>
      {#each models as m (m.key)}
        <option value={m.key}>{modelText(m)}</option>
      {/each}
      {#if !models.some((m) => m.key === cfg.model)}
        <option value={cfg.model}>{cfg.model}</option>
      {/if}
    </select>
    {#if chosen}
      <dl class="hint about">
        <dt>disk</dt>
        <dd>
          {megabytes(chosen.size_mb)}
          {chosen.downloaded ? "downloaded" : "to download on the next restart"}
        </dd>
        <dt>memory</dt>
        <dd>about {megabytes(chosen.memory_mb)} of RAM, the whole time hearme is open</dd>
        <dt>runs on</dt>
        <dd>
          your Mac's GPU. On Apple Silicon it shares the Mac's regular memory, so there's no
          separate VRAM: that RAM figure is all it takes.
        </dd>
      </dl>
    {/if}
    <div class="hint">model: {modelStatus}</div>
  </Field>
  <Field label="Injection" restart>
    <select bind:value={cfg.inject.strategy} onchange={() => settings.save()}>
      <option value="paste">Paste (recommended)</option>
      <option value="type">Type keystrokes</option>
    </select>
  </Field>
  <Field label="Ducking" restart>
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
  <p class="hint">* applies after restarting hearme. Everything saves as you change it.</p>
{/if}

<style>
.about {
  display: grid;
  grid-template-columns: 66px 1fr;
  margin: 4px 0 0;
}
.about dd {
  margin: 0;
}
</style>
