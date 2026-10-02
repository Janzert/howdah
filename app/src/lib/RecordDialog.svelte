<script lang="ts">
  import { untrack } from 'svelte';

  let {
    initial,
    onLoad,
    onClose,
  }: { initial: string; onLoad: (record: string) => Promise<string | null>; onClose: () => void } = $props();

  // The dialog edits its own copy of the record.
  let text = $state(untrack(() => initial));
  let error = $state<string | null>(null);
  let copied = $state(false);
  let dialog: HTMLDialogElement;

  $effect(() => {
    dialog.showModal();
  });

  async function load() {
    error = await onLoad(text);
    if (!error) onClose();
  }

  async function openFile(e: Event) {
    const file = (e.currentTarget as HTMLInputElement).files?.[0];
    if (file) text = await file.text();
  }

  async function copy() {
    try {
      await navigator.clipboard.writeText(text);
      copied = true;
    } catch {
      error = 'Clipboard not available; select the text and copy it instead.';
    }
  }
</script>

<dialog bind:this={dialog} onclose={onClose}>
  <h2>Game record</h2>
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
    }}
  ></textarea>
  {#if error}<p class="error">{error}</p>{/if}
  <div class="buttons">
    <label class="file">Open file… <input type="file" accept=".txt,text/plain" onchange={openFile} /></label>
    <button onclick={copy}>{copied ? 'Copied' : 'Copy'}</button>
    <span class="spacer"></span>
    <button onclick={onClose}>Close</button>
    <button class="primary" onclick={load} title="Load (Ctrl+Enter)">Load</button>
  </div>
</dialog>

<style>
  dialog {
    width: min(680px, 90vw);
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
