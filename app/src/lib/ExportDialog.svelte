<script lang="ts">
  // Exporting from the game database (the Games panel's Export…): the
  // games the list shows, or only engine evals, as a database file in the
  // same format, to share or keep. Whoever imports it sees the user's
  // comments under the name given here.
  import { untrack } from 'svelte';
  import { api, errorMessage } from './api';
  import type { LibraryEngineView } from './bindings/LibraryEngineView';
  import type { LibraryExportView } from './bindings/LibraryExportView';
  import type { LibraryFilter } from './bindings/LibraryFilter';
  import { EXPORT_FILTERS, chooseSavePath, exportName, nativeFiles } from './files';
  import { pref, savePref } from './gameroom/store.svelte';

  interface Props {
    /** The list's filter, and how many games it lets through. */
    filter: LibraryFilter;
    count: number | null;
    /** Words for what the list shows, for the file's name ("99of9"). */
    what: string;
    onClose: () => void;
  }
  let { filter, count, what, onClose }: Props = $props();

  let dialog: HTMLDialogElement;
  $effect(() => {
    dialog.showModal();
  });

  let games = $state<'list' | 'none'>('list');
  let variations = $state(true);
  let comments = $state(true);
  let times = $state(true);
  let chat = $state(false);
  let evals = $state<'none' | 'games' | 'all'>('games');
  let author = $state(pref('lobby.exportAuthor') ?? '');
  /** The engines with evals, and those chosen (all at first). */
  let engines = $state<LibraryEngineView[]>([]);
  let chosen = $state<Set<string>>(new Set());
  /** In the browser preview, the path typed. */
  let path = $state(untrack(() => exportName(what)));
  let busy = $state(false);
  let error = $state<string | null>(null);
  let done = $state<LibraryExportView | null>(null);

  void api
    .libraryEvalEngines()
    .then((e) => {
      engines = e;
      chosen = new Set(e.map((x) => x.author));
    })
    .catch(() => {});

  function toggle(author: string, on: boolean) {
    const next = new Set(chosen);
    if (on) next.add(author);
    else next.delete(author);
    chosen = next;
  }

  $effect(() => {
    if (games === 'none' && evals === 'games') evals = 'all';
  });

  const nothing = $derived(
    (games === 'none' || count === 0) && (evals === 'none' || (engines.length > 0 && chosen.size === 0)),
  );

  async function exportNow() {
    error = null;
    done = null;
    const to = nativeFiles ? await chooseSavePath(exportName(what), EXPORT_FILTERS) : path.trim();
    if (!to) return;
    savePref('lobby.exportAuthor', author.trim());
    busy = true;
    try {
      done = await api.libraryExport(
        {
          filter: games === 'list' ? filter : null,
          games: null,
          variations,
          comments,
          times,
          chat,
          evals,
          engines: engines.length > 0 && chosen.size < engines.length ? [...chosen] : null,
          author: author.trim(),
        },
        to,
      );
    } catch (e) {
      error = errorMessage(e);
    }
    busy = false;
  }

  const n = (x: number, one: string, many: string) => `${x.toLocaleString()} ${x === 1 ? one : many}`;
</script>

<dialog bind:this={dialog} onclose={onClose} aria-labelledby="export-title">
  <h2 id="export-title">Export games</h2>
  <p class="note">
    Writes a game database file in Howdah's format, to keep or to share. Anyone can import it into their Games, and what
    they already have is kept: new games are added, and new variations and comments are joined into games they have.
  </p>

  <fieldset>
    <legend>Games</legend>
    <label
      ><input type="radio" bind:group={games} value="list" />
      {count == null ? 'The games in the list' : `The ${n(count, 'game', 'games')} in the list`}</label
    >
    <label><input type="radio" bind:group={games} value="none" /> No games, only engine evals</label>
  </fieldset>

  {#if games === 'list'}
    <fieldset>
      <legend>With them</legend>
      <label><input type="checkbox" bind:checked={variations} /> Variations</label>
      <label><input type="checkbox" bind:checked={comments} /> Comments and move marks</label>
      <label><input type="checkbox" bind:checked={times} /> Move times</label>
      <label><input type="checkbox" bind:checked={chat} /> The games' chat</label>
    </fieldset>
  {/if}

  <fieldset>
    <legend>Engine evals</legend>
    <label><input type="radio" bind:group={evals} value="none" /> None</label>
    {#if games === 'list'}
      <label><input type="radio" bind:group={evals} value="games" /> For the positions in these games</label>
    {/if}
    <label><input type="radio" bind:group={evals} value="all" /> All of them</label>
    {#if evals !== 'none'}
      {#if engines.length === 0}
        <p class="note">There are no engine evals yet.</p>
      {:else}
        <div class="engines" role="group" aria-label="Engines">
          {#each engines as e (e.author)}
            <label
              ><input
                type="checkbox"
                checked={chosen.has(e.author)}
                onchange={(ev) => toggle(e.author, ev.currentTarget.checked)}
              />
              {e.author} <span class="muted">({n(e.evals, 'position', 'positions')})</span></label
            >
          {/each}
        </div>
      {/if}
    {/if}
  </fieldset>

  <label class="field"
    >Your name, for your comments and evals
    <input bind:value={author} placeholder="Anonymous" autocomplete="off" spellcheck="false" /></label
  >
  {#if !nativeFiles}
    <label class="field"
      >Save to (a path on this computer)
      <input bind:value={path} aria-label="Save to" autocomplete="off" spellcheck="false" /></label
    >
  {/if}

  {#if error}<p class="error" role="alert">{error}</p>{/if}
  {#if done}
    <p class="done" role="status">
      Exported {n(done.games, 'game', 'games')}{done.annotations > 0
        ? `, ${n(done.annotations, 'annotation', 'annotations')}`
        : ''}{done.evals > 0 ? ` and ${n(done.evals, 'eval', 'evals')}` : ''} to {done.path}.
    </p>
  {/if}
  <div class="buttons">
    <span class="spacer"></span>
    <button onclick={() => dialog.close()}>{done ? 'Close' : 'Cancel'}</button>
    <button class="primary" onclick={exportNow} disabled={busy || nothing}
      >{busy ? 'Exporting…' : nativeFiles ? 'Export…' : 'Export'}</button
    >
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
  fieldset {
    border: none;
    padding: 0;
    margin: 0 0 12px;
    display: flex;
    flex-direction: column;
    gap: 4px;
  }
  legend {
    font-weight: 600;
    margin-bottom: 4px;
  }
  .engines {
    display: flex;
    flex-direction: column;
    gap: 2px;
    margin: 4px 0 0 22px;
  }
  .muted {
    color: var(--muted);
  }
  .field {
    display: flex;
    flex-direction: column;
    gap: 2px;
    margin-bottom: 12px;
    font-size: 12px;
    color: var(--muted);
  }
  .field input {
    font-size: 14px;
  }
  .done {
    word-break: break-all;
  }
  .buttons {
    display: flex;
    gap: 8px;
    margin-top: 6px;
  }
  .spacer {
    flex: 1;
  }
</style>
