<script lang="ts">
import { type AppleAi, api, type Tone } from "@/lib/api";
import { chord } from "@/lib/format";
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

const PROVIDERS = [
  { value: "command", text: "A command of yours" },
  { value: "apple", text: "Apple, on this Mac (macOS 26)" },
];
const save = () => settings.save();
</script>

{#if cfg}
  <Field label="Shortcut">
    {#if cfg.hotkeys.polish.length}
      <kbd>{chord(cfg.hotkeys.polish)}</kbd>
      <span class="hint">polishes the selected text, or the whole field · change it in General</span>
    {:else}
      <span class="hint">off · turn it on in General</span>
    {/if}
  </Field>
  <Field label="Rewrite with">
    <Select
      label="Rewrite with"
      bind:value={cfg.polish.provider}
      options={PROVIDERS}
      onchange={save}
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
        onchange={save}
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
</style>
