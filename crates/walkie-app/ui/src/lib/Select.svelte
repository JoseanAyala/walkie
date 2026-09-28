<script lang="ts">
// A dropdown drawn in walkie's own style instead of macOS's native popup
// menu. Keyboard works as in a native one (lib/listbox.ts): the open list
// takes focus and tracks the highlight with aria-activedescendant. To
// macOS the button is an AXPopUpButton named by the chosen option (no
// aria-label, which would hide it), which the OS e2e tests read.
import { move, type Option, typeahead } from "./listbox";

let {
  value = $bindable(),
  options,
  onchange,
  label,
}: {
  value: string;
  options: Option[];
  onchange?: () => void;
  /** names the open list */
  label?: string;
} = $props();

const id = $props.id();
let btn = $state<HTMLButtonElement>();
let list = $state<HTMLUListElement>();
let open = $state(false);
let active = $state(0);
// where the list sits: under the button, or above it near the window's foot
let at = $state("");

const chosen = $derived(options.findIndex((o) => o.value === value));

const LIST_MAX = 220;

function show() {
  if (!btn || !options.length) return;
  const r = btn.getBoundingClientRect();
  const below = innerHeight - r.bottom;
  const up = below < Math.min(options.length * 22 + 8, LIST_MAX) && r.top > below;
  at = `left:${r.left}px;min-width:${r.width}px;${
    up ? `bottom:${innerHeight - r.top + 2}px` : `top:${r.bottom + 2}px`
  }`;
  active = Math.max(chosen, 0);
  open = true;
}

function choose(i: number) {
  const o = options[i];
  open = false;
  btn?.focus();
  if (!o || o.value === value) return;
  value = o.value;
  onchange?.();
}

/** On the closed button: the keys that open it. */
function opener(e: KeyboardEvent) {
  if (["ArrowDown", "ArrowUp", "Enter", " "].includes(e.key)) {
    e.preventDefault();
    show();
  }
}

/** In the open list. */
function key(e: KeyboardEvent) {
  const to = move(active, e.key, options.length);
  if (to !== null) {
    e.preventDefault();
    active = to;
  } else if (e.key === "Enter" || e.key === " ") {
    e.preventDefault();
    choose(active);
  } else if (e.key === "Escape") {
    e.preventDefault();
    open = false;
    btn?.focus();
  } else if (e.key === "Tab") {
    // back on the button, so Tab carries on from there
    open = false;
    btn?.focus();
  } else {
    const j = typeahead(options, active, e.key);
    if (j >= 0) active = j;
  }
}

$effect(() => {
  if (open) list?.focus({ preventScroll: true });
});

// keep the highlighted option in view
$effect(() => {
  if (open) document.getElementById(`${id}-${active}`)?.scrollIntoView({ block: "nearest" });
});

// a click elsewhere, a scroll or a resize closes it, as a native menu does
$effect(() => {
  if (!open) return;
  const away = (e: Event) => {
    const t = e.target as Node;
    if (!btn?.contains(t) && !document.getElementById(`${id}-list`)?.contains(t)) open = false;
  };
  // scrolling the list itself is fine
  const scrolled = (e: Event) => {
    if (!document.getElementById(`${id}-list`)?.contains(e.target as Node)) open = false;
  };
  const close = () => (open = false);
  addEventListener("mousedown", away, true);
  addEventListener("scroll", scrolled, true);
  addEventListener("resize", close);
  addEventListener("blur", close);
  return () => {
    removeEventListener("mousedown", away, true);
    removeEventListener("scroll", scrolled, true);
    removeEventListener("resize", close);
    removeEventListener("blur", close);
  };
});
</script>

<button
  bind:this={btn}
  class="select"
  aria-haspopup="listbox"
  aria-expanded={open}
  aria-controls="{id}-list"
  onclick={() => (open ? (open = false) : show())}
  onkeydown={opener}
>{options[chosen]?.text ?? value}</button>

{#if open}
  <ul
    bind:this={list}
    id="{id}-list"
    class="list"
    role="listbox"
    tabindex="-1"
    aria-label={label}
    aria-activedescendant="{id}-{active}"
    style={at}
    onkeydown={key}
  >
    {#each options as o, i (o.value)}
      <!-- the list has focus and handles the keys (aria-activedescendant) -->
      <!-- svelte-ignore a11y_click_events_have_key_events -->
      <li
        id="{id}-{i}"
        role="option"
        aria-selected={i === chosen}
        class:active={i === active}
        onmousemove={() => (active = i)}
        onclick={() => choose(i)}
      >
        <span class="mark">{i === chosen ? "■" : ""}</span>{o.text}
      </li>
    {/each}
  </ul>
{/if}

<style>
/* looks like a text field (theme.css's select), not a button */
.select {
  display: block;
  width: 100%;
  height: 26px;
  padding: 0 22px 0 6px;
  background-color: var(--paper);
  box-shadow: none;
  transform: none;
  text-align: left;
  overflow: hidden;
  text-overflow: ellipsis;
  line-height: 22px;
  background-image:
    linear-gradient(45deg, transparent 50%, var(--fg) 50%),
    linear-gradient(-45deg, transparent 50%, var(--fg) 50%);
  background-position:
    calc(100% - 12px) 10px,
    calc(100% - 8px) 10px;
  background-size: 4px 4px;
  background-repeat: no-repeat;
}
.select:hover,
.select:active,
.select[aria-expanded="true"] {
  background-color: var(--paper);
  color: var(--fg);
  transform: none;
}
.select:focus-visible {
  outline-offset: -2px;
}
.list {
  position: fixed;
  z-index: 10;
  margin: 0;
  padding: 2px;
  list-style: none;
  max-height: 220px;
  overflow: auto;
  max-width: calc(100vw - 24px);
  background: var(--paper);
  border: 2px solid var(--line);
  box-shadow: 4px 4px 0 var(--line);
  animation: open 100ms steps(2) both;
  outline: none;
}
li {
  display: flex;
  padding: 0 8px 0 4px;
  white-space: nowrap;
  cursor: pointer;
}
li.active {
  background: var(--chip);
  color: var(--chip-fg);
}
.mark {
  width: 14px;
  flex: none;
}
</style>
