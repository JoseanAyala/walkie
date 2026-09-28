<script lang="ts">
import { onMount } from "svelte";
import { api, type Check, type Config, type Pane } from "@/lib/api";
import { glyph } from "@/lib/format";
import { PERMS, turnedOk, verdict } from "./checks";

const ROWS: Record<(typeof PERMS)[number], { name: string; why: string; pane: Pane }> = {
  mic: { name: "Microphone", why: "to hear you — asked on your first dictation", pane: "mic" },
  accessibility: {
    name: "Accessibility",
    why: "for the shortcut and to type text — restart walkie once after granting",
    pane: "accessibility",
  },
  globe: {
    name: "Globe (fn) key",
    why: 'set "Press 🌐 key to" → Do Nothing, so fn only dictates',
    pane: "keyboard",
  },
};

let checks = $state<Check[]>([]);
let cfg = $state<Config | null>(null);
let login = $state(true);
const missing = $derived(PERMS.filter((p) => verdict(checks, p) === "missing").length);
// rows granted since the last check flash once
let flash = $state<string[]>([]);

async function refresh() {
  try {
    const next = await api.getStatus();
    const granted = turnedOk(checks, next);
    checks = next;
    if (granted.length) {
      flash = granted;
      setTimeout(() => (flash = flash.filter((p) => !granted.includes(p))), 900);
    }
  } catch {
    // the next tick tries again
  }
}

onMount(() => {
  api.getConfig().then(
    (c) => (cfg = c),
    () => {},
  );
  refresh();
  const timer = setInterval(() => {
    if (document.visibilityState === "visible") refresh();
  }, 1500);
  return () => clearInterval(timer);
});
</script>

{#snippet keys(
  k: string[],
)}
  {#each k as name, i (i)}
    {#if i}
      {" + "}
    {/if}
    <kbd>{glyph(name)}</kbd>
  {/each}
{/snippet}

<!-- the title bar is hidden: the traffic lights sit on the header, which
     drags the window -->
<div class="app">
  <header data-tauri-drag-region>
    <span class="logo" data-tauri-drag-region>walkie<em>.</em></span>
    <span class="caps" data-tauri-drag-region>setup</span>
  </header>
  <div class="trim"></div>

  <main>
    <section class="intro">
      <h1 class="title">Hold a key, talk, and it's typed.</h1>
      <p>
        Your words land wherever you're writing. Everything runs locally — your voice never
        leaves this Mac.
      </p>
    </section>

    <section>
      <h2><em>01</em> Permissions <span class="caps">{missing ? `${missing} to go` : "all set"}</span></h2>
      {#each PERMS as p (p)}
        {@const v = verdict(checks, p)}
        {#if v !== "absent"}
          <div class="perm">
            <span
              class="chip"
              class:ghost={v === "pending"}
              class:warn={v === "missing"}
              class:flash={flash.includes(p)}
              >{v === "ok" ? "OK" : v === "pending" ? ".." : "!!"}</span
            >
            <span><b>{ROWS[p].name}</b><small>{ROWS[p].why}</small></span>
            <button class:hidden={v === "ok"} onclick={() => api.openSettingsPane(ROWS[p].pane)}>
              Open Settings
            </button>
          </div>
        {/if}
      {/each}
    </section>

    <section class="try">
      <h2><em>02</em> Try it</h2>
      {#if cfg}
        {@const h = cfg.hotkeys}
        {#if h.dictate.length}
          <p>Click into any text field, hold {@render keys(h.dictate)}, speak, release.</p>
        {/if}
        {#if h.polish.length}
          <p>{@render keys(h.polish)} polishes the selected text, or the whole field.</p>
        {/if}
      {:else}
        <p>Click into any text field, hold the Dictate key, speak, release.</p>
      {/if}
      <p class="muted">Change them in Settings → General.</p>
    </section>
  </main>

  <!-- the way out stays on screen however tall the rest gets -->
  <footer>
    <div class="row">
      <label class="cb"
        ><input type="checkbox" bind:checked={login}> {"Launch walkie at login"}</label
      >
      <span class="grow"></span>
      <button class="primary" onclick={() => api.finishOnboarding(login)}>
        I've granted everything — finish
      </button>
    </div>
    {#if missing}
      <div class="hint">
        {missing}
        still missing — you can finish anyway; Settings → Status keeps checking.
      </div>
    {/if}
    <div class="hint">Running walkie from a terminal? Grant these to the terminal app instead.</div>
  </footer>
</div>

<style>
.app {
  min-height: 100vh;
  display: flex;
  flex-direction: column;
  background: var(--sheet);
}
header {
  display: flex;
  gap: 14px;
  align-items: baseline;
  /* clear of the traffic lights (tauri.conf.json's trafficLightPosition) */
  padding: 12px 24px 12px 86px;
  background: var(--bg);
}
main {
  flex: 1;
  padding: 8px 30px 12px;
}
section {
  padding: 18px 0;
  border-bottom: 1px solid var(--rule);
}
section:last-child {
  border-bottom: 0;
}
.intro p {
  margin: 8px 0 0;
  color: var(--muted);
}
h2 {
  display: flex;
  align-items: baseline;
  gap: 6px;
  margin: 0 0 8px;
  font: 400 26px / 30px var(--serif);
}
h2 em {
  color: var(--red);
  font-size: 18px;
}
h2 .caps {
  margin-left: auto;
}
.row {
  display: flex;
  gap: 8px;
  align-items: center;
}
.grow {
  flex: 1;
}
.perm {
  display: grid;
  grid-template-columns: 34px 1fr auto;
  gap: 12px;
  align-items: center;
  padding: 8px 0;
  border-top: 1px solid var(--rule);
}
.perm > .chip {
  text-align: center;
}
/* just granted: the chip flashes red twice */
.perm > .chip.flash {
  animation: granted 600ms steps(1) both;
}
@keyframes granted {
  0%,
  50% {
    background: var(--red);
    color: var(--on-red);
  }
  25%,
  75% {
    background: var(--ink);
    color: var(--bg);
  }
}
b {
  font-weight: 500;
}
small {
  display: block;
  font-size: 12px;
  line-height: 17px;
  color: var(--muted);
}
.hidden {
  visibility: hidden;
}
.try p {
  margin: 0 0 6px;
}
footer {
  position: sticky;
  bottom: 0;
  padding: 12px 30px 14px;
  border-top: 1px solid var(--ink);
  background: var(--lime);
}
</style>
