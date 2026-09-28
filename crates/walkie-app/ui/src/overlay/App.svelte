<script lang="ts">
import { onMount } from "svelte";
import { on, type SessionState } from "@/lib/api";
import { COLUMNS, deaf, heard, loudness, push, TICK_MS } from "./meter";

type Mode = "rec" | "busy" | "done" | "notice" | "error";
const NAMES: Partial<Record<SessionState, string>> = {
  recording: "listening…",
  transcribing: "transcribing…",
  polishing: "polishing…",
  injecting: "typing…",
};
const DEAF = "can't hear you — check the mic";
const FLAT = () => Array<number>(COLUMNS).fill(0);

let mode = $state<Mode>("rec");
let label = $state("listening…");
// bumped each time a dictation starts, so the pill replays its entrance
let shown = $state(0);
let last: SessionState | "" = "";

// the wave: one column per tick, the loudest level heard during it
let wave = $state(FLAT());
let peak = 0;
let startedAt = 0;
let heardAny = false;

const CHIPS: Record<Mode, string> = {
  rec: "REC",
  busy: "WAIT",
  done: "DONE",
  notice: "NOTE",
  error: "ERR",
};
// the wave's slot stays put from REC to DONE, so nothing shifts; a notice
// or error gets the room for its text
const metered = $derived(mode === "rec" || mode === "busy" || mode === "done");

// the wave, mirrored about its middle: the newest level in the centre,
// older ones spreading out to both sides
const newest = COLUMNS / 2 - 1;
const bars = $derived(
  Array.from(
    { length: 2 * newest + 1 },
    (_, i) => wave[wave.length - 1 - Math.abs(i - newest)] ?? 0,
  ),
);

function tick() {
  if (mode !== "rec") return;
  wave = push(wave, peak);
  peak = 0;
  if (deaf(startedAt, heardAny, Date.now())) label = DEAF;
}

onMount(() => {
  const ticker = setInterval(tick, TICK_MS);
  const offs = [
    on("state", (s) => {
      if ((s === "recording" || s === "polishing") && (last === "" || last === "idle")) {
        shown++;
        wave = FLAT();
        startedAt = Date.now();
      }
      last = s;
      // idle comes as the pill hides; keep whatever it last said. What was
      // heard is cleared now, not when the next recording starts: the mic
      // starts (and reports levels) before the session says "recording".
      if (s === "idle") {
        peak = 0;
        heardAny = false;
        return;
      }
      mode = s === "recording" ? "rec" : "busy";
      label = NAMES[s] ?? s;
    }),
    // typed: a beat of confirmation before the pill goes (glue's
    // DONE_GRACE). An error or notice already showing stays.
    on("transcribed", () => {
      if (mode !== "busy") return;
      mode = "done";
      label = "typed";
    }),
    on("level", (v) => {
      peak = Math.max(peak, loudness(v));
      if (heard(v) && !heardAny) {
        heardAny = true;
        if (label === DEAF) label = NAMES.recording ?? "";
      }
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
    clearInterval(ticker);
    for (const off of offs) off.then((f) => f());
  };
});
</script>

{#key shown}
<div id="pill" class="moves {mode}">
  <div class="trim"></div>
  <div class="row">
    <span class="state">
      {#if mode === "rec"}
        <span class="dot"></span>
      {:else if mode === "busy"}
        <!-- sand runs down, then the glass turns over; it's symmetric, so the
             turn ends where the loop starts -->
        <svg class="glass" viewBox="0 0 10 14" aria-hidden="true">
          <g class="turn">
            <path class="frame" d="M1.5 1h7M1.5 13h7M2.5 1c0 3.5 2.5 4.5 2.5 6s-2.5 2.5-2.5 6M7.5 1c0 3.5-2.5 4.5-2.5 6s2.5 2.5 2.5 6" />
            <path class="sand top" d="M3.2 3h3.6L5 6.2z" />
            <path class="sand bottom" d="M3 12.5h4L5 9.8z" />
            <path class="stream" d="M5 7v5" />
          </g>
        </svg>
      {:else if mode === "done"}
        <svg class="check" viewBox="0 0 10 10" aria-hidden="true">
          <path d="M1.5 5.5 4 8l4.5-6" />
        </svg>
      {/if}
      {CHIPS[mode]}
    </span>
    <span class="label" class:warn={label === DEAF}>{label}</span>
    {#if metered}
      <span class="wave" aria-hidden="true">
        {#each bars as v, i (i)}
          <i class:mid={i === newest && mode === "rec"} style="height:{2 + v * 26}px"></i>
        {/each}
      </span>
    {/if}
  </div>
</div>
{/key}

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
    flex-direction: column;
    width: 400px;
    margin: 8px 10px 14px;
    background: var(--sheet);
    border: 1px solid var(--ink);
    box-shadow: 0 4px 12px rgb(0 0 0 / 0.18);
    animation: enter 180ms cubic-bezier(0.2, 0.8, 0.2, 1) both;
  }
  .row {
    display: flex;
    height: 44px;
  }
  /* one width for every state, so the label never moves */
  .state {
    flex: none;
    width: 72px;
    display: flex;
    align-items: center;
    justify-content: center;
    gap: 6px;
    font: 600 11px var(--sans);
    letter-spacing: 0.14em;
    background: var(--red);
    color: var(--on-red);
    border-right: 1px solid var(--ink);
  }
  .busy .state {
    background: var(--lav);
    color: var(--ink);
  }
  .done .state,
  .notice .state {
    background: var(--ink);
    color: var(--bg);
  }
  .error .state {
    background: var(--ink);
    color: var(--red);
  }
  .dot {
    width: 7px;
    height: 7px;
    border-radius: 50%;
    background: currentColor;
    animation: pulse 1.2s ease-in-out infinite;
  }
  .glass {
    width: 9px;
    height: 13px;
    overflow: visible;
  }
  .glass .frame,
  .glass .stream,
  .check path {
    fill: none;
    stroke: currentColor;
    stroke-width: 1.2;
    stroke-linecap: round;
    stroke-linejoin: round;
  }
  .glass .sand {
    fill: currentColor;
    transform-box: fill-box;
  }
  .glass .turn {
    transform-origin: 5px 7px;
    animation: turn 1.8s cubic-bezier(0.6, 0, 0.3, 1) infinite;
  }
  /* the top empties toward its neck, the bottom heaps up from the floor */
  .glass .top {
    transform-origin: 50% 100%;
    animation: drain 1.8s linear infinite;
  }
  .glass .bottom {
    transform-origin: 50% 100%;
    animation: heap 1.8s linear infinite;
  }
  .glass .stream {
    stroke-width: 0.8;
    stroke-dasharray: 1 1;
    animation: fall 1.8s linear infinite;
  }
  .check {
    width: 10px;
    height: 10px;
  }
  .check path {
    stroke-width: 1.6;
    stroke-dasharray: 12;
    animation: draw 320ms 60ms ease-out both;
  }
  @keyframes turn {
    0%,
    75% {
      transform: rotate(0);
    }
    100% {
      transform: rotate(180deg);
    }
  }
  @keyframes drain {
    0% {
      transform: scale(1);
    }
    72%,
    100% {
      transform: scale(0);
    }
  }
  @keyframes heap {
    0% {
      transform: scale(0);
    }
    72%,
    100% {
      transform: scale(1);
    }
  }
  @keyframes fall {
    0% {
      stroke-dashoffset: 0;
      opacity: 1;
    }
    70% {
      stroke-dashoffset: -6;
      opacity: 1;
    }
    72%,
    100% {
      stroke-dashoffset: -6;
      opacity: 0;
    }
  }
  @keyframes draw {
    from {
      stroke-dashoffset: 12;
    }
    to {
      stroke-dashoffset: 0;
    }
  }
  .label {
    flex: 1;
    min-width: 0;
    align-self: center;
    padding: 0 14px;
    font: italic 19px / 20px var(--serif);
    overflow: hidden;
    display: -webkit-box;
    -webkit-line-clamp: 2;
    line-clamp: 2;
    -webkit-box-orient: vertical;
    overflow-wrap: anywhere;
  }
  .label.warn {
    color: var(--red);
  }
  .wave {
    flex: none;
    width: 112px;
    display: flex;
    align-items: center;
    justify-content: center;
    gap: 2px;
    border-left: 1px solid var(--rule);
  }
  .wave i {
    display: block;
    width: 2px;
    background: var(--ink);
  }
  .wave i.mid {
    background: var(--red);
  }
  /* frozen and dimmed once it stops listening */
  :not(.rec) > .row > .wave {
    opacity: 0.3;
  }
  /* an error gives a small shake as it arrives */
  #pill.error {
    animation: nudge 260ms ease both;
  }
  @keyframes nudge {
    25% {
      transform: translateX(-3px);
    }
    50% {
      transform: translateX(3px);
    }
    75% {
      transform: translateX(-1px);
    }
  }
</style>
