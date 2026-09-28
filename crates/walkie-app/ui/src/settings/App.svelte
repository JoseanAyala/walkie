<script lang="ts">
import { getVersion } from "@tauri-apps/api/app";
import { onMount } from "svelte";
import { api, type Check, on } from "@/lib/api";
import type { Mode } from "@/lib/palette";
import { applyTheme, modeOf } from "@/lib/theme";
import Win from "@/lib/Win.svelte";
import Cleanup from "./Cleanup.svelte";
import General from "./General.svelte";
import History from "./History.svelte";
import Polish from "./Polish.svelte";
import Status from "./Status.svelte";
import { settings } from "./state.svelte";
import Theme from "./Theme.svelte";

const TABS = {
  general: { name: "General", sub: "how walkie listens and types" },
  status: { name: "Status", sub: "what walkie needs to work" },
  cleanup: { name: "Cleanup", sub: "drop filler words before typing" },
  polish: { name: "Polish", sub: "rewrite with any CLI: transcript on stdin, result on stdout" },
  history: { name: "History", sub: "your dictations, stored only on this Mac" },
  theme: { name: "Theme", sub: "colors, light and dark, your own palettes" },
} as const;
type Tab = keyof typeof TABS;

let tab = $state<Tab>("general");
let checks = $state<Check[]>([]);
let modelStatus = $state("…");
let version = $state("");

// The speech model loading or downloading isn't something to fix, so it
// doesn't count towards the badge.
const problems = $derived(checks.filter((c) => !c.ok && c.id !== "model").length);

async function loadStatus() {
  try {
    checks = await api.getStatus();
    const model = checks.find((c) => c.id === "model");
    if (model) modelStatus = model.detail;
  } catch (e) {
    settings.fail(e);
  }
}

// the light | dark switch: the mode showing now is filled; picking one pins
// it (Theme → Appearance → system follows macOS again)
const mode = $derived(settings.cfg ? modeOf(settings.cfg.theme) : null);
function setMode(m: Mode) {
  const cfg = settings.cfg;
  if (!cfg || cfg.theme.appearance === m) return;
  cfg.theme.appearance = m;
  applyTheme($state.snapshot(cfg.theme));
  settings.save();
}

function show(t: Tab) {
  tab = t;
  if (t === "status") loadStatus();
}

onMount(() => {
  settings.load();
  loadStatus();
  getVersion().then(
    (v) => (version = v),
    () => {},
  );
  // the open Status tab every 2s; otherwise just the badge, every 10s
  let ticks = 0;
  const timer = setInterval(() => {
    if (document.visibilityState !== "visible") return;
    if (tab === "status" || ++ticks % 5 === 0) loadStatus();
  }, 2000);
  const offs = [
    on("show-tab", (t) => {
      if (t in TABS) show(t as Tab);
    }),
    on("model-status", (s) => (modelStatus = s)),
    on("download-progress", (p) => (modelStatus = `downloading ${p}%`)),
    // Errors are otherwise only shown on the overlay, which most of the
    // time isn't visible (e.g. the hotkey listener's "still loading the
    // speech model" warning when no recording ever started).
    on("app-error", (e) => settings.fail(e)),
  ];
  return () => {
    clearInterval(timer);
    for (const off of offs) off.then((f) => f());
  };
});
</script>

<div class="desktop">
  <div class="top">
    <span class="os">WALKIE.OS1</span><span class="grow"></span>
    {#if settings.cfg}
      <div class="modes" role="group" aria-label="Light or dark">
        {#each ["light", "dark"] as const as m (m)}
          <button class:on={mode === m} aria-pressed={mode === m} onclick={() => setMode(m)}
            >{m}</button
          >
        {/each}
      </div>
    {/if}
  </div>

  <div class="side">
    <Win title="Menu">
      <nav>
        {#each Object.entries(TABS) as [t, { name }] (t)}
          <button class:on={tab === t} aria-label={name} onclick={() => show(t as Tab)}>
            {name}
            {#if t === "status" && problems > 0}
              <span class="badge" aria-hidden="true">{problems}</span>
            {/if}
          </button>
        {/each}
      </nav>
    </Win>
    <Win title="Walkie {version}" class="about" bodyClass="muted">
      Runs locally.<br>Your voice never<br>leaves this Mac.
    </Win>
  </div>

  <Win title={TABS[tab].name} class="main" bodyClass="scroll">
    {#snippet extra()}
      <!-- every change saves itself; this just confirms it happened -->
      {#if settings.saved}<span class="saved">saved ✓</span>{/if}
    {/snippet}
    {#if settings.error}
      <div class="error">
        <span class="chip warn">ERR</span><span class="text">{settings.error}</span>
        <button onclick={() => (settings.error = "")}>✕</button>
      </div>
    {/if}
    <div class="head">
      <span class="chip big">{TABS[tab].name}</span><span class="muted">{TABS[tab].sub}</span>
    </div>
    {#if tab === "status"}
      <Status {checks} />
    {:else if settings.cfg}
      {#if tab === "general"}
        <General {modelStatus} />
      {:else if tab === "cleanup"}
        <Cleanup />
      {:else if tab === "polish"}
        <Polish />
      {:else if tab === "history"}
        <History />
      {:else if tab === "theme"}
        <Theme />
      {/if}
    {/if}
  </Win>
  {#if settings.pending.length}
    <div class="restart">
      <span>Restart to apply: {settings.pending.join(", ")}</span>
      <button class="primary" onclick={() => api.restartApp()}>Restart walkie</button>
    </div>
  {/if}
</div>

<style>
.desktop {
  height: 100vh;
  display: grid;
  grid-template-columns: 158px 1fr;
  grid-template-rows: auto 1fr auto;
  gap: 10px 12px;
  padding: 10px 12px 12px;
}
.top {
  grid-column: 1 / -1;
  display: flex;
  gap: 8px;
  align-items: center;
}
.grow {
  flex: 1;
}
.modes {
  display: flex;
}
.modes button + button {
  border-left: 0;
}
/* the accent, not the chip color: a black "light" read as dark */
.modes button.on {
  background: var(--accent);
  color: var(--accent-fg);
}
.side {
  grid-row: 2 / 4;
  display: flex;
  flex-direction: column;
  gap: 10px;
  min-height: 0;
}
nav {
  margin: -4px -6px;
}
nav button {
  display: flex;
  width: 100%;
  border: 0;
  background: none;
  text-align: left;
  padding: 2px 6px;
  line-height: 22px;
}
nav button:hover:not(.on) {
  background: var(--tag);
}
nav button.on {
  background: var(--chip);
  color: var(--chip-fg);
}
.badge {
  margin-left: auto;
  background: var(--warn);
  color: var(--accent-fg);
  padding: 0 5px;
}
.side :global(.about) {
  margin-top: auto;
}
.side :global(.about .body) {
  line-height: 16px;
}
:global(.main) {
  min-height: 0;
}
:global(.main > .body.scroll) {
  flex: 1;
  overflow: auto;
  padding: 12px 14px 16px;
}
.saved {
  opacity: 0.7;
}
.error {
  display: flex;
  gap: 8px;
  align-items: center;
  margin-bottom: 10px;
  border: 2px solid var(--line);
  background: var(--paper);
  padding: 2px 4px;
}
.error .text {
  flex: 1;
  line-height: 16px;
  -webkit-user-select: text;
  user-select: text;
}
.error button {
  border: 0;
  background: none;
  padding: 0 4px;
}
.restart {
  grid-column: 2;
  display: flex;
  gap: 8px;
  align-items: center;
  padding: 4px 8px;
  border: 2px solid var(--line);
  background: var(--paper);
  box-shadow: 4px 4px 0 var(--line);
}
.restart span {
  flex: 1;
}
</style>
