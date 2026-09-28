<script lang="ts">
  import { api, errorMessage } from './api';
  import type { EngineSpec } from './bindings/EngineSpec';

  interface Props {
    /** Called after any change so the parent can reload the list. */
    onChanged: () => void;
    onClose: () => void;
  }
  let { onChanged, onClose }: Props = $props();

  let engines = $state<EngineSpec[]>([]);
  let editing = $state<{ id: string; name: string; program: string; args: string; workingDir: string } | null>(null);
  let status = $state<{ ok: boolean; text: string } | null>(null);
  let busy = $state(false);
  let dialog: HTMLDialogElement;

  $effect(() => {
    dialog.showModal();
    reload();
  });

  async function reload() {
    engines = await api.listEngines();
  }

  function edit(e: EngineSpec | null) {
    status = null;
    editing = e
      ? { id: e.id, name: e.name, program: e.program, args: e.args.join(' '), workingDir: e.workingDir ?? '' }
      : { id: '', name: '', program: '', args: '', workingDir: '' };
  }

  function spec(): EngineSpec {
    const e = editing!;
    return {
      id: e.id,
      name: e.name.trim(),
      program: e.program.trim(),
      args: e.args.split(/\s+/).filter((a) => a.length > 0),
      workingDir: e.workingDir.trim() || null,
    };
  }

  async function test() {
    busy = true;
    status = null;
    try {
      const id = await api.testEngine(spec());
      const parts = [id.name ?? 'unnamed', id.version && `version ${id.version}`, id.author && `by ${id.author}`];
      status = { ok: true, text: `OK: ${parts.filter(Boolean).join(', ')} (AEI protocol ${id.protocolVersion})` };
    } catch (e) {
      status = { ok: false, text: errorMessage(e) };
    } finally {
      busy = false;
    }
  }

  async function save() {
    try {
      await api.saveEngine(spec());
      editing = null;
      await reload();
      onChanged();
    } catch (e) {
      status = { ok: false, text: errorMessage(e) };
    }
  }

  async function remove(e: EngineSpec) {
    await api.deleteEngine(e.id);
    await reload();
    onChanged();
  }
</script>

<dialog bind:this={dialog} onclose={onClose}>
  <h2>Engines</h2>
  {#if editing}
    <div class="form">
      <label for="en-name">Name</label>
      <input id="en-name" bind:value={editing.name} placeholder="e.g. Sharp" />
      <label for="en-program">Program</label>
      <input id="en-program" bind:value={editing.program} placeholder="/path/to/engine" />
      <label for="en-args">Arguments</label>
      <input id="en-args" bind:value={editing.args} placeholder="e.g. aei" />
      <label for="en-dir">Working dir</label>
      <input id="en-dir" bind:value={editing.workingDir} placeholder="optional" />
    </div>
    <p class="hint">The program is run directly, not through a shell. Arguments are split on spaces.</p>
    {#if status}<p class:ok={status.ok} class="status">{status.text}</p>{/if}
    <div class="buttons">
      <button onclick={test} disabled={busy}>{busy ? 'Testing…' : 'Test'}</button>
      <span class="spacer"></span>
      <button onclick={() => (editing = null)}>Cancel</button>
      <button class="primary" onclick={save}>Save</button>
    </div>
  {:else}
    {#if engines.length === 0}
      <p class="hint">No engines yet. Add any program that speaks AEI.</p>
    {/if}
    <ul>
      {#each engines as e (e.id)}
        <li>
          <div class="info">
            <strong>{e.name}</strong>
            <code>{[e.program, ...e.args].join(' ')}</code>
          </div>
          <button onclick={() => edit(e)}>Edit</button>
          <button onclick={() => remove(e)}>Delete</button>
        </li>
      {/each}
    </ul>
    <div class="buttons">
      <button onclick={() => edit(null)}>Add engine…</button>
      <span class="spacer"></span>
      <button class="primary" onclick={onClose}>Done</button>
    </div>
  {/if}
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
  ul {
    list-style: none;
    margin: 0;
    padding: 0;
  }
  li {
    display: flex;
    align-items: center;
    gap: 6px;
    padding: 6px 0;
    border-bottom: 1px solid var(--border);
  }
  .info {
    flex: 1;
    min-width: 0;
    display: flex;
    flex-direction: column;
  }
  code {
    font-size: 11px;
    color: var(--muted);
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .form {
    display: grid;
    grid-template-columns: auto 1fr;
    gap: 8px 12px;
    align-items: center;
  }
  input {
    font: inherit;
    padding: 4px 8px;
    border: 1px solid var(--border);
    border-radius: 5px;
    background: var(--bg);
    color: var(--text);
  }
  .hint {
    font-size: 12px;
    color: var(--muted);
  }
  .status {
    font-size: 13px;
    color: var(--warn);
    word-break: break-word;
  }
  .status.ok {
    color: var(--accent);
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
