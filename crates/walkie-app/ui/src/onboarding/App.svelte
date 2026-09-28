<script lang="ts">
import { onMount } from "svelte";
import { api, type Check, type Config, type Pane } from "@/lib/api";
import { glyph } from "@/lib/format";
import Icon from "@/lib/Icon.svelte";
import Win from "@/lib/Win.svelte";
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

<!-- the title bar is hidden: the traffic lights sit on the top strip, and
     it and the bare desk around the windows drag the window -->
<div class="desktop" data-tauri-drag-region>
  <div class="top" data-tauri-drag-region>
    <span class="os" data-tauri-drag-region>WALKIE.OS1</span><span class="grow" data-tauri-drag-region></span
    ><span class="chip ghost" data-tauri-drag-region>setup</span>
  </div>

  <Win title="Welcome" bodyClass="intro">
    <span class="chip big">Walkie</span>
    <span
      >Hold a key, talk, and your words are typed wherever you're writing. Everything runs locally —
      your voice never leaves this Mac.</span
    >
  </Win>

  <Win title="Permissions">
    {#snippet extra()}
      <span>{missing ? `${missing} to go` : "all set"}</span>
    {/snippet}
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
          <span><b><Icon name={p} /> {ROWS[p].name}</b><small>{ROWS[p].why}</small></span>
          <button class:hidden={v === "ok"} onclick={() => api.openSettingsPane(ROWS[p].pane)}>
            Open Settings
          </button>
        </div>
      {/if}
    {/each}
  </Win>

  <Win title="Try it" bodyClass="try">
    {#if cfg}
      {@const h = cfg.hotkeys}
      {#if h.dictate.length}
        <p>Click into any text field, hold {@render keys(h.dictate)}, speak, release.</p>
      {/if}
      {#if h.polish.length}
        <p>{@render keys(h.polish)} dictates and polishes.</p>
      {/if}
    {:else}
      <p>Click into any text field, hold the Dictate key, speak, release.</p>
    {/if}
    <p class="muted">Change them in Settings → General.</p>
  </Win>

  <Win title="Finish" class="finish">
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
        <span class="chip warn">!!</span>
        {missing}
        still missing — you can finish anyway; Settings → Status keeps checking.
      </div>
    {/if}
    <div class="hint">Running walkie from a terminal? Grant these to the terminal app instead.</div>
  </Win>
</div>

<style>
:global(body) {
  overflow: auto;
}
.desktop {
  min-height: 100vh;
  padding: 10px 12px 12px;
  display: flex;
  flex-direction: column;
  gap: 10px;
}
.top,
.row {
  display: flex;
  gap: 8px;
  align-items: center;
}
/* clear of the traffic lights (tauri.conf.json's trafficLightPosition) */
.top {
  padding-left: 70px;
}
.grow {
  flex: 1;
}
.desktop :global(.body) {
  line-height: var(--lh-tight);
}
.desktop :global(.intro) {
  display: flex;
  gap: 12px;
  align-items: center;
}
.desktop :global(.intro .chip.big) {
  flex: none;
}
.perm {
  display: grid;
  grid-template-columns: 34px 1fr auto;
  gap: 10px;
  align-items: center;
  padding: 6px 0;
  border-bottom: 1px solid var(--line);
}
.perm:last-child {
  border-bottom: 0;
}
.perm > .chip {
  text-align: center;
}
/* just granted: the chip blinks in the accent color */
.perm > .chip.flash {
  animation: granted 600ms steps(1) both;
}
@keyframes granted {
  0%,
  50% {
    background: var(--accent);
    color: var(--accent-fg);
  }
  25%,
  75% {
    background: var(--chip);
    color: var(--chip-fg);
  }
}
small {
  display: block;
  color: var(--muted);
}
.hidden {
  visibility: hidden;
}
.desktop :global(.try p) {
  margin: 0 0 6px;
}
.desktop :global(kbd) {
  background: var(--paper);
  border: 2px solid var(--line);
  padding: 0 4px;
}
/* the way out stays on screen however tall the rest gets */
.desktop :global(.finish) {
  position: sticky;
  bottom: 12px;
  margin-top: auto;
}
button.primary {
  padding: 4px 10px;
}
</style>
