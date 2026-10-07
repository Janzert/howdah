<script lang="ts">
  import { api, errorMessage } from './api';
  import type { EngineOption } from './bindings/EngineOption';
  import type { EngineSpec } from './bindings/EngineSpec';
  import type { ManifestOptionView } from './bindings/ManifestOptionView';
  import {
    formatOptions,
    gameOptions,
    optionError,
    parseOptions,
    settingOptions,
    splitOptions,
  } from './engineOptions';
  import OptionFields from './OptionFields.svelte';

  interface Props {
    engine: EngineSpec;
    /** The game's options so far. */
    current: EngineOption[];
    /** Whether the game is being played, so changes reach a running engine. */
    running: boolean;
    /** Takes the game's new options; resolves to an error message, or null. */
    onApply: (options: EngineOption[]) => Promise<string | null>;
    onClose: () => void;
  }
  let { engine, current, running, onApply, onClose }: Props = $props();

  let described = $state<ManifestOptionView[] | null>(null);
  let values = $state<Record<string, string>>({});
  let other = $state('');
  let error = $state<string | null>(null);
  let dialog: HTMLDialogElement;

  $effect(() => {
    dialog.showModal();
    load();
  });

  /** Fields from the engine's manifest, filled with the game's values or
   * else the saved ones; the game's other options as lines. */
  async function load() {
    let options: ManifestOptionView[] = [];
    if (engine.installed) {
      try {
        const catalog = await api.engineCatalog();
        options = catalog.manifests.find((m) => m.id === engine.installed?.manifest)?.options ?? [];
      } catch {
        /* no fields, only lines */
      }
    }
    const shown = settingOptions(options);
    const saved = splitOptions(engine.options, shown).values;
    const game = splitOptions(current, shown);
    values = { ...saved, ...game.values };
    other = formatOptions(game.other);
    described = shown;
  }

  /** Back to the engine's saved settings (applied, the game's options that
   * have saved values or defaults go back to them). */
  function reset() {
    values = splitOptions(engine.options, described ?? []).values;
    other = '';
  }

  async function apply() {
    const fields = described ?? [];
    for (const o of fields) {
      const e = optionError(o, values[o.name] ?? '');
      if (e) {
        error = `${o.name} ${e}`;
        return;
      }
    }
    const options = [...gameOptions(fields, values, engine.options, current), ...parseOptions(other)];
    try {
      error = await onApply(options);
    } catch (e) {
      error = errorMessage(e);
    }
    if (!error) dialog.close();
  }
</script>

<dialog bind:this={dialog} onclose={onClose}>
  <h2>{engine.name}: options for this game</h2>
  {#if described}
    <div class="form">
      <OptionFields {described} bind:values idPrefix="go-opt" />
      <label for="go-other">{described.length ? 'Other options' : 'Options'}</label>
      <textarea id="go-other" rows="3" bind:value={other} placeholder="e.g. threads = 2"></textarea>
    </div>
    <p class="hint">
      These apply to this game only, over the engine's saved settings (Engines).
      {#if described.length}
        Fields show what the engine plays with; a blank one keeps the saved value or the default. Other options
      {:else}
        Options
      {/if}
      are one per line, as <code>name = value</code>.
      {#if running}Changes reach the engine before its next move.{/if}
    </p>
  {:else}
    <p class="hint">Loading…</p>
  {/if}
  {#if error}<p class="error" role="alert">{error}</p>{/if}
  <div class="buttons">
    <button onclick={reset} disabled={!described}>Saved settings</button>
    <span class="spacer"></span>
    <button onclick={() => dialog.close()}>Cancel</button>
    <button class="primary" onclick={apply} disabled={!described}>{running ? 'Apply' : 'OK'}</button>
  </div>
</dialog>

<style>
  dialog {
    width: min(620px, 92vw);
    border: 1px solid var(--border);
    border-radius: 8px;
    background: var(--panel);
    color: var(--text);
    padding: 16px;
  }
  dialog::backdrop {
    background: rgba(0, 0, 0, 0.4);
  }
  h2 {
    margin: 0 0 12px;
    font-size: 16px;
  }
  .form {
    display: grid;
    grid-template-columns: auto 1fr;
    gap: 8px 12px;
    align-items: center;
  }
  label[for='go-other'] {
    align-self: start;
    padding-top: 4px;
  }
  textarea {
    font: inherit;
    padding: 4px 8px;
    border: 1px solid var(--border);
    border-radius: 5px;
    background: var(--bg);
    color: var(--text);
    resize: vertical;
    font-family: ui-monospace, 'DejaVu Sans Mono', monospace;
    font-size: 12px;
  }
  .hint {
    font-size: 12px;
    color: var(--muted);
  }
  .hint code {
    font-size: 11px;
  }
  .error {
    font-size: 13px;
    color: var(--warn);
  }
  .buttons {
    display: flex;
    gap: 6px;
    margin-top: 12px;
  }
  .spacer {
    flex: 1;
  }
</style>
