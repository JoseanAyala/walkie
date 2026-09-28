<script lang="ts">
import { getVersion } from "@tauri-apps/api/app";
import { onMount } from "svelte";
import { api, type Check, on } from "../lib/api";
import Win from "../lib/Win.svelte";
import Cleanup from "./Cleanup.svelte";
import General from "./General.svelte";
import History from "./History.svelte";
import Polish from "./Polish.svelte";
import Status from "./Status.svelte";
import { settings } from "./state.svelte";

const TABS = {
  general: { name: "General", sub: "how hearme listens and types" },
  status: { name: "Status", sub: "what hearme needs to work" },
  cleanup: { name: "Cleanup", sub: "drop filler words before typing" },
  polish: { name: "Polish", sub: "rewrite with any CLI: transcript on stdin, result on stdout" },
  history: { name: "History", sub: "your dictations, stored only on this Mac" },
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
    <span class="os">HEARME.OS1</span><span class="grow"></span>
    <span class="chip ghost">model: {modelStatus}</span>
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
    <Win title="hearme {version}" class="about" bodyClass="muted">
      Runs locally.<br>Your voice never<br>leaves this Mac.
    </Win>
  </div>

  <Win title="{TABS[tab].name} 1.1" class="main" bodyClass="scroll">
    {#snippet extra()}
      <span class="saved">{settings.saved ? "saved ✓" : "autosave"}</span>
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
      {/if}
    {/if}
  </Win>
  {#if settings.pending.length}
    <div class="restart">
      <span>Restart to apply: {settings.pending.join(", ")}</span>
      <button class="primary" onclick={() => api.restartApp()}>Restart hearme</button>
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
