<script lang="ts">
  // An engine's options for one game, or for analysis in this window, over
  // its saved settings; and its button options, pressed from here.
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
    /** Options for a game, or for analysis. */
    scope?: 'game' | 'analysis';
    /** The game's (or analysis's) options so far. */
    current: EngineOption[];
    /** Whether the engine is running, so changes reach it. */
    running: boolean;
    /** Takes the new options; resolves to an error message, or null. */
    onApply: (options: EngineOption[]) => Promise<string | null>;
    /** Presses a button option of the running engine; resolves to an error
     * message, or null. */
    onPress?: (name: string) => Promise<string | null>;
    onClose: () => void;
  }
  let { engine, scope = 'game', current, running, onApply, onPress, onClose }: Props = $props();

  let described = $state<ManifestOptionView[] | null>(null);
  /** The manifest's button options. */
  let buttons = $state<ManifestOptionView[]>([]);
  /** The button pressed last, for the note under the buttons. */
  let pressed = $state<string | null>(null);
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
    buttons = options.filter((o) => o.kind === 'button');
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

  async function press(name: string) {
    if (!onPress) return;
    try {
      error = await onPress(name);
    } catch (e) {
      error = errorMessage(e);
    }
    pressed = error ? null : name;
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
  <h2>{engine.name}: options for {scope === 'game' ? 'this game' : 'analysis'}</h2>
  {#if described}
    {#if buttons.length && onPress && running}
      <div class="actions" role="group" aria-label="Engine actions">
        {#each buttons as b (b.name)}
          <button onclick={() => press(b.name)} title={b.description ?? `Send ${b.name} to the engine now`}
            >{b.name}</button
          >
        {/each}
      </div>
      <p class="hint">
        {#if pressed}Sent {pressed}.{/if}
        Buttons go to the engine at once, even while it's thinking.
      </p>
    {/if}
    <div class="form">
      <OptionFields {described} bind:values idPrefix="go-opt" />
      <label for="go-other">{described.length ? 'Other options' : 'Options'}</label>
      <textarea id="go-other" rows="3" bind:value={other} placeholder="e.g. threads = 2"></textarea>
    </div>
    <p class="hint">
      These apply to {scope === 'game' ? 'this game' : 'analysis in this window'} only, over the engine's saved
      settings (Engines).
      {#if described.length}
        Fields show what the engine plays with; a blank one keeps the saved value or the default. Other options
      {:else}
        Options
      {/if}
      are one per line, as <code>name = value</code>.
      {#if scope === 'analysis'}The search starts again with changes.
      {:else if running}Changes reach the engine before its next move.{/if}
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
  .actions {
    display: flex;
    flex-wrap: wrap;
    gap: 6px;
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
