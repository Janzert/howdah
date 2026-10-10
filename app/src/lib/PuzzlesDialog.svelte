<script lang="ts">
  // arimaa.com's puzzle list (fetched once by the backend and kept), by
  // its headings, with the ones solved here ticked. A puzzle opens in a
  // window of its own.
  import { api, errorMessage } from './api';
  import type { PuzzleGroupView } from './bindings/PuzzleGroupView';
  import { solvedPuzzles } from './puzzles';

  interface Props {
    /** Opens a puzzle; resolves to an error message, or null. */
    onOpen: (id: string) => Promise<string | null>;
    onClose: () => void;
  }
  let { onOpen, onClose }: Props = $props();

  let groups = $state<PuzzleGroupView[] | null>(null);
  let error = $state<string | null>(null);
  let opening = $state<string | null>(null);
  const solved = solvedPuzzles();
  let dialog: HTMLDialogElement;

  $effect(() => {
    dialog.showModal();
  });

  async function load(refresh = false) {
    error = null;
    try {
      groups = await api.puzzleList(refresh);
    } catch (e) {
      error = errorMessage(e);
    }
  }
  void load();

  async function open(id: string) {
    opening = id;
    error = await onOpen(id);
    opening = null;
    if (!error) dialog.close();
  }
</script>

<dialog bind:this={dialog} onclose={onClose} aria-labelledby="puzzles-title">
  <h2 id="puzzles-title">Puzzles</h2>
  <p class="note">From arimaa.com's puzzle pages. Solve one in its window: your moves are checked against its answer.</p>
  {#if error}<p class="error" role="alert">{error}</p>{/if}
  {#if groups == null && !error}
    <p>Loading…</p>
  {:else if groups}
    <div class="groups">
      {#each groups as g (g.name)}
        <section>
          <h3>{g.name || 'Puzzles'}</h3>
          <ul aria-label={g.name || 'Puzzles'}>
            {#each g.puzzles as p (p.id)}
              <li>
                <button class="link" disabled={opening != null} onclick={() => open(p.id)}>
                  {#if solved.has(p.id)}<span class="done" title="Solved">✓</span>{/if}
                  {p.title || p.id}
                </button>
                <span class="id">{p.id}</span>
              </li>
            {/each}
          </ul>
        </section>
      {/each}
    </div>
  {/if}
  <div class="buttons">
    <button onclick={() => load(true)} title="Fetch the list again">Refresh</button>
    <span class="spacer"></span>
    <button onclick={() => dialog.close()}>Close</button>
  </div>
</dialog>

<style>
  dialog {
    width: min(620px, 90vw);
    max-height: 85vh;
    border: 1px solid var(--border);
    border-radius: 8px;
    background: var(--panel);
    color: var(--text);
    padding: 16px;
  }
  dialog[open] {
    display: flex;
    flex-direction: column;
  }
  dialog::backdrop {
    background: rgba(0, 0, 0, 0.4);
  }
  h2 {
    margin: 0 0 4px;
    font-size: 16px;
  }
  h3 {
    margin: 10px 0 4px;
    font-size: 14px;
  }
  .note {
    margin: 0 0 8px;
    font-size: 12px;
    color: var(--muted);
  }
  .groups {
    overflow-y: auto;
    min-height: 0;
  }
  ul {
    margin: 0;
    padding: 0;
    list-style: none;
  }
  li {
    display: flex;
    align-items: baseline;
    gap: 6px;
  }
  .link {
    border: none;
    background: none;
    padding: 2px 0;
    color: var(--text);
    text-align: left;
    cursor: pointer;
  }
  .link:hover:not(:disabled) {
    text-decoration: underline;
  }
  .done {
    color: var(--clock-ok);
    margin-right: 4px;
  }
  .id {
    margin-left: auto;
    color: var(--muted);
    font-size: 12px;
  }
  .error {
    color: var(--warn);
    font-size: 13px;
  }
  .buttons {
    display: flex;
    gap: 6px;
    margin-top: 10px;
  }
  .spacer {
    flex: 1;
  }
</style>
