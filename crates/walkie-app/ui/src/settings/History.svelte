<script lang="ts">
import { onMount } from "svelte";
import { api, type HistoryRecord } from "@/lib/api";
import { byDay, clock } from "@/lib/format";
import { settings } from "./state.svelte";

const cfg = $derived(settings.cfg);
let q = $state("");
let rows = $state<HistoryRecord[] | null>(null);
let copied = $state<number | null>(null);

async function load() {
  const query = q.trim();
  try {
    rows = query ? await api.historySearch(query, 100) : await api.historyRecent(50);
  } catch (e) {
    settings.fail(e);
  }
}

let searching: ReturnType<typeof setTimeout> | undefined;
function typed() {
  clearTimeout(searching);
  searching = setTimeout(load, 200);
}
function enter(e: KeyboardEvent) {
  if (e.key !== "Enter") return;
  clearTimeout(searching);
  load();
}

async function copy(r: HistoryRecord) {
  try {
    await api.copyText(r.polished || r.cleaned);
    copied = r.id;
    setTimeout(() => {
      if (copied === r.id) copied = null;
    }, 1200);
  } catch (e) {
    settings.fail(e);
  }
}

async function remove(r: HistoryRecord) {
  try {
    await api.historyDelete(r.id);
  } catch (e) {
    settings.fail(e);
  }
  load();
}

// two clicks, so a stray one can't wipe everything
let armed = $state(false);
let disarm: ReturnType<typeof setTimeout> | undefined;
async function clearAll() {
  clearTimeout(disarm);
  if (!armed) {
    armed = true;
    disarm = setTimeout(() => (armed = false), 3000);
    return;
  }
  armed = false;
  try {
    await api.historyClear();
  } catch (e) {
    settings.fail(e);
  }
  load();
}

onMount(load);
</script>

{#if cfg}
  <label class="cb"
    ><input type="checkbox" bind:checked={cfg.history.enabled} onchange={() => settings.save()}>
    {"Keep local history"}</label
  ><span class="rs" title="applies after a restart">*</span>
{/if}
<div class="tools">
  <input type="search" placeholder="search…" bind:value={q} oninput={typed} onkeydown={enter}>
  <span class="grow"></span>
  <button class:primary={armed} disabled={!rows?.length && !q} onclick={clearAll}>
    {armed ? "Really clear?" : "Clear all"}
  </button>
</div>
{#if rows && !rows.length}
  <div class="empty">
    <span class="big">{q.trim() ? "No matches." : "Nothing yet."}</span>
    <span class="caps">{q.trim() ? "try other words" : "your dictations will show up here"}</span>
  </div>
{:else if rows}
  {#each byDay(rows) as g (g.day)}
    <div class="day">{g.day}</div>
    {#each g.rows as r (r.id)}
    <div class="entry">
      <div class="meta">
        <span class="time">{clock(r.created_at) || r.created_at}</span>
        {#if r.lang}
          <span class="tag">{r.lang}</span>
        {/if}
        {#if r.polished}
          <span class="tag">polished</span>
        {/if}
        <span class="muted">{(r.duration_ms / 1000).toFixed(1)}s</span>
        <span class="grow"></span>
        <span class="acts" class:shown={copied === r.id}>
          <button onclick={() => copy(r)}>{copied === r.id ? "Copied ✓" : "Copy"}</button>
          <button onclick={() => remove(r)}>Delete</button>
        </span>
      </div>
      <p>{r.polished || r.cleaned}</p>
    </div>
    {/each}
  {/each}
{/if}

<style>
.rs {
  opacity: 0.6;
  margin-left: 4px;
}
.tools {
  display: flex;
  gap: 8px;
  align-items: center;
  margin: 12px 0 4px;
}
.tools input {
  max-width: 260px;
}
.grow {
  flex: 1;
}
/* a day heading: the date in serif, over an ink rule */
.day {
  margin-top: 22px;
  padding-bottom: 4px;
  border-bottom: 1px solid var(--ink);
  font: 22px / 26px var(--serif);
}
.entry {
  border-bottom: 1px solid var(--rule);
  padding: 10px 0;
}
.time {
  font-weight: 600;
  font-variant-numeric: tabular-nums;
}
.entry:last-child {
  border-bottom: 0;
}
/* the row's buttons stay out of the way until it's pointed at or tabbed
   into (opacity, not visibility, so they stay reachable) */
.acts {
  display: flex;
  gap: 6px;
  opacity: 0;
}
.entry:hover .acts,
.entry:focus-within .acts,
.acts.shown {
  opacity: 1;
}
.meta {
  display: flex;
  gap: 8px;
  align-items: center;
  font-size: 12px;
}
.meta .tag {
  margin: 0;
}
p {
  margin: 4px 0 0;
  line-height: 19px;
  -webkit-user-select: text;
  user-select: text;
  overflow-wrap: anywhere;
}
.empty {
  height: 160px;
  display: grid;
  place-content: center;
  justify-items: center;
  gap: 4px;
  border: 1px dashed var(--rule);
  margin-top: 12px;
}
.big {
  font: italic 26px / 30px var(--serif);
}
</style>
