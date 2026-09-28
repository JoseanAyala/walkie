<script lang="ts">
import { onMount } from "svelte";
import { api, type HotkeyName, message, on } from "@/lib/api";
import { chord, sameKeys } from "@/lib/format";
import { settings } from "./state.svelte";

const ROWS: { k: HotkeyName; name: string; label: string; help: string }[] = [
  {
    k: "dictate",
    name: "Dictate",
    label: "Dictate",
    help: "hold, speak, release: it's typed where you are · double-tap to keep it on",
  },
  {
    k: "polish",
    name: "Polish",
    label: "Polish",
    help: "tap: rewrites the selected text, or the whole field (see Polish)",
  },
  {
    k: "paste_last",
    name: "Paste last",
    label: "Paste last transcript",
    help: "insert your last dictation again",
  },
];
const label = (k: HotkeyName) => ROWS.find((r) => r.k === k)?.label ?? k;
// macOS gives 🌐 a job of its own unless told not to; the overlay also says
// so if it gets in the way
const GLOBE =
  'fn is also macOS\'s 🌐 key: set Keyboard → "Press 🌐 key to" → Do Nothing, or it opens emoji or switches input too. Click to open Keyboard settings.';
const usesFn = (keys: string[]) => keys.some((k) => k.toLowerCase() === "fn");

let recording = $state<HotkeyName | null>(null);
// only while recording, and to confirm or refuse what was recorded
let msg = $state("");
let err = $state(false);
const say = (text: string, isErr = false) => {
  msg = text;
  err = isErr;
};

async function record(k: HotkeyName) {
  try {
    await api.recordShortcut();
    recording = k;
    say("Press and release the keys you want, together. Esc cancels.");
  } catch (e) {
    say(message(e), true);
  }
}

function clear(k: HotkeyName) {
  // Dictate is the one way to dictate; its ✕ is disabled
  if (k === "dictate" || !settings.cfg) return;
  settings.cfg.hotkeys[k] = [];
  settings.save();
}

onMount(() => {
  const offs = [
    on("shortcut-recorded", (keys) => {
      const k = recording;
      recording = null;
      const cfg = settings.cfg;
      if (!k || !cfg) return;
      const clash = ROWS.find(
        (r) => r.k !== k && cfg.hotkeys[r.k].length && sameKeys(cfg.hotkeys[r.k], keys),
      );
      if (clash) {
        say(`${chord(keys)} is already ${clash.label}.`, true);
      } else {
        cfg.hotkeys[k] = keys;
        say(`${label(k)} is now ${chord(keys)}.`);
        settings.save();
      }
    }),
    on("shortcut-error", (e) => {
      recording = null;
      say(e, true);
    }),
    on("shortcut-cancelled", () => {
      recording = null;
      say("Recording cancelled.");
    }),
  ];
  return () => {
    for (const off of offs) off.then((f) => f());
  };
});
</script>

{#if settings.cfg}
  {#each ROWS as r (r.k)}
    <div class="sc">
      <span class="what"
        ><span>{r.name}{#if usesFn(settings.cfg.hotkeys[r.k])}
          <button
            class="info"
            title={GLOBE}
            aria-label="About the fn key"
            onclick={() => api.openSettingsPane("keyboard")}
          >
            ⓘ
          </button>
        {/if}</span
        ><small>{r.help}</small></span
      >
      <kbd class:rec={recording === r.k} title={recording === r.k ? "esc cancels" : ""}
        >{recording === r.k ? "press keys…" : chord(settings.cfg.hotkeys[r.k])}</kbd
      >
      <button onclick={() => record(r.k)}>Record</button>
      {#if r.k === "dictate"}
        <button disabled title="Dictate can't be turned off — record a different key instead">
          ✕
        </button>
      {:else}
        <button title="Turn off" onclick={() => clear(r.k)}>✕</button>
      {/if}
    </div>
  {/each}
{/if}
{#if msg}<div class="hint" class:err>{msg}</div>{/if}

<style>
.sc {
  display: grid;
  grid-template-columns: 1fr 110px auto auto;
  gap: 2px 8px;
  align-items: center;
  padding: 4px 0 8px;
  line-height: 20px;
}
/* the name shares the keys' line; its help runs underneath, full width */
.what {
  display: contents;
}
.what small {
  grid-row: 2;
  grid-column: 1 / -1;
  font-size: 12px;
  line-height: 16px;
  color: var(--muted);
}
.info {
  border: 0;
  background: none;
  padding: 0 4px;
  color: var(--muted);
  font-size: 12px;
}
kbd {
  text-align: center;
  overflow: hidden;
  text-overflow: ellipsis;
}
kbd.rec {
  background: var(--red);
  color: var(--on-red);
  border-color: var(--red);
  animation: pulse 1s ease-in-out infinite;
}
.err {
  color: var(--red);
}
</style>
