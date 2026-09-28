<script lang="ts">
import { api, message } from "@/lib/api";
import Field from "./Field.svelte";
import { settings } from "./state.svelte";

const cfg = $derived(settings.cfg);
const noCommand = $derived(!!cfg?.hotkeys.polish.length && !cfg.polish.command.trim());

let running = $state(false);
let out = $state<{ ok: boolean; text: string } | null>(null);

async function test() {
  if (!cfg) return;
  running = true;
  out = null;
  try {
    const r = await api.testPolish(cfg.polish.command, cfg.polish.timeout_secs);
    out = { ok: true, text: `→ ${r.trim()}` };
  } catch (e) {
    out = { ok: false, text: message(e) };
  }
  running = false;
}
</script>

{#if cfg}
  <Field label="Command" restart>
    <input
      type="text"
      aria-label="Polish command"
      placeholder={'claude -p "Clean up this dictated text. Output only the cleaned text."'}
      bind:value={cfg.polish.command}
      oninput={() => settings.saveSoon()}
    >
    <div class="hint">e.g. claude -p "…" · codex exec "…" · ollama run llama3.2 "…"</div>
    {#if noCommand}
      <div class="hint">
        <span class="chip warn">!!</span>
        the Dictate + polish shortcut is on, but there is no command
      </div>
    {/if}
  </Field>
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
