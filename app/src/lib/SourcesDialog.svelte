<script lang="ts">
  // Where the game database's contents came from (the Games panel's
  // Sources…): this computer, arimaa.com's archive, and each file
  // imported, with how much each brought. A source's games can be listed,
  // and what came from it removed.
  import { api, errorMessage } from './api';
  import type { LibrarySourceView } from './bindings/LibrarySourceView';
  import { clock } from './clock.svelte';
  import { sourceName } from './library';

  interface Props {
    /** Lists a source's games. */
    onShow: (source: number) => void;
    /** Something was removed. */
    onChanged: () => void;
    onClose: () => void;
  }
  let { onShow, onChanged, onClose }: Props = $props();

  let dialog: HTMLDialogElement;
  $effect(() => {
    dialog.showModal();
  });

  let sources = $state<LibrarySourceView[] | null>(null);
  let error = $state<string | null>(null);
  let busy = $state<number | null>(null);
  let confirming = $state<number | null>(null);
  let removed = $state<string | null>(null);

  async function load() {
    try {
      sources = await api.librarySources();
    } catch (e) {
      error = errorMessage(e);
    }
  }
  void load();

  const n = (x: number, one: string, many: string) => `${x.toLocaleString()} ${x === 1 ? one : many}`;

  function counts(s: LibrarySourceView): string {
    const parts = [n(s.games, 'game', 'games')];
    if (s.annotations > 0) parts.push(n(s.annotations, 'annotation', 'annotations'));
    if (s.evals > 0) parts.push(n(s.evals, 'eval', 'evals'));
    return parts.join(', ');
  }

  function when(secs: number): string {
    return new Date(secs * 1000).toLocaleDateString(undefined, { dateStyle: 'medium', ...clock.options });
  }

  async function remove(s: LibrarySourceView) {
    if (confirming !== s.id) {
      confirming = s.id;
      return;
    }
    confirming = null;
    busy = s.id;
    error = null;
    try {
      const r = await api.libraryRemoveSource(s.id);
      removed =
        `Removed ${n(r.games, 'game', 'games')}, ${n(r.annotations, 'annotation', 'annotations')} and ` +
        `${n(r.evals, 'eval', 'evals')} from ${sourceName(s)}` +
        (r.gamesKept > 0 ? `; ${n(r.gamesKept, 'game', 'games')} with comments from elsewhere stayed.` : '.');
      onChanged();
      await load();
    } catch (e) {
      error = errorMessage(e);
    }
    busy = null;
  }
</script>

<dialog bind:this={dialog} onclose={onClose} aria-labelledby="sources-title">
  <h2 id="sources-title">Sources</h2>
  <p class="note">
    Everything in your games remembers where it came from. Removing a source takes out its games (except those with
    comments from you or another source), its comments and its evals; moves it added to your games stay.
  </p>
  {#if sources == null}
    {#if !error}<p>Loading…</p>{/if}
  {:else}
    <ul class="sources" aria-label="Sources">
      {#each sources as s (s.id)}
        <li>
          <div class="about">
            <span class="name">{sourceName(s)}</span>
            <span class="meta">{counts(s)}{s.own ? '' : ` · imported ${when(s.addedAt)}`}</span>
          </div>
          <button
            onclick={() => {
              onShow(s.id);
              dialog.close();
            }}
            disabled={s.games === 0}>Games</button
          >
          {#if !s.own}
            <button
              class:danger={confirming === s.id}
              onclick={() => remove(s)}
              onblur={() => confirming === s.id && (confirming = null)}
              disabled={busy != null}
              title="Take out what came from it"
              >{busy === s.id ? 'Removing…' : confirming === s.id ? 'Remove all of it?' : 'Remove'}</button
            >
          {/if}
        </li>
      {/each}
    </ul>
  {/if}
  {#if removed}<p class="done" role="status">{removed}</p>{/if}
  {#if error}<p class="error" role="alert">{error}</p>{/if}
  <div class="buttons">
    <span class="spacer"></span>
    <button onclick={() => dialog.close()}>Close</button>
  </div>
</dialog>

<style>
  dialog {
    width: min(560px, 92vw);
  }
  h2 {
    margin: 0 0 6px;
    font-size: 18px;
  }
  .note {
    color: var(--muted);
    margin: 0 0 12px;
  }
  .sources {
    list-style: none;
    padding: 0;
    margin: 0 0 12px;
  }
  .sources li {
    display: flex;
    align-items: center;
    gap: 8px;
    padding: 6px 0;
    border-bottom: 1px solid var(--border);
  }
  .about {
    flex: 1;
    display: flex;
    flex-direction: column;
    min-width: 0;
  }
  .name {
    font-weight: 600;
  }
  .meta {
    color: var(--muted);
    font-size: 12px;
  }
  .buttons {
    display: flex;
    gap: 8px;
  }
  button.danger {
    color: var(--warn);
    font-weight: 600;
  }
  .spacer {
    flex: 1;
  }
</style>
