<script lang="ts">
import type { Appearance } from "@/lib/api";
import { applyTheme } from "@/lib/theme";
import Field from "./Field.svelte";
import { settings } from "./state.svelte";

const cfg = $derived(settings.cfg);
const APPEARANCES: Appearance[] = ["system", "light", "dark"];

function appearance(a: Appearance) {
  if (!cfg) return;
  cfg.theme.appearance = a;
  applyTheme($state.snapshot(cfg.theme));
  settings.save();
}
</script>

{#if cfg}
  <Field label="Appearance">
    <div class="inline">
      {#each APPEARANCES as a (a)}
        <button aria-pressed={cfg.theme.appearance === a} onclick={() => appearance(a)}>{a}</button>
      {/each}
    </div>
    <div class="hint">system follows macOS's light or dark mode</div>
  </Field>
{/if}
