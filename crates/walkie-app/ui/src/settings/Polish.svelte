<script lang="ts">
import { type AppleAi, api, message } from "@/lib/api";
import Field from "./Field.svelte";
import { settings } from "./state.svelte";

const cfg = $derived(settings.cfg);
const apple = $derived(cfg?.polish.provider === "apple");
const noCommand = $derived(!!cfg?.hotkeys.polish.length && !apple && !cfg.polish.command.trim());

let ai = $state<AppleAi | null>(null);
$effect(() => {
  if (apple && !ai) api.appleAiStatus().then((s) => (ai = s));
});

let running = $state(false);
let out = $state<{ ok: boolean; text: string } | null>(null);

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
  <Field label="Model" restart>
    <select
      aria-label="Polish model"
      bind:value={cfg.polish.provider}
      onchange={() => {
        out = null;
        settings.save();
      }}
    >
      <option value="command">Your command</option>
      <option value="apple">Apple, on this Mac (macOS 26)</option>
    </select>
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
    <Field label="Prompt" restart>
      <textarea
        rows="6"
        aria-label="Polish prompt"
        bind:value={cfg.polish.prompt}
        oninput={() => settings.saveSoon()}
      ></textarea>
      <div class="hint">what the model does with your words; it gets the transcript as the text</div>
    </Field>
  {:else}
    <Field label="Command" restart>
      <input
        type="text"
        aria-label="Polish command"
        placeholder={'claude -p "Clean up this dictated text. Output only the cleaned text."'}
        bind:value={cfg.polish.command}
        oninput={() => settings.saveSoon()}
      >
      <div class="hint">
        transcript on stdin, result on stdout · e.g. claude -p "…" · codex exec "…" · ollama run llama3.2 "…"
      </div>
      {#if noCommand}
        <div class="hint">
          <span class="chip warn">!!</span>
          the Dictate + polish shortcut is on, but there is no command
        </div>
      {/if}
    </Field>
  {/if}
  <Field label="Timeout" restart>
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
  <p class="hint"><span class="req">*</span> applies after restarting walkie.</p>
{/if}

<style>
.out {
  margin-top: 8px;
  min-height: 44px;
  border: 1px solid var(--line);
  padding: 4px 6px;
  background-color: var(--paper);
  line-height: 16px;
  white-space: pre-wrap;
  -webkit-user-select: text;
  user-select: text;
}
</style>
