<script lang="ts">
  // The lobby's Puzzles panel: arimaa.com's puzzle list (fetched once by
  // the backend and kept), by its headings, with the ones solved here
  // ticked. A puzzle opens on a board of its own. Listed under Home in
  // the sidebar once opened.
  import { api, errorMessage } from './api';
  import type { PuzzleGroupView } from './bindings/PuzzleGroupView';
  import Icon from './Icon.svelte';
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
  let filter = $state('');
  const solved = solvedPuzzles();

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
  }

  const shown = $derived.by(() => {
    const words = filter.trim().toLowerCase();
    return (groups ?? [])
      .map((g) => ({
        ...g,
        puzzles: g.puzzles.filter(
          (p) => !words || (p.title ?? '').toLowerCase().includes(words) || p.id.toLowerCase().includes(words),
        ),
      }))
      .filter((g) => g.puzzles.length > 0);
  });
</script>

<div class="screen">
  <header>
    <h1>Puzzles</h1>
    <span class="spacer"></span>
    <input bind:value={filter} placeholder="Filter" aria-label="Filter puzzles" autocomplete="off" />
    <button class="icon" onclick={() => load(true)} title="Fetch the list again" aria-label="Refresh"
      ><Icon name="refresh" size={16} /></button
    >
    <button onclick={onClose} title="Close this panel">Close</button>
  </header>
  <p class="hint intro">
    From arimaa.com's puzzle pages. Each opens on a board of its own, where your moves are checked against its answer.
  </p>
  {#if error}<p class="error" role="alert">{error}</p>{/if}
  {#if groups == null && !error}
    <p class="empty">Fetching the puzzles from arimaa.com…</p>
  {:else if groups && shown.length === 0}
    <p class="empty">No puzzle matches.</p>
  {:else}
    <div class="groups">
      {#each shown as g (g.name)}
        {@const done = g.puzzles.filter((p) => solved.has(p.id)).length}
        <section class="card" aria-labelledby="pz-{g.name}">
          <div class="card-head">
            <h2 id="pz-{g.name}">{g.name || 'Puzzles'}</h2>
            <span class="count">{done > 0 ? `${done} of ${g.puzzles.length} solved` : g.puzzles.length}</span>
          </div>
          <ul class="list" aria-label={g.name || 'Puzzles'}>
            {#each g.puzzles as p (p.id)}
              <li>
                <span class="row-text">
                  <span class="row-line">
                    {#if solved.has(p.id)}<span class="done" title="Solved">✓</span>{/if}
                    <span class="name">{p.title || p.id}</span>
                  </span>
                </span>
                <span class="meta">{p.id}</span>
                <button onclick={() => open(p.id)} disabled={opening != null} aria-label="Open puzzle {p.title || p.id}"
                  >{opening === p.id ? 'Opening…' : 'Open'}</button
                >
              </li>
            {/each}
          </ul>
        </section>
      {/each}
    </div>
  {/if}
</div>

<style>
  header {
    display: flex;
    align-items: center;
    gap: 8px;
    margin-bottom: 6px;
    min-height: 30px;
  }
  h1 {
    margin: 0;
    font-size: 20px;
  }
  header input {
    width: 12em;
  }
  header button.icon {
    padding: 4px 6px;
  }
  .intro {
    margin-bottom: 14px;
  }
  .groups {
    display: grid;
    grid-template-columns: repeat(auto-fill, minmax(340px, 1fr));
    gap: 16px;
    align-items: start;
  }
  .done {
    color: var(--clock-ok);
  }
  .meta {
    flex: none;
  }
  .error {
    margin-bottom: 10px;
  }
</style>
