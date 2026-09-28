<script lang="ts">
import { getVersion } from "@tauri-apps/api/app";
import { onMount } from "svelte";
import { api, type Check, on } from "@/lib/api";
import Icon from "@/lib/Icon.svelte";
import Win from "@/lib/Win.svelte";
import General from "./General.svelte";
import History from "./History.svelte";
import Polish from "./Polish.svelte";
import Status from "./Status.svelte";
import { settings } from "./state.svelte";
import Theme from "./Theme.svelte";

// `file` names the main window, like a document open on the desk; the
// page itself carries the tab's name
const TABS = {
  general: { name: "General", file: "general.cfg", sub: "how walkie listens and types" },
  status: { name: "Status", file: "status.log", sub: "what walkie needs to work" },
  polish: {
    name: "Polish",
    file: "polish.cfg",
    sub: "rewrite with Apple's on-device model or any CLI",
  },
  history: { name: "History", file: "history.db", sub: "your dictations, stored only on this Mac" },
  theme: { name: "Theme", file: "theme.cfg", sub: "light, dark, or as macOS is" },
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

<!-- the title bar is hidden: the traffic lights sit on the top strip, and
     it and the bare desk around the windows drag the window -->
<div class="desktop" data-tauri-drag-region>
  <div class="top" data-tauri-drag-region>
    <span class="os" data-tauri-drag-region>WALKIE.OS1</span>
  </div>

  <div class="side" data-tauri-drag-region>
    <Win title="Menu">
      <nav>
        {#each Object.entries(TABS) as [t, { name }] (t)}
          <button class:on={tab === t} aria-label={name} onclick={() => show(t as Tab)}>
            <Icon name={t as Tab} />{name}
            {#if t === "status" && problems > 0}
              <span class="badge" aria-hidden="true">{problems}</span>
            {/if}
          </button>
        {/each}
      </nav>
    </Win>
    <Win class="about" bodyClass="muted">
      Runs locally. Your voice never leaves this Mac.
    </Win>
  </div>

  <!-- one column, so the main window ends level with the side ones unless
       the restart bar takes its place at the bottom -->
  <div class="right" data-tauri-drag-region>
  {#key tab}
  <Win title={TABS[tab].file} class="main glitch" bodyClass="scroll">
    {#snippet extra()}
      <!-- every change saves itself; this just confirms it happened -->
      {#if settings.saved}{#key settings.saves}<span class="saved">saved ✓</span>{/key}{/if}
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
      <Status {checks} {version} />
    {:else if settings.cfg}
      {#if tab === "general"}
        <General {modelStatus} />
      {:else if tab === "polish"}
        <Polish />
      {:else if tab === "history"}
        <History />
      {:else if tab === "theme"}
        <Theme />
      {/if}
    {/if}
  </Win>
  {/key}
  {#if settings.pending.length}
    <div class="restart">
      <span>Restart to apply: {settings.pending.join(", ")}</span>
      <button class="primary" onclick={() => api.restartApp()}>Restart walkie</button>
    </div>
  {/if}
  </div>
</div>

<style>
.desktop {
  height: 100vh;
  display: grid;
  grid-template-columns: 158px 1fr;
  grid-template-rows: auto 1fr;
  gap: 10px 12px;
  padding: 10px 12px 12px;
}
.top {
  grid-column: 1 / -1;
  /* clear of the traffic lights (tauri.conf.json's trafficLightPosition) */
  padding-left: 70px;
  display: flex;
  gap: 8px;
  align-items: center;
}
.side,
.right {
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
  gap: 8px;
  align-items: center;
  width: 100%;
  border: 0;
  background: none;
  box-shadow: none;
  transform: none;
  text-align: left;
  padding: 2px 6px;
  line-height: var(--lh);
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
  line-height: var(--lh-tight);
}
:global(.main) {
  flex: 1;
  min-height: 0;
  /* each tab opens its window: the hard shadow jumps about, the title bar
     flickers a frame */
  animation: jump 180ms steps(1) both;
}
:global(.main > .bar) {
  animation: bar-flick 120ms steps(1) both;
}
:global(.main > .body.scroll) {
  flex: 1;
  overflow: auto;
  padding: 12px 14px 16px;
}
.saved {
  animation: flash-out 1200ms steps(3) both;
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
  line-height: var(--lh-tight);
  -webkit-user-select: text;
  user-select: text;
}
.error button {
  border: 0;
  background: none;
  box-shadow: none;
  transform: none;
  padding: 0 4px;
}
.restart {
  animation: rise 150ms steps(3) both;
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
