<script lang="ts">
  // The games being played on arimaa.com, a line each above the chat
  // room's log, from the lobby's lists (no requests of their own). Board
  // shows one small beside the log (`livePreview`), Watch on a board.
  import type { LiveGameView } from '../bindings/LiveGameView';
  import Icon from '../Icon.svelte';
  import { gameMeta, liveMoveLabel } from './format';
  import { livePreview } from './livePreview.svelte';
  import Matchup from './Matchup.svelte';
  import { gameroom } from './store.svelte';

  const live = $derived(gameroom.games?.live ?? []);

  /** The move being played, from the game shown beside the chat when
   * it's this one (newer than the list). */
  function moveLabel(g: LiveGameView): string | null {
    const view = livePreview.gid === g.gid ? livePreview.view : null;
    return liveMoveLabel(g, view && !view.result ? view.moves.length : undefined);
  }
</script>

{#if gameroom.games}
  <section class="live" aria-label="Live games on arimaa.com">
    {#if live.length === 0}
      <p class="none">No games being played on arimaa.com.</p>
    {:else}
      <ul>
        {#each live as g (g.gid)}
          {@const shown = livePreview.gid === g.gid}
          {@const label = moveLabel(g)}
          <li class:shown>
            <span class="matchup">
              <Matchup
                gold={g.gold}
                silver={g.silver}
                goldRating={g.goldRating}
                silverRating={g.silverRating}
                goldId={g.goldId}
                silverId={g.silverId}
              />
            </span>
            <span class="meta">{[label, gameMeta(g)].filter(Boolean).join(' · ')}</span>
            <button
              class="bare toggle"
              aria-pressed={shown}
              onclick={() => livePreview.toggle(g.gid)}
              title={shown ? 'Hide the board' : 'Show the game on a small board beside the chat'}
              aria-label={shown ? `Hide game ${g.gid}'s board` : `Show game ${g.gid} beside the chat`}
              ><Icon name="board" size={16} /></button
            >
            <button onclick={() => gameroom.open(g.gid)} disabled={gameroom.busy} title="Watch game {g.gid} on a board"
              >Watch</button
            >
          </li>
        {/each}
      </ul>
    {/if}
  </section>
{/if}

<style>
  .live {
    font-size: 13px;
  }
  .none {
    margin: 0;
    color: var(--muted);
  }
  ul {
    display: flex;
    flex-direction: column;
    gap: 2px;
    margin: 0;
    padding: 0;
    list-style: none;
  }
  li {
    display: flex;
    align-items: center;
    gap: 8px;
    padding: 2px 4px 2px 8px;
    border: 1px solid var(--border);
    border-radius: 8px;
  }
  li.shown {
    background: var(--hover);
  }
  .matchup {
    display: flex;
    align-items: baseline;
    gap: 4px;
    min-width: 0;
    white-space: nowrap;
    overflow: hidden;
  }
  .meta {
    flex: 1;
    min-width: 0;
    color: var(--muted);
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
  }
  .toggle {
    display: flex;
    padding: 4px;
    border-radius: 6px;
    color: var(--muted);
  }
  .toggle[aria-pressed='true'] {
    color: var(--accent);
  }
  button:not(.bare) {
    padding: 2px 10px;
    font-size: 12px;
  }
</style>
