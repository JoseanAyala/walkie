<script lang="ts">
import { onMount } from "svelte";
import { on, type SessionState } from "@/lib/api";
import Icon from "@/lib/Icon.svelte";
import { FRAME_MS, FRAMES, scramble } from "./decode";
import { COLUMNS, deaf, heard, loudness, push, REACH, reach, TICK_MS } from "./meter";

type Mode = "rec" | "busy" | "done" | "notice" | "error";
const NAMES: Partial<Record<SessionState, string>> = {
  recording: "listening…",
  transcribing: "transcribing…",
  polishing: "polishing…",
  injecting: "typing…",
};
const DEAF = "can't hear you — check the mic";
const SPIN = ["▖", "▘", "▝", "▗"];
const FLAT = () => Array<number>(COLUMNS).fill(0);

let mode = $state<Mode>("rec");
let label = $state("listening…");
let spin = $state(0);
// bumped each time a dictation starts, so the pill replays its entrance
let shown = $state(0);
let last: SessionState | "" = "";

// the wave: one column per tick, the loudest level heard during it
let wave = $state(FLAT());
let peak = 0;
let startedAt = 0;
let heardAny = false;

const chip = $derived(
  mode === "rec"
    ? "REC"
    : mode === "busy"
      ? `${SPIN[spin % 4]} BUSY`
      : mode === "done"
        ? "✓ DONE"
        : mode === "notice"
          ? "NOTE"
          : "ERR",
);
// the wave's slot stays put from REC to DONE, so nothing shifts; a notice
// or error gets the room for its text
const metered = $derived(mode === "rec" || mode === "busy" || mode === "done");

// what the label shows: a new status decodes in from glyph noise. Plain
// "listening…" arrives with the pill's own tear; errors and notices split
// instead (CSS), and stay readable from the first frame. Runs under Reduce
// Motion too, like the rest of the glitch (theme.css).
let shownLabel = $state("listening…");
$effect(() => {
  const text = label;
  if (mode === "error" || mode === "notice" || text === NAMES.recording) {
    shownLabel = text;
    return;
  }
  let frame = 0;
  shownLabel = scramble(text, frame);
  const decoding = setInterval(() => {
    shownLabel = scramble(text, ++frame);
    if (frame >= FRAMES) clearInterval(decoding);
  }, FRAME_MS);
  return () => clearInterval(decoding);
});

function tick() {
  if (mode !== "rec") return;
  wave = push(wave, peak);
  peak = 0;
  if (deaf(startedAt, heardAny, Date.now())) label = DEAF;
}

onMount(() => {
  const spinner = setInterval(() => {
    if (mode === "busy") spin++;
  }, 150);
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
    clearInterval(spinner);
    clearInterval(ticker);
    for (const off of offs) off.then((f) => f());
  };
});
</script>

{#key shown}
<div id="pill" class="glitch {mode}" class:metered>
  <span class="chip" data-text={chip}
    >{#if mode === "rec"}<span class="live"><Icon name="mic" /></span>{/if}{chip}</span
  ><span class="label" class:warn={label === DEAF} data-text={label}>{shownLabel}</span>
  {#if metered}
    <!-- one icon pixel is 2 CSS pixels; columns 1 wide with a 1 gap, the
         newest (rightmost) in the accent color -->
    <svg
      class="wave"
      width={COLUMNS * 4}
      height={(2 * REACH + 1) * 2}
      viewBox="0 0 {COLUMNS * 2} {2 * REACH + 1}"
      shape-rendering="crispEdges"
      aria-hidden="true"
    >
      {#each wave as v, i (i)}
        {@const r = reach(v)}
        <rect
          x={i * 2}
          y={REACH - r}
          width="1"
          height={2 * r + 1}
          class:now={i === wave.length - 1 && mode === "rec"}
        />
      {/each}
    </svg>
  {/if}
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
    align-items: center;
    gap: 8px;
    height: 40px;
    margin: 9px 13px 15px 9px;
    padding: 0 8px;
    background: var(--panel);
    border: 2px solid var(--line);
    box-shadow: 4px 4px 0 var(--line);
    /* tears in: horizontal slices jump sideways on the pixel grid, then settle */
    animation: tear 160ms steps(1) both;
  }
  @keyframes tear {
    0% {
      clip-path: inset(0 0 55% 0);
      transform: translateX(-4px);
    }
    25% {
      clip-path: inset(45% 0 0 0);
      transform: translateX(4px);
    }
    50% {
      clip-path: inset(20% 0 35% 0);
      transform: translateX(-2px);
    }
    75% {
      clip-path: none;
      transform: translateX(2px);
    }
    100% {
      clip-path: none;
      transform: none;
    }
  }
  /* an error jolts the pill and splits the chip and label into two sliced
     copies (pink and ink) that snap back; once, as it arrives */
  #pill.error {
    animation: jolt 180ms steps(1) both;
  }
  @keyframes jolt {
    0% {
      transform: translateX(2px);
    }
    33% {
      transform: translateX(-2px);
    }
    66% {
      transform: translateX(2px);
    }
    100% {
      transform: none;
    }
  }
  .error .chip,
  .error .label {
    position: relative;
  }
  .error .chip::before,
  .error .chip::after,
  .error .label::before,
  .error .label::after {
    content: attr(data-text);
    position: absolute;
    inset: 0;
    background: inherit;
    pointer-events: none;
  }
  .error .chip::before,
  .error .label::before {
    animation: chan-a 220ms steps(1) both;
  }
  .error .chip::after,
  .error .label::after {
    animation: chan-b 220ms steps(1) both;
  }
  .error .label::before {
    color: var(--accent);
  }
  .error .label::after {
    color: var(--fg);
  }
  .error .chip::after {
    background: var(--line);
  }
  @keyframes chan-a {
    0% {
      clip-path: inset(0 0 60% 0);
      transform: translateX(-2px);
    }
    33% {
      clip-path: inset(50% 0 10% 0);
      transform: translateX(4px);
    }
    66% {
      clip-path: inset(20% 0 50% 0);
      transform: translateX(-4px);
    }
    100% {
      clip-path: inset(0 0 100% 0);
      transform: none;
    }
  }
  @keyframes chan-b {
    0% {
      clip-path: inset(60% 0 0 0);
      transform: translateX(2px);
    }
    33% {
      clip-path: inset(10% 0 70% 0);
      transform: translateX(-2px);
    }
    66% {
      clip-path: inset(70% 0 0 0);
      transform: translateX(2px);
    }
    100% {
      clip-path: inset(100% 0 0 0);
      transform: none;
    }
  }
  /* one width for every state, so the label never moves */
  .chip {
    flex: none;
    width: 56px;
    text-align: center;
    white-space: nowrap;
    background: var(--accent);
    color: var(--accent-fg);
  }
  .live {
    margin-right: 4px;
    animation: blink 1s steps(1) infinite;
  }
  .busy .chip {
    background: var(--chip);
    color: var(--chip-fg);
  }
  .done .chip {
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
    line-height: var(--lh-tight);
    overflow: hidden;
    display: -webkit-box;
    -webkit-line-clamp: 2;
    line-clamp: 2;
    -webkit-box-orient: vertical;
    overflow-wrap: anywhere;
  }
  .label.warn {
    color: var(--warn);
  }
  /* the wave, frozen and dimmed once it stops listening */
  .wave {
    flex: none;
    fill: var(--fg);
  }
  .wave rect.now {
    fill: var(--accent);
  }
  :not(.rec) > .wave {
    opacity: 0.35;
  }
</style>
