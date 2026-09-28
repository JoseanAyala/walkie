<script lang="ts">
import { getVersion } from "@tauri-apps/api/app";
import { onMount } from "svelte";
import { api, type Check, on } from "@/lib/api";
import Advanced from "./Advanced.svelte";
import General from "./General.svelte";
import History from "./History.svelte";
import Polish from "./Polish.svelte";
import Problems from "./Problems.svelte";
import { problems } from "./problems";
import { settings } from "./state.svelte";

// in menu order; each page is titled with its number
const TABS = {
  general: { name: "General", sub: "Your shortcuts, microphone and language." },
  polish: { name: "Polish", sub: "Rewrite with Apple's on-device model or any CLI." },
  history: { name: "History", sub: "Your dictations, stored only on this Mac." },
  advanced: { name: "Advanced", sub: "The speech model, how text gets in, and checks." },
} as const;
const num = (t: Tab) => String(Object.keys(TABS).indexOf(t) + 1).padStart(2, "0");
type Tab = keyof typeof TABS;

let tab = $state<Tab>("general");
let checks = $state<Check[]>([]);
let modelStatus = $state("…");
let version = $state("");

const needs = $derived(problems(checks).length);

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
  if (t === "advanced") loadStatus();
}

onMount(() => {
  settings.load();
  loadStatus();
  getVersion().then(
    (v) => (version = v),
    () => {},
  );
  // every 2s while something's missing (so a grant shows at once) or the
  // Advanced tab (with every check) is open; otherwise every 10s
  let ticks = 0;
  const timer = setInterval(() => {
    if (document.visibilityState !== "visible") return;
    if (needs || tab === "advanced" || ++ticks % 5 === 0) loadStatus();
  }, 2000);
  const offs = [
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

<!-- the title bar is hidden: the traffic lights sit on the header, and it
     and the menu's empty space drag the window -->
<div class="app">
  <header data-tauri-drag-region>
    <span class="logo" data-tauri-drag-region>walkie<em>.</em></span>
    <span class="caps" data-tauri-drag-region>on-device dictation</span>
    <span class="grow" data-tauri-drag-region></span>
    <!-- every change saves itself; this just confirms it happened -->
    {#if settings.saved}{#key settings.saves}<span class="saved">Saved</span>{/key}{/if}
  </header>

  <!-- the menu: a lavender block over a red one -->
  <aside>
    <nav data-tauri-drag-region>
      {#each Object.keys(TABS) as t (t)}
        <button class:on={tab === t} aria-label={TABS[t as Tab].name} onclick={() => show(t as Tab)}>
          <span class="n">{num(t as Tab)}</span>{TABS[t as Tab].name}
        </button>
      {/each}
    </nav>
    <p class="note">Runs locally. Your voice never leaves this Mac.</p>
  </aside>

  <main>
    <div class="trim"></div>
    {#key tab}
      <div class="page">
        {#if settings.error}
          <div class="error">
            <span class="chip warn">ERR</span><span class="text">{settings.error}</span>
            <button class="link" onclick={() => (settings.error = "")}>dismiss</button>
          </div>
        {/if}
        <Problems {checks} />
        <div class="head">
          <h1 class="title"><em>{num(tab)}</em> {TABS[tab].name}</h1>
          <div class="muted">{TABS[tab].sub}</div>
        </div>
        {#if tab === "advanced"}
          <Advanced {modelStatus} {checks} {version} />
        {:else if settings.cfg}
          {#if tab === "general"}
            <General />
          {:else if tab === "polish"}
            <Polish />
          {:else if tab === "history"}
            <History />
          {/if}
        {/if}
      </div>
    {/key}
  </main>
</div>

<style>
.app {
  height: 100vh;
  display: grid;
  grid-template-columns: 200px 1fr;
  grid-template-rows: auto 1fr;
}
header {
  grid-column: 1 / -1;
  display: flex;
  gap: 14px;
  align-items: baseline;
  /* clear of the traffic lights (tauri.conf.json's trafficLightPosition) */
  padding: 12px 24px 12px 86px;
  border-bottom: 1px solid var(--ink);
}
.grow {
  flex: 1;
}
.saved {
  font-size: 12px;
  color: var(--muted);
  animation: flash-out 1200ms ease both;
}
aside {
  display: flex;
  flex-direction: column;
  min-height: 0;
  border-right: 1px solid var(--ink);
}
nav {
  flex: 1;
  padding: 14px 0;
  background: var(--lav);
}
nav button {
  display: flex;
  align-items: center;
  width: 100%;
  border: 0;
  background: none;
  padding: 7px 20px;
  font: 500 13px / 20px var(--sans);
  text-align: left;
}
nav button:hover:not(.on) {
  background: rgb(255 255 255 / 0.3);
}
nav button.on {
  background: var(--ink);
  color: var(--bg);
}
.n {
  width: 22px;
  font-size: 10px;
  opacity: 0.55;
}
.note {
  margin: 0;
  padding: 14px 20px 16px;
  background: var(--red);
  color: var(--on-red);
  border-top: 1px solid var(--ink);
  font: italic 17px / 20px var(--serif);
}
main {
  display: flex;
  flex-direction: column;
  min-height: 0;
  background: var(--sheet);
}
.page {
  flex: 1;
  overflow: auto;
  padding: 26px 34px 30px;
  animation: fade 150ms ease both;
}
.head {
  margin-bottom: 18px;
}
.head .muted {
  margin-top: 2px;
}
.error {
  display: flex;
  gap: 10px;
  align-items: center;
  margin-bottom: 16px;
  padding: 6px 10px;
  border: 1px solid var(--red);
}
.error .text {
  flex: 1;
  line-height: 17px;
  -webkit-user-select: text;
  user-select: text;
}
</style>
