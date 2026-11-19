<script lang="ts">
  // The board's explorer: the game database's games reaching the shown
  // position, and the moves played next in them with how each went
  // (gold's score beside what the ratings predicted). A move plays on the
  // board as a variation; Games lists the games, each opening on a board
  // of its own at the position. Folded away until opened, and off while
  // the user plays an online game (as analysis is).
  import { untrack } from 'svelte';
  import { api, errorMessage } from './api';
  import type { ExplorerMoveView } from './bindings/ExplorerMoveView';
  import type { ExplorerView } from './bindings/ExplorerView';
  import type { LibraryGameView } from './bindings/LibraryGameView';
  import type { ResultsView } from './bindings/ResultsView';
  import type { SessionView } from './bindings/SessionView';
  import { pref, savePref } from './gameroom/store.svelte';
  import { EMPTY_FORM, libraryMeta, percent, toFilter } from './library';
  import { openBoard } from './windows';

  interface Props {
    view: SessionView;
    /** Runs a command, showing its error if it fails. */
    run: (p: Promise<unknown>) => Promise<boolean>;
  }
  let { view, run }: Props = $props();

  const MOVES = 30;
  /** Moves shown until More moves. */
  const FEW = 10;
  const GAMES = 20;

  let open = $state(pref('board.explorerOpen') === '1');
  $effect(() => savePref('board.explorerOpen', open ? '1' : '0'));
  /** Only this player's games (any name they've had). */
  let player = $state(pref('board.explorerPlayer') ?? '');
  $effect(() => savePref('board.explorerPlayer', player));

  let tree = $state<ExplorerView | null>(null);
  let loading = $state(false);
  let error = $state<string | null>(null);
  let showGames = $state(false);
  let allMoves = $state(false);
  let games = $state<LibraryGameView[]>([]);
  let gamesTotal = $state(0);
  let gamesLoading = $state(false);
  let opening = $state<number | null>(null);
  let generation = 0;

  const allowed = $derived(view.analysisAllowed);
  const filter = $derived(toFilter({ ...EMPTY_FORM, player }));
  /** The explorer is behind the board while it's for another move. */
  const stale = $derived(tree != null && tree.node !== view.cursor);

  async function load() {
    const g = ++generation;
    loading = true;
    error = null;
    try {
      const t = await api.exploreLibrary(filter, MOVES);
      if (g !== generation) return;
      tree = t;
      games = [];
      gamesTotal = t.games.games;
      if (showGames) void loadGames();
    } catch (e) {
      if (g === generation) error = errorMessage(e);
    } finally {
      if (g === generation) loading = false;
    }
  }

  // Follows the shown position (a moment after it stops changing, so
  // stepping through a game doesn't scan at every move) and the player.
  let timer: ReturnType<typeof setTimeout> | undefined;
  $effect(() => {
    void view.cursor;
    void view.position.short;
    void JSON.stringify(filter);
    if (!open || !allowed) return;
    clearTimeout(timer);
    timer = setTimeout(() => untrack(() => void load()), 200);
    return () => clearTimeout(timer);
  });

  async function loadGames(more = false) {
    const t = tree;
    if (!t) return;
    const g = generation;
    gamesLoading = true;
    try {
      const page = await api.libraryGames(
        { ...filter, position: t.position, positionMatch: t.positionMatch, exclude: t.exclude },
        more ? games.length : 0,
        GAMES,
      );
      if (g !== generation) return;
      games = more ? [...games, ...page.games] : page.games;
      gamesTotal = page.total;
    } catch (e) {
      error = errorMessage(e);
    } finally {
      gamesLoading = false;
    }
  }

  function toggleGames() {
    showGames = !showGames;
    if (showGames && games.length === 0) void loadGames();
  }

  async function openGame(g: LibraryGameView) {
    opening = g.id;
    try {
      await openBoard((a) => a.openLibraryGame(g.id, g.positionPly), undefined, 'Opening the game…');
    } catch (e) {
      error = errorMessage(e);
    }
    opening = null;
  }

  function play(m: ExplorerMoveView) {
    if (!tree || stale) return;
    void run(api.addLine(tree.node, [m.notation]));
  }

  const mover = $derived(tree?.position.sideToMove ?? 'gold');
  /** Gold's score, with what the ratings predicted. */
  const scoreTitle = (r: ResultsView) =>
    r.ratedGames > 0
      ? `Gold won ${percent(r.goldWins, r.games)} of ${r.games.toLocaleString()} games; in the ${r.ratedGames.toLocaleString()} with both ratings ${percent(r.ratedGoldWins, r.ratedGames)}, where the ratings predicted ${percent(r.expectedGoldWins, r.ratedGames)}`
      : `Gold won ${percent(r.goldWins, r.games)} of ${r.games.toLocaleString()} games`;
  /** 1 when the side to move did 3 points better than its ratings
   * predicted after the move (over 20 rated games or more), -1 worse. */
  const better = (r: ResultsView) => {
    if (r.ratedGames < 20) return 0;
    const d = (r.ratedGoldWins - r.expectedGoldWins) / r.ratedGames;
    return Math.abs(d) < 0.03 ? 0 : Math.sign(d) * (mover === 'gold' ? 1 : -1);
  };
  const share = (m: ExplorerMoveView) =>
    tree && tree.games.games > 0 ? `${Math.round((100 * m.tally.games) / tree.games.games)}%` : '';
  /** The moves are setups: the side to move has no pieces yet. */
  const setupDue = $derived(
    tree != null && !(mover === 'gold' ? /[A-Z]/ : /[a-z]/).test(tree.position.short.replace(/^\[|\]$/g, '')),
  );
</script>

<details class="explorer" bind:open>
  <summary>Explorer</summary>
  {#if !allowed}
    <p class="note">The game database isn't searched during an online game you play.</p>
  {:else}
    <div class="body" class:stale aria-busy={loading}>
      <label class="player">
        Player
        <input
          aria-label="Explorer player"
          bind:value={player}
          placeholder="Any"
          autocomplete="off"
          spellcheck="false"
        />
      </label>
      {#if error}<p class="error" role="alert">{error}</p>{/if}
      {#if tree && !tree.available}
        <p class="note">There's no game database: it couldn't be opened.</p>
      {:else if tree}
        <p class="summary" aria-live="polite" title={tree.games.games > 0 ? scoreTitle(tree.games) : undefined}>
          {#if tree.games.games === 0}
            No {tree.exclude != null ? 'other ' : ''}game reaches this position{player.trim()
              ? ` with ${player.trim()}`
              : ''}.
          {:else}
            {tree.games.games.toLocaleString()}
            {tree.games.games === 1 ? 'game' : 'games'} · gold {percent(tree.games.goldWins, tree.games.games)}
            {#if tree.games.ratedGames > 0}(expected {percent(tree.games.expectedGoldWins, tree.games.ratedGames)}){/if}
            {#if tree.games.ended > 0}· {tree.games.ended.toLocaleString()} ended here{/if}
          {/if}
        </p>
        {#if tree.moves.length > 0}
          <table aria-label="Moves played next">
            <thead>
              <tr>
                <th scope="col">Move</th>
                <th scope="col" class="num">Games</th>
                <th scope="col" class="num" title="Gold's wins">Gold</th>
                <th scope="col" class="num">Last</th>
              </tr>
            </thead>
            <tbody>
              {#each allMoves ? tree.moves : tree.moves.slice(0, FEW) as m (m.notation)}
                <tr>
                  <td class="move">
                    <button
                      class="bare"
                      onclick={() => play(m)}
                      disabled={stale}
                      title={setupDue ? `Play this setup: ${m.notation}` : 'Play this move on the board'}
                      aria-label="Play {m.notation}">{m.notation}</button
                    >
                  </td>
                  <td class="num">{m.tally.games.toLocaleString()} <span class="share">{share(m)}</span></td>
                  <td
                    class="num"
                    class:up={better(m.tally) > 0}
                    class:down={better(m.tally) < 0}
                    title={scoreTitle(m.tally) +
                      (m.tally.rating != null ? `; the mover's rating ${Math.round(m.tally.rating)} on average` : '')}
                    >{percent(m.tally.goldWins, m.tally.games)}</td
                  >
                  <td class="num muted">{m.tally.lastDate.slice(0, 4)}</td>
                </tr>
              {/each}
            </tbody>
          </table>
          {#if !allMoves && tree.moves.length > FEW}
            <button class="more" onclick={() => (allMoves = true)}>More moves</button>
          {:else if tree.distinct > tree.moves.length}
            <p class="note">
              And {(tree.distinct - tree.moves.length).toLocaleString()} more moves, each played less.
            </p>
          {/if}
        {/if}
        {#if tree.games.games > 0}
          <button class="games-toggle" onclick={toggleGames} aria-expanded={showGames}>
            {showGames ? 'Hide games' : `Games (${tree.games.games.toLocaleString()})`}
          </button>
          {#if showGames}
            <ul class="games" aria-label="Games reaching this position">
              {#each games as g (g.id)}
                <li>
                  <button class="bare" onclick={() => openGame(g)} disabled={opening != null}>
                    <span class="names"
                      >{g.gold ?? '?'}{g.goldRating ? ` (${g.goldRating})` : ''} – {g.silver ?? '?'}{g.silverRating
                        ? ` (${g.silverRating})`
                        : ''}</span
                    >
                    <span class="meta">{opening === g.id ? 'Opening…' : libraryMeta(g)}</span>
                  </button>
                </li>
              {/each}
            </ul>
            {#if games.length < gamesTotal}
              <button class="more" onclick={() => loadGames(true)} disabled={gamesLoading}>
                {gamesLoading ? 'Loading…' : `More (${(gamesTotal - games.length).toLocaleString()} left)`}
              </button>
            {/if}
          {/if}
        {/if}
      {:else if loading}
        <p class="note">Searching the games…</p>
      {/if}
    </div>
  {/if}
</details>

<style>
  .explorer {
    margin-top: 6px;
    border-top: 1px solid var(--border);
    padding-top: 4px;
  }
  summary {
    cursor: pointer;
    font-weight: 600;
    font-size: 13px;
  }
  .body {
    margin-top: 6px;
    font-size: 12px;
  }
  .body.stale {
    opacity: 0.6;
  }
  .player {
    display: flex;
    align-items: center;
    gap: 6px;
  }
  .player input {
    flex: 1;
    min-width: 0;
  }
  .summary,
  .note {
    margin: 6px 0;
    color: var(--muted);
  }
  table {
    width: 100%;
    border-collapse: collapse;
  }
  th,
  td {
    padding: 2px 4px;
    text-align: left;
    border-bottom: 1px solid var(--border);
  }
  thead th {
    color: var(--muted);
    font-weight: 600;
  }
  .num {
    text-align: right;
    font-variant-numeric: tabular-nums;
    white-space: nowrap;
  }
  .move {
    max-width: 0;
    width: 100%;
  }
  .move button {
    display: block;
    max-width: 100%;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
    text-align: left;
    padding: 1px 2px;
    color: var(--accent);
    font-family: var(--mono, monospace);
  }
  tbody tr:hover > * {
    background: var(--hover);
  }
  .bare {
    background: none;
    border: none;
  }
  .share,
  .muted {
    color: var(--muted);
  }
  .share {
    font-size: 10px;
  }
  .up {
    color: var(--clock-ok);
    font-weight: 600;
  }
  .down {
    color: var(--warn);
    font-weight: 600;
  }
  .games-toggle,
  .more {
    margin-top: 6px;
  }
  .games {
    list-style: none;
    margin: 6px 0 0;
    padding: 0;
  }
  .games li button {
    display: flex;
    flex-direction: column;
    width: 100%;
    text-align: left;
    padding: 3px 2px;
    border-bottom: 1px solid var(--border);
  }
  .games li button:hover {
    background: var(--hover);
  }
  .names {
    font-weight: 600;
  }
  .meta {
    color: var(--muted);
    font-size: 11px;
  }
</style>
