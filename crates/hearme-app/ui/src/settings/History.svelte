<script lang="ts">
import { onMount } from "svelte";
import { api, type HistoryRecord } from "../lib/api";
import { when } from "../lib/format";
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
  <div class="empty dots">
    <span class="chip">{q.trim() ? "no matches" : "no dictations yet"}</span>
  </div>
{:else if rows}
  {#each rows as r (r.id)}
    <div class="entry">
      <div class="meta">
        <span class="chip">{when(r.created_at)}</span>
        {#if r.lang}
          <span class="tag">{r.lang}</span>
        {/if}
        {#if r.polished}
          <span class="tag">polished</span>
        {/if}
        <span class="muted">{(r.duration_ms / 1000).toFixed(1)}s</span>
        <span class="grow"></span>
        <button onclick={() => copy(r)}>{copied === r.id ? "Copied ✓" : "Copy"}</button>
        <button onclick={() => remove(r)}>Delete</button>
      </div>
      <p>{r.polished || r.cleaned}</p>
    </div>
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
  margin: 8px 0;
}
.tools input {
  max-width: 260px;
}
.grow {
  flex: 1;
}
.entry {
  border-bottom: 1px solid var(--line);
  padding: 8px 0;
}
.meta {
  display: flex;
  gap: 6px;
  align-items: center;
}
.meta .tag {
  margin: 0;
}
p {
  margin: 4px 0 0;
  line-height: 16px;
  -webkit-user-select: text;
  user-select: text;
  overflow-wrap: anywhere;
}
.empty {
  height: 120px;
  display: grid;
  place-items: center;
  border: 1px solid var(--line);
  margin-top: 8px;
}
</style>
