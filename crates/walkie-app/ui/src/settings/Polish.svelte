<script lang="ts">
import { type AppleAi, api, message, type Tone } from "@/lib/api";
import Select from "@/lib/Select.svelte";
import Field from "./Field.svelte";
import { settings } from "./state.svelte";

// The prompts behind the tones are built into walkie (pipeline/style.rs).
const TONES: { value: Tone; name: string; example: string }[] = [
  {
    value: "formal",
    name: "Formal · caps + punctuation",
    example: "Hey, are you free for lunch tomorrow? Let's do 12 if that works for you.",
  },
  {
    value: "casual",
    name: "Casual · caps + less punctuation",
    example: "Hey are you free for lunch tomorrow? Let's do 12 if that works for you",
  },
  {
    value: "very_casual",
    name: "Very casual · no caps + less punctuation",
    example: "hey are you free for lunch tomorrow? let's do 12 if that works for you",
  },
  {
    value: "excited",
    name: "Excited · more exclamations",
    example: "Hey, are you free for lunch tomorrow? Let's do 12 if that works for you!",
  },
];
const example = $derived(TONES.find((t) => t.value === cfg?.polish.tone)?.example ?? "");

const cfg = $derived(settings.cfg);
const apple = $derived(cfg?.polish.provider === "apple");
const noCommand = $derived(!!cfg?.hotkeys.polish.length && !apple && !cfg.polish.command.trim());

let ai = $state<AppleAi | null>(null);
$effect(() => {
  if (apple && !ai) api.appleAiStatus().then((s) => (ai = s));
});

let running = $state(false);
let out = $state<{ ok: boolean; text: string } | null>(null);

const PROVIDERS = [
  { value: "command", text: "Your command" },
  { value: "apple", text: "Apple, on this Mac (macOS 26)" },
];
// a different model or tone: the last test's output no longer applies
function changed() {
  out = null;
  settings.save();
}

async function test() {
  if (!cfg) return;
  running = true;
  out = null;
  try {
    const r = await api.testPolish(cfg.polish);
    out = { ok: true, text: `→ ${r.trim()}` };
  } catch (e) {
    out = { ok: false, text: message(e) };
  }
  running = false;
}
</script>

{#if cfg}
  <Field label="Model">
    <Select
      label="Polish model"
      bind:value={cfg.polish.provider}
      options={PROVIDERS}
      onchange={changed}
    />
    {#if apple}
      <div class="hint" data-testid="apple-ai">
        {#if !ai}
          checking…
        {:else if ai.status === "available"}
          Apple Intelligence: {ai.detail}. Nothing leaves this Mac.
        {:else}
          <span class="chip warn">!!</span>
          {ai.detail}
          {#if ai.status === "off"}
            <button onclick={() => api.openSettingsPane("ai")}>Open settings</button>
          {/if}
        {/if}
      </div>
    {/if}
  </Field>
  {#if apple}
    <Field label="Tone">
      <Select
        label="Polish tone"
        bind:value={cfg.polish.tone}
        options={TONES.map((t) => ({ value: t.value, text: t.name }))}
        onchange={changed}
      />
      <div class="example" data-testid="tone-example">{example}</div>
    </Field>
  {:else}
    <Field label="Command">
      <input
        type="text"
        aria-label="Polish command"
        placeholder={'claude -p "Clean up this text. Output only the cleaned text."'}
        bind:value={cfg.polish.command}
        oninput={() => settings.saveSoon()}
      >
      <div class="hint">
        text on stdin, result on stdout · e.g. claude -p "…" · codex exec "…" · ollama run llama3.2 "…"
      </div>
      {#if noCommand}
        <div class="hint">
          <span class="chip warn">!!</span>
          the Polish shortcut is on, but there is no command
        </div>
      {/if}
    </Field>
  {/if}
  <Field label="Timeout">
    <div class="range">
      <input
        type="range"
        min="5"
        max="300"
        step="5"
        aria-label="Polish timeout in seconds"
        bind:value={cfg.polish.timeout_secs}
        oninput={() => settings.saveSoon()}
      ><output>{cfg.polish.timeout_secs}s</output>
    </div>
  </Field>
  <Field label="Try it">
    <button disabled={running} onclick={test}>Test</button>
    <div class="out" class:dots={!out && !running}>
      {#if running}
        running…
      {:else if out?.ok}
        {out.text}
      {:else if out}
        <span class="chip warn">ERR</span>
        {out.text}
      {/if}
    </div>
  </Field>
{/if}

<style>
/* sample text, set like a quotation */
.example {
  margin-top: 8px;
  padding-left: 12px;
  border-left: 2px solid var(--lav);
  font: italic 17px / 22px var(--serif);
  white-space: pre-wrap;
}
.out {
  margin-top: 8px;
  min-height: 48px;
  border: 1px solid var(--ink);
  padding: 6px 10px;
  background: var(--bg);
  line-height: 18px;
  white-space: pre-wrap;
  -webkit-user-select: text;
  user-select: text;
}
.out.dots {
  border-style: dashed;
  border-color: var(--rule);
}
</style>
