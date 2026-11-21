<script lang="ts">
  import { untrack } from 'svelte';
  import { api } from './api';

  let {
    initial,
    hasEvals = false,
    hasChances = false,
    onExport,
    onLoad,
    onClose,
  }: {
    initial: string;
    /** Whether there are evals the record can carry as `[%eval]` commands
     * (`SessionView.recordEvals`). */
    hasEvals?: boolean;
    /** Whether there are win chances the record can carry as `[%win]`
     * commands (`SessionView.recordChances`). */
    hasChances?: boolean;
    /** The game's record again, in full or only the main line, with or
     * without evals and win chances. Without it the dialog only opens a
     * record (the lobby's Open record). */
    onExport?: (mainLineOnly: boolean, evals: boolean, chances: boolean) => Promise<string>;
    onLoad: (record: string) => Promise<string | null>;
    onClose: () => void;
  } = $props();

  // The dialog edits its own copy of the record.
  let text = $state(untrack(() => initial));
  // The game's record as last exported: loading that back would change
  // nothing but could add commands (`[%eval]`) to the game's comments, so
  // Load waits for an edit, and Copy is the main button.
  let exported = $state(untrack(() => (onExport ? initial : null)));
  const canLoad = $derived(text.trim() !== '' && text !== exported);
  let error = $state<string | null>(null);
  let copied = $state(false);
  let mainLineOnly = $state(false);
  let withEvals = $state(false);
  let withChances = $state(false);

  async function reexport() {
    if (onExport) text = exported = await onExport(mainLineOnly, withEvals, withChances);
    copied = false;
  }
  let dialog: HTMLDialogElement;

  $effect(() => {
    dialog.showModal();
  });

  async function load() {
    if (!canLoad) return;
    error = await onLoad(text);
    if (!error) onClose();
  }

  async function openFile(e: Event) {
    const file = (e.currentTarget as HTMLInputElement).files?.[0];
    if (file) text = await file.text();
  }

  async function copy() {
    try {
      await api.copyText(text);
      copied = true;
    } catch {
      error = 'Clipboard not available; select the text and copy it instead.';
    }
  }
</script>

<dialog bind:this={dialog} onclose={onClose} aria-labelledby="record-title">
  <h2 id="record-title">{onExport ? 'Game record' : 'Open a record'}</h2>
  <p class="hint">Paste a record (one move per line, e.g. <code>2g Ed2n Ed3n</code>) or open a file.</p>
  <!-- svelte-ignore a11y_autofocus -->
  <textarea
    bind:value={text}
    spellcheck="false"
    autofocus
    onkeydown={(e) => {
      if (e.key === 'Enter' && (e.ctrlKey || e.metaKey)) {
        e.preventDefault();
        load();
      }
    }}></textarea>
  {#if error}<p class="error">{error}</p>{/if}
  <div class="buttons">
    <label class="file">Open file… <input type="file" accept=".txt,text/plain" onchange={openFile} /></label>
    {#if onExport}
      <button class:primary={!canLoad} onclick={copy}>{copied ? 'Copied' : 'Copy'}</button>
      <label class="check" title="A plain record without variations, comments or tags, for tools that read only those">
        <input
          type="checkbox"
          checked={mainLineOnly}
          onchange={(e) => {
            mainLineOnly = e.currentTarget.checked;
            reexport();
          }}
        />
        Main line only
      </label>
      {#if hasEvals}
        <label class="check" title="Each analysed move's eval as [%eval] in its comment, from gold's side">
          <input
            type="checkbox"
            checked={withEvals}
            disabled={mainLineOnly}
            onchange={(e) => {
              withEvals = e.currentTarget.checked;
              reexport();
            }}
          />
          Evals
        </label>
      {/if}
      {#if hasChances}
        <label
          class="check"
          title="Gold's chance of winning, where the engine gives one, as [%win] in the move's comment"
        >
          <input
            type="checkbox"
            checked={withChances}
            disabled={mainLineOnly}
            onchange={(e) => {
              withChances = e.currentTarget.checked;
              reexport();
            }}
          />
          Win %
        </label>
      {/if}
    {/if}
    <span class="spacer"></span>
    <button onclick={onClose}>Close</button>
    <button
      class:primary={canLoad || !onExport}
      onclick={load}
      disabled={!canLoad}
      title={canLoad
        ? 'Load (Ctrl+Enter)'
        : onExport
          ? 'Edit the record to load it as the game'
          : 'Paste or open a record first'}>Load</button
    >
  </div>
</dialog>

<style>
  dialog {
    width: min(680px, 90vw);
  }
  h2 {
    margin: 0 0 4px;
    font-size: 16px;
  }
  .hint {
    margin: 0 0 8px;
    font-size: 12px;
    color: var(--muted);
  }
  textarea {
    width: 100%;
    height: 320px;
    box-sizing: border-box;
    font-family: ui-monospace, 'DejaVu Sans Mono', monospace;
    font-size: 12px;
    background: var(--bg);
    color: var(--text);
    border: 1px solid var(--border);
    border-radius: 4px;
    padding: 8px;
  }
  .error {
    color: var(--warn);
    font-size: 13px;
  }
  .buttons {
    display: flex;
    gap: 6px;
    margin-top: 8px;
    align-items: center;
  }
  .spacer {
    flex: 1;
  }
  .check {
    display: flex;
    align-items: center;
    gap: 4px;
    font-size: 13px;
  }
  .file {
    position: relative;
    overflow: hidden;
  }
  .file input {
    position: absolute;
    inset: 0;
    opacity: 0;
    cursor: pointer;
  }
</style>
