<script lang="ts">
  // The daemon's archived engine tests (zips in its archive/ folder): each
  // can be restored to the queue as it was, or deleted for good.
  import { onMount } from 'svelte';
  import { api, errorMessage } from './api';
  import type { ArchivedTestView } from './bindings/ArchivedTestView';
  import type { TestView } from './bindings/TestView';
  import { on } from './events';
  import { scoreText, testState } from './tests';

  interface Props {
    /** The daemon's data directory, for saying where the zips are. */
    dataDir: string | null;
    /** A test went back in the queue. */
    onRestored: (t: TestView) => void;
    onClose: () => void;
  }
  let { dataDir, onRestored, onClose }: Props = $props();

  let archived = $state<ArchivedTestView[] | null>(null);
  let error = $state<string | null>(null);
  let busy = $state<string | null>(null);
  /** The zip whose Delete was clicked once; the second click deletes. */
  let confirmDelete = $state<string | null>(null);
  let dialog: HTMLDialogElement;

  $effect(() => {
    dialog.showModal();
  });

  async function load() {
    try {
      archived = await api.tourneyArchived();
    } catch (e) {
      error = errorMessage(e);
    }
  }

  onMount(() => {
    void load();
    // A test archived meanwhile joins the list.
    const unlisten = on('tourney://changed', (u) => {
      if (u.removed) void load();
    });
    return () => void unlisten.then((f) => f());
  });

  async function act(file: string, f: () => Promise<void>) {
    busy = file;
    error = null;
    try {
      await f();
      archived = (archived ?? []).filter((a) => a.file !== file);
    } catch (e) {
      error = errorMessage(e);
    }
    busy = null;
  }

  const restore = (file: string) =>
    act(file, async () => {
      onRestored(await api.tourneyRestore(file));
    });

  function remove(file: string) {
    if (confirmDelete !== file) {
      confirmDelete = file;
      return;
    }
    confirmDelete = null;
    void act(file, () => api.tourneyDeleteArchived(file));
  }

  function when(secs: number): string {
    return new Date(secs * 1000).toLocaleString(undefined, { dateStyle: 'medium', timeStyle: 'short' });
  }

  function size(bytes: number): string {
    return bytes >= 1e6 ? `${(bytes / 1e6).toFixed(1)} MB` : `${Math.max(1, Math.round(bytes / 1e3))} kB`;
  }

  const folder = $derived(dataDir ? `${dataDir}${dataDir.includes('\\') ? '\\' : '/'}archive` : null);
</script>

<dialog bind:this={dialog} onclose={onClose} aria-labelledby="archive-title">
  <h2 id="archive-title">Archived tests</h2>
  <p class="note">Restoring puts a test back in the list as it was when archived; deleting removes its zip for good.</p>
  {#if archived == null}
    {#if !error}<p>Loading…</p>{/if}
  {:else if archived.length === 0}
    <p class="note">No archived tests.</p>
  {:else}
    <ul class="tests" aria-label="Archived tests">
      {#each archived as a (a.file)}
        <li>
          <div class="about">
            {#if a.test}
              <span class="title">
                <span class="name">{a.test.name}</span>
                <span class="state">{testState(a.test)}</span>
              </span>
              <span class="meta">{a.test.engines[0]} vs {a.test.engines[1]}</span>
              <span class="meta">{scoreText(a.test, true)}{a.test.elo ? `, Elo ${a.test.elo}` : ''}</span>
            {:else}
              <span class="title"><span class="name">{a.file}</span></span>
              <span class="meta error">Can't be read: {a.error}</span>
            {/if}
            <span class="meta">Archived {when(a.archived)}, {size(a.size)} ({a.file})</span>
          </div>
          {#if a.test}
            <button
              onclick={() => restore(a.file)}
              disabled={busy != null}
              title="Put the test back in the list, as it was when archived">Restore</button
            >
          {/if}
          <button
            class:danger={confirmDelete === a.file}
            onclick={() => remove(a.file)}
            onblur={() => confirmDelete === a.file && (confirmDelete = null)}
            disabled={busy != null}
            title="Delete the archived test and all its games for good"
            >{confirmDelete === a.file ? 'Delete for good?' : 'Delete'}</button
          >
        </li>
      {/each}
    </ul>
  {/if}
  {#if error}<p class="error" role="alert">{error}</p>{/if}
  {#if folder}<p class="note path">Kept in {folder}</p>{/if}
  <div class="buttons">
    <span class="spacer"></span>
    <button onclick={() => dialog.close()}>Close</button>
  </div>
</dialog>

<style>
  dialog {
    width: min(620px, 92vw);
    max-height: 88vh;
  }
  dialog[open] {
    display: flex;
    flex-direction: column;
  }
  h2 {
    margin: 0 0 4px;
    font-size: 16px;
  }
  .note {
    margin: 0 0 8px;
    font-size: 12px;
    color: var(--muted);
  }
  .path {
    margin-top: 8px;
    overflow-wrap: anywhere;
  }
  .tests {
    list-style: none;
    margin: 0;
    padding: 0;
    overflow-y: auto;
    min-height: 0;
    display: flex;
    flex-direction: column;
    gap: 6px;
  }
  .tests li {
    display: flex;
    align-items: center;
    gap: 6px;
    padding: 6px 8px;
    border: 1px solid var(--border);
    border-radius: 6px;
  }
  .about {
    flex: 1;
    min-width: 0;
    display: flex;
    flex-direction: column;
    gap: 2px;
  }
  .title {
    display: flex;
    align-items: center;
    gap: 6px;
    font-weight: 600;
    font-size: 13px;
  }
  .name {
    overflow: hidden;
    white-space: nowrap;
    text-overflow: ellipsis;
  }
  .state {
    flex: none;
    font-size: 11px;
    font-weight: 400;
    padding: 0 6px;
    border-radius: 8px;
    border: 1px solid var(--border);
    color: var(--muted);
  }
  .meta {
    color: var(--muted);
    font-size: 12px;
    overflow-wrap: anywhere;
  }
  .error {
    color: var(--warn);
    font-size: 13px;
  }
  .buttons {
    display: flex;
    gap: 6px;
    margin-top: 14px;
  }
  .spacer {
    flex: 1;
  }
  .danger {
    color: var(--warn);
  }
</style>
