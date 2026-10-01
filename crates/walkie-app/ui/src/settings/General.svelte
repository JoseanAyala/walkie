<script lang="ts">
import { onMount } from "svelte";
import { type Appearance, api, type LoginItem } from "@/lib/api";
import Select from "@/lib/Select.svelte";
import { applyTheme } from "@/lib/theme";
import Field from "./Field.svelte";
import Shortcuts from "./Shortcuts.svelte";
import { settings } from "./state.svelte";

const cfg = $derived(settings.cfg);

// ---- launch at login: the OS (or, on Linux, the autostart file) owns
// this, so it's applied on toggle and read back live (never saved in the
// config)
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

const LANGUAGES = [
  { value: "auto", text: "Auto-detect (en/es)" },
  { value: "en", text: "English" },
  { value: "es", text: "Español" },
];
const APPEARANCES: Appearance[] = ["system", "light", "dark"];
const save = () => settings.save();

function appearance(a: Appearance) {
  if (!cfg) return;
  cfg.theme.appearance = a;
  applyTheme($state.snapshot(cfg.theme));
  save();
}

onMount(() => {
  loadLogin();
  loadMics();
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
  <Field label="Shortcuts"><Shortcuts /></Field>
  <Field label="Microphone">
    <Select
      label="Microphone"
      bind:value={cfg.audio.input_device}
      options={mics}
      onchange={save}
    />
  </Field>
  <Field label="Language">
    <Select label="Language" bind:value={cfg.language} options={LANGUAGES} onchange={save} />
  </Field>
  <Field label="Appearance">
    <div class="inline">
      {#each APPEARANCES as a (a)}
        <button aria-pressed={cfg.theme.appearance === a} onclick={() => appearance(a)}>{a}</button>
      {/each}
    </div>
  </Field>
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
{/if}
