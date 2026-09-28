<script lang="ts">
import { onMount } from "svelte";
import { on, type SessionState } from "../lib/api";
import { litBlocks } from "./meter";

type Mode = "rec" | "busy" | "notice" | "error";
const NAMES: Partial<Record<SessionState, string>> = {
  recording: "listening…",
  transcribing: "transcribing…",
  polishing: "polishing…",
  injecting: "typing…",
};
const SPIN = ["▖", "▘", "▝", "▗"];
const N = 10;

let mode = $state<Mode>("rec");
let label = $state("listening…");
let lit = $state(0);
let spin = $state(0);

const chip = $derived(
  mode === "rec"
    ? "REC"
    : mode === "busy"
      ? `${SPIN[spin % 4]} BUSY`
      : mode === "notice"
        ? "NOTE"
        : "ERR",
);

onMount(() => {
  const spinner = setInterval(() => {
    if (mode === "busy") spin++;
  }, 150);
  const offs = [
    on("state", (s) => {
      mode = s === "recording" ? "rec" : "busy";
      label = NAMES[s] ?? s;
    }),
    on("level", (v) => {
      lit = litBlocks(v, lit, N);
    }),
    on("app-error", (e) => {
      mode = "error";
      label = e;
    }),
    // Informational, not a failure.
    on("app-notice", (e) => {
      mode = "notice";
      label = e;
    }),
  ];
  return () => {
    clearInterval(spinner);
    for (const off of offs) off.then((f) => f());
  };
});
</script>

<div id="pill" class={mode}>
  <span class="chip">{chip}</span><span class="label">{label}</span>
  {#if mode === "rec"}
    <span class="bars">
      {#each { length: N }, i (i)}<i class:on={i < lit} class:hot={i >= N - 2}></i>{/each}
    </span>
  {/if}
</div>

<style>
  /* a dark color-scheme would paint an opaque canvas behind the pill */
  :global(html),
  :global(body) {
    background: transparent;
    overflow: hidden;
    color-scheme: normal !important;
  }
  #pill {
    display: flex;
    align-items: center;
    gap: 8px;
    height: 40px;
    margin: 9px 13px 15px 9px;
    padding: 0 8px;
    background: var(--panel);
    border: 2px solid var(--line);
    box-shadow: 4px 4px 0 var(--line);
  }
  .chip {
    flex: none;
    min-width: 44px;
    text-align: center;
    background: var(--accent);
    color: var(--accent-fg);
  }
  .rec .chip::before {
    content: "■ ";
    animation: blink 1s steps(1) infinite;
  }
  .busy .chip {
    background: var(--chip);
    color: var(--chip-fg);
  }
  .notice .chip {
    background: var(--tag);
    color: var(--fg);
  }
  .error .chip {
    background: var(--warn);
    color: var(--accent-fg);
  }
  .label {
    flex: 1;
    min-width: 0;
    line-height: 14px;
    overflow: hidden;
    display: -webkit-box;
    -webkit-line-clamp: 2;
    line-clamp: 2;
    -webkit-box-orient: vertical;
    overflow-wrap: anywhere;
  }
  /* a pixel VU meter: blocks light up left to right with the input level */
  .bars {
    display: flex;
    gap: 2px;
    flex: none;
  }
  .bars i {
    width: 5px;
    height: 12px;
    background: var(--tag);
    display: block;
  }
  .bars i.on {
    background: var(--fg);
  }
  .bars i.on.hot {
    background: var(--accent);
  }
</style>
