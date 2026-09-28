<script lang="ts">
import { onMount } from "svelte";
import { api, type HotkeyName, message, on } from "@/lib/api";
import { chord, sameKeys } from "@/lib/format";
import { settings } from "./state.svelte";

const ROWS: { k: HotkeyName; name: string; label: string; help: string }[] = [
  { k: "dictate", name: "Dictate", label: "Dictate", help: "hold to talk · double-tap to lock" },
  {
    k: "polish",
    name: "Dictate + polish",
    label: "Dictate + polish",
    help: "hold; the text is polished before it's typed (Polish tab)",
  },
  {
    k: "hands_free",
    name: "Hands-free",
    label: "Hands-free",
    help: "press to start, again (or Dictate) to stop",
  },
  {
    k: "paste_last",
    name: "Paste last",
    label: "Paste last transcript",
    help: "insert your last dictation again",
  },
];
const label = (k: HotkeyName) => ROWS.find((r) => r.k === k)?.label ?? k;

let recording = $state<HotkeyName | null>(null);
let msg = $state(
  "Holding Dictate and adding the extra key of another shortcut switches the running recording (e.g. Fn, then Space → hands-free).",
);
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
      <span class="what">{r.name}<small>{r.help}</small></span>
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
<div class="hint" class:err>{msg}</div>

<style>
.sc {
  display: grid;
  grid-template-columns: 1fr 130px auto auto;
  gap: 6px;
  align-items: center;
  padding: 4px 0;
}
.what small {
  display: block;
  color: var(--muted);
  line-height: var(--lh-tight);
}
kbd {
  background: var(--paper);
  border: 2px solid var(--line);
  text-align: center;
  padding: 0 4px;
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
}
kbd.rec {
  background: var(--accent);
  color: var(--accent-fg);
  animation: blink 1s steps(1) infinite;
}
.err {
  color: var(--warn);
}
</style>
