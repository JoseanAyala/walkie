<script lang="ts">
import type { Appearance } from "@/lib/api";
import {
  badName,
  DEFAULT_THEME,
  type Palette,
  PRESETS,
  type Preset,
  parseColors,
  suggest,
  TOKENS,
  tokensFor,
} from "@/lib/palette";
import { applyTheme, modeOf } from "@/lib/theme";
import Field from "./Field.svelte";
import { settings } from "./state.svelte";

const cfg = $derived(settings.cfg);
const mode = $derived(cfg ? modeOf(cfg.theme) : "light");
const themes = $derived<Preset[]>([
  ...PRESETS,
  ...(cfg?.theme.custom ?? []).map((c) => ({ ...c, key: c.name, feel: "imported" })),
]);
const APPEARANCES: Appearance[] = ["system", "light", "dark"];

/** A preview's colors: the theme's tokens, scoped to that element. */
function vars(p: Palette & { fixed?: Preset["fixed"] }) {
  const t = tokensFor(p as Preset, mode);
  return TOKENS.map((k) => `--${k}:${t[k]}`).join(";");
}

function save() {
  if (!cfg) return;
  applyTheme($state.snapshot(cfg.theme));
  settings.save();
}
function pick(key: string) {
  if (!cfg) return;
  cfg.theme.name = key;
  save();
}
function appearance(a: Appearance) {
  if (!cfg) return;
  cfg.theme.appearance = a;
  save();
}
function remove(name: string) {
  if (!cfg) return;
  cfg.theme.custom = cfg.theme.custom.filter((c) => c.name !== name);
  if (cfg.theme.name === name) cfg.theme.name = DEFAULT_THEME;
  save();
}

// ---- import: paste text with colors in it, adjust the roles, name it
let pasted = $state("");
let draft = $state<Palette | null>(null);
let name = $state("");
const found = $derived(parseColors(pasted));
const nameError = $derived(cfg ? badName(name, cfg.theme.custom) : "");

function read() {
  const s = suggest(found);
  if (s) draft = s;
}
function swap() {
  if (draft) draft = { ...draft, main: draft.accent, accent: draft.main };
}
function add() {
  if (!cfg || !draft || nameError) return;
  const n = name.trim();
  cfg.theme.custom.push({ name: n, ...draft });
  cfg.theme.name = n;
  pasted = "";
  draft = null;
  name = "";
  save();
}
</script>

{#if cfg}
  <Field label="Appearance">
    <div class="inline">
      {#each APPEARANCES as a (a)}
        <button class="mode" class:on={cfg.theme.appearance === a} aria-pressed={cfg.theme.appearance === a} onclick={() => appearance(a)}>{a}</button>
      {/each}
    </div>
  </Field>

  <Field label="Theme">
    <div class="grid">
      {#each themes as p (p.key)}
        <div class="slot" class:on={cfg.theme.name === p.key}>
          <button class="card" aria-label={p.name} style={vars(p)} onclick={() => pick(p.key)}>
            <span class="mini">
              <span class="mbar">{p.name}</span>
              <span class="mbody">
                <span class="mchip">Aa</span>
                <span class="macc"></span>
                <span class="mfeel">{p.feel}</span>
              </span>
            </span>
          </button>
          {#if p.feel === "imported"}
            <button class="link del" aria-label="Delete {p.name}" onclick={() => remove(p.name)}
              >delete</button
            >
          {/if}
        </div>
      {/each}
    </div>
  </Field>

  <Field label="Import">
    <textarea
      rows="2"
      aria-label="Colors to import"
      placeholder="paste hex codes, a coolors.co link or a Lospec list"
      bind:value={pasted}
      oninput={read}
    ></textarea>
    <div class="hint">
      {#if found.length}
        found
        {#each found as c (c)}<i class="sw" style="background:{c}" title={c}></i>{/each}
        {#if found.length < 3}— need three colors{/if}
      {:else}
        three or more colors
      {/if}
    </div>
    {#if draft}
      <div class="draft">
        <div class="roles">
          <label class="inline"
            ><input type="color" bind:value={draft.base}><span class="k">base</span
            ><span class="muted">{draft.base} · text, the dark desk</span></label
          >
          <label class="inline"
            ><input type="color" bind:value={draft.main}><span class="k">main</span
            ><span class="muted">{draft.main} · the light desk, title bars</span></label
          >
          <label class="inline"
            ><input type="color" bind:value={draft.accent}><span class="k">accent</span
            ><span class="muted">{draft.accent} · hover, warnings</span></label
          >
          <div class="inline">
            <button onclick={swap}>Swap main ⇄ accent</button>
          </div>
        </div>
        <div class="slot">
          <div class="card" style={vars(draft)}>
            <span class="mini">
              <span class="mbar">{name.trim() || "preview"}</span>
              <span class="mbody">
                <span class="mchip">Aa</span>
                <span class="macc"></span>
                <span class="mfeel">in {mode} mode</span>
              </span>
            </span>
          </div>
        </div>
      </div>
      <div class="inline save">
        <input type="text" aria-label="Theme name" placeholder="name" bind:value={name}>
        <button class="primary" disabled={!!nameError} onclick={add}>Save theme</button>
      </div>
      {#if name && nameError}<div class="hint">{nameError}</div>{/if}
    {/if}
  </Field>
{/if}

<style>
/* like the light | dark switch: the accent marks the one in use */
.mode.on {
  background: var(--accent);
  color: var(--accent-fg);
}
.grid {
  display: grid;
  grid-template-columns: repeat(auto-fill, minmax(118px, 1fr));
  gap: 10px;
}
.slot {
  display: flex;
  flex-direction: column;
  gap: 2px;
  outline: 2px solid transparent;
  outline-offset: 2px;
}
.slot.on {
  outline-color: var(--accent);
}
.card {
  display: block;
  width: 100%;
  padding: 10px 8px;
  border: 2px solid var(--line);
  /* the preview inside carries the shadow; the card itself stays put */
  box-shadow: none;
  transform: none;
  background: var(--desk) radial-gradient(var(--desk-dot) 0.9px, transparent 1.1px) 0 0 / 6px 6px;
  color: var(--fg);
  text-align: left;
  line-height: var(--lh-tight);
}
.card:hover:not(:disabled),
.card:active:not(:disabled) {
  background-color: var(--desk);
  color: var(--fg);
}
.mini {
  display: block;
  background: var(--panel);
  border: 2px solid var(--line);
  box-shadow: 3px 3px 0 var(--line);
}
.mbar {
  display: block;
  background: var(--bar);
  color: var(--bar-fg);
  padding: 0 4px;
  overflow: hidden;
  text-overflow: ellipsis;
}
.mbody {
  display: block;
  background: var(--paper);
  margin: 2px;
  border: 1px solid var(--line);
  padding: 3px 4px;
}
.mchip {
  background: var(--chip);
  color: var(--chip-fg);
  padding: 0 4px;
}
.macc {
  display: inline-block;
  width: 22px;
  height: 11px;
  vertical-align: -1px;
  background: var(--accent);
}
.mfeel {
  display: block;
  color: var(--muted);
  white-space: nowrap;
  overflow: hidden;
  text-overflow: ellipsis;
}
.del {
  align-self: flex-start;
}
textarea {
  width: 100%;
  background: var(--paper);
  border: 2px solid var(--line);
  padding: 2px 6px;
  outline: none;
  resize: vertical;
  line-height: var(--lh-tight);
  -webkit-user-select: text;
  user-select: text;
}
.sw {
  display: inline-block;
  width: 11px;
  height: 11px;
  margin: 0 2px;
  vertical-align: -1px;
  border: 1px solid var(--line);
}
.draft {
  display: grid;
  grid-template-columns: 1fr 140px;
  gap: 12px;
  margin-top: 8px;
  align-items: start;
}
.roles {
  display: flex;
  flex-direction: column;
  gap: 4px;
}
.k {
  width: 48px;
}
input[type="color"] {
  -webkit-appearance: none;
  appearance: none;
  width: 26px;
  height: 18px;
  padding: 0;
  border: 2px solid var(--line);
  background: none;
  cursor: pointer;
}
input[type="color"]::-webkit-color-swatch-wrapper {
  padding: 0;
}
input[type="color"]::-webkit-color-swatch {
  border: 0;
}
.save {
  margin-top: 8px;
  max-width: 360px;
}
</style>
