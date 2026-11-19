<script lang="ts">
  // The Games panel's setup explorer: the games the filter lets through,
  // by gold's setup, then silver's replies to one, or by silver's setup
  // alone. A setup and its mirror image count as one (the backend writes
  // it with the elephant on files e–h), and a reply keeps its place
  // relative to gold's pieces. Each row shows how gold did and what the
  // ratings predicted; Games lists the games with that setup. A reply
  // opens the moves played after the pair, and each move the moves after
  // it, down the game (a game's mirror image counting as the game, its
  // moves seen as the setups are shown).
  import { api, errorMessage } from './api';
  import type { ExplorerView } from './bindings/ExplorerView';
  import type { LibraryFilter } from './bindings/LibraryFilter';
  import type { PositionSpec } from './bindings/PositionSpec';
  import type { ResultsView } from './bindings/ResultsView';
  import type { SetupStatView } from './bindings/SetupStatView';
  import { percent, setupWords } from './library';
  import SetupDiagram from './SetupDiagram.svelte';
  import { pref, savePref } from './gameroom/store.svelte';

  interface Props {
    filter: LibraryFilter;
    /** Shows the games with these setups in the list. */
    onGames: (gold: string | null, silver: string | null) => void;
    /** Shows the games reaching a position (or its mirror image). */
    onPosition: (position: PositionSpec) => void;
  }
  let { filter, onGames, onPosition }: Props = $props();

  type Level = 'gold' | 'silver';
  interface Place {
    level: Level;
    /** The gold setup whose replies are shown. */
    gold: string | null;
    /** Silver's reply whose moves are shown, and the moves after it
     * (notation, seen as the setups are shown). */
    reply: string | null;
    moves: string[];
  }

  function readPlace(): Place {
    try {
      const p = JSON.parse(pref('lobby.setupExplorer') || '{}');
      const gold = typeof p.gold === 'string' ? p.gold : null;
      const reply = gold && typeof p.reply === 'string' ? p.reply : null;
      const moves = reply && Array.isArray(p.moves) ? p.moves.filter((m: unknown) => typeof m === 'string') : [];
      return { level: p.level === 'silver' ? 'silver' : 'gold', gold, reply, moves };
    } catch {
      return { level: 'gold', gold: null, reply: null, moves: [] };
    }
  }

  const FIRST = 100;
  let place = $state<Place>(readPlace());
  let limit = $state(FIRST);
  let rows = $state<SetupStatView[]>([]);
  let games = $state<number | null>(null);
  let setups = $state(0);
  let available = $state(true);
  let loading = $state(false);
  let error = $state<string | null>(null);
  let generation = 0;

  /** The moves after a pair of setups. */
  let line = $state<ExplorerView | null>(null);
  const inLine = $derived(place.level === 'gold' && place.gold != null && place.reply != null);
  const replies = $derived(place.level === 'gold' && place.gold != null && !inLine);
  /** The color of the setups listed. */
  const listed = $derived(place.level === 'gold' && !replies ? 'gold' : 'silver');

  async function load() {
    const g = ++generation;
    loading = true;
    error = null;
    try {
      if (inLine) {
        const t = await api.libraryLine(filter, place.gold!, place.reply!, place.moves, limit);
        if (g !== generation) return;
        line = t;
        available = t.available;
        return;
      }
      line = null;
      const level = replies ? 'replies' : place.level;
      const stats = await api.librarySetups(filter, level, place.gold, limit);
      if (g !== generation) return;
      rows = stats.rows;
      games = stats.games;
      setups = stats.setups;
      available = stats.available;
    } catch (e) {
      if (g === generation) error = errorMessage(e);
    } finally {
      if (g === generation) loading = false;
    }
  }

  $effect(() => {
    savePref('lobby.setupExplorer', JSON.stringify(place));
    void JSON.stringify(filter);
    void limit;
    void load();
  });

  function go(p: Partial<Place> & { level: Level }) {
    place = { gold: null, reply: null, moves: [], ...p };
    limit = FIRST;
  }

  const MOVES_FIRST = 30;
  function goLine(moves: string[]) {
    place = { ...place, moves };
    limit = MOVES_FIRST;
  }

  /** "2g", "2s", … for the move after `n` moves past the setups. */
  const label = (n: number) => `${2 + Math.floor(n / 2)}${n % 2 === 0 ? 'g' : 's'}`;
  /** The side to move after the line's moves. */
  const lineMover = $derived(place.moves.length % 2 === 0 ? 'gold' : 'silver');

  let findingGames = $state<string | null>(null);
  /** The games reaching the position after `moves`. */
  async function lineGames(moves: string[]) {
    findingGames = moves.join('|');
    try {
      const t =
        moves.length === place.moves.length && line
          ? line
          : await api.libraryLine(filter, place.gold!, place.reply!, moves, 0);
      onPosition(t.position);
    } catch (e) {
      error = errorMessage(e);
    }
    findingGames = null;
  }

  const lineTitle = (r: ResultsView) =>
    r.ratedGames > 0
      ? `In the ${r.ratedGames.toLocaleString()} games with both ratings gold won ${percent(r.ratedGoldWins, r.ratedGames)}; the ratings predicted ${percent(r.expectedGoldWins, r.ratedGames)}`
      : 'No games with both ratings';
  const lineBetter = (r: ResultsView) => {
    if (r.ratedGames < 20) return 0;
    const d = (r.ratedGoldWins - r.expectedGoldWins) / r.ratedGames;
    return Math.abs(d) < 0.03 ? 0 : Math.sign(d) * (lineMover === 'gold' ? 1 : -1);
  };

  function games_of(row: SetupStatView) {
    if (replies) onGames(place.gold, row.setup);
    else if (place.level === 'gold') onGames(row.setup, null);
    else onGames(null, row.setup);
  }

  const share = (row: SetupStatView) => (games ? `${((100 * row.games) / games).toFixed(1)}%` : '');
  const expectedTitle = (row: SetupStatView) =>
    row.ratedGames > 0
      ? `In the ${row.ratedGames.toLocaleString()} games with both ratings gold won ${percent(row.ratedGoldWins, row.ratedGames)}; the ratings predicted ${percent(row.expectedGoldWins, row.ratedGames)}`
      : 'No games with both ratings';
  const better = (row: SetupStatView) => {
    if (row.ratedGames < 20) return 0;
    const d = (row.ratedGoldWins - row.expectedGoldWins) / row.ratedGames;
    // Green when the side playing the setup did better than predicted.
    return Math.abs(d) < 0.03 ? 0 : Math.sign(d) * (listed === 'gold' ? 1 : -1);
  };
</script>

<div class="explorer">
  <div class="levels">
    <div class="tabs" role="tablist" aria-label="Setups">
      <button
        role="tab"
        aria-selected={place.level === 'gold'}
        class:on={place.level === 'gold'}
        onclick={() => go({ level: 'gold' })}>Gold setups</button
      >
      <button
        role="tab"
        aria-selected={place.level === 'silver'}
        class:on={place.level === 'silver'}
        onclick={() => go({ level: 'silver' })}>Silver setups</button
      >
    </div>
    <span class="count" aria-live="polite">
      {#if games != null && !inLine}
        {games.toLocaleString()} games, {setups.toLocaleString()} {setups === 1 ? 'setup' : 'setups'}
      {/if}
    </span>
  </div>

  {#if replies && place.gold}
    <div class="crumb">
      <button class="bare back" onclick={() => go({ level: 'gold' })}>← Gold setups</button>
      <span class="against">Silver's replies to</span>
      <SetupDiagram setup={place.gold} color="gold" size={30} />
      <button onclick={() => onGames(place.gold, null)}>Games</button>
    </div>
  {:else if inLine && place.gold && place.reply}
    <nav class="crumb line-crumb" aria-label="The line explored">
      <button class="bare back" onclick={() => go({ level: 'gold' })}>← Gold setups</button>
      <button
        class="bare pick"
        onclick={() => go({ level: 'gold', gold: place.gold })}
        title="Silver's replies to this setup"
        aria-label="Silver's replies to gold {setupWords(place.gold, 'gold')}"
        ><SetupDiagram setup={place.gold} color="gold" size={20} /></button
      >
      <button
        class="bare pick"
        onclick={() => goLine([])}
        title="The moves after both setups"
        aria-label="The moves after silver {setupWords(place.reply, 'silver')}"
        aria-current={place.moves.length === 0 ? 'step' : undefined}
        ><SetupDiagram setup={place.reply} color="silver" size={20} /></button
      >
      {#each place.moves as m, i (i)}
        <button
          class="bare move"
          class:current={i === place.moves.length - 1}
          onclick={() => goLine(place.moves.slice(0, i + 1))}
          aria-current={i === place.moves.length - 1 ? 'step' : undefined}
          title="The moves after this one"><span class="label">{label(i)}</span> {m}</button
        >
      {/each}
      <button onclick={() => lineGames(place.moves)} disabled={findingGames != null}>Games</button>
    </nav>
  {/if}

  <p class="hint">
    {#if inLine}
      The moves played next, {label(place.moves.length)}; choose one to go on. A game played the other way round counts
      too, its moves seen as these setups stand.
    {:else if replies}
      Silver's setups against this one, placed as they stood against it. Choose one to see the moves after it.
    {:else}
      A setup and its mirror image count as one, shown with the elephant on the right.
      {#if place.level === 'gold'}Choose one to see silver's replies.{/if}
    {/if}
    Gold's score is beside the score the players' ratings predicted.
  </p>

  {#if error}<p class="error" role="alert">{error}</p>{/if}
  {#if !available}
    <p class="empty">There's no game database: it couldn't be opened.</p>
  {:else if inLine}
    {#if line}
      <p class="count line-count" aria-live="polite">
        {line.games.games.toLocaleString()}
        {line.games.games === 1 ? 'game' : 'games'} · gold {percent(line.games.goldWins, line.games.games)}
        {#if line.games.ratedGames > 0}(expected {percent(line.games.expectedGoldWins, line.games.ratedGames)}){/if}
        {#if line.games.ended > 0}· {line.games.ended.toLocaleString()} ended here{/if}
      </p>
      {#if line.moves.length > 0}
        <table class="setups" aria-label="Moves played at {label(place.moves.length)}">
          <thead>
            <tr>
              <th scope="col">Move {label(place.moves.length)}</th>
              <th scope="col" class="num">Games</th>
              <th scope="col" class="num" title="Gold's wins">Gold won</th>
              <th scope="col" class="num" title="Gold's expected score from the ratings">Expected</th>
              <th scope="col" class="num" title="The average rating of the side making the move">Rating</th>
              <th scope="col" class="num">Last played</th>
              <th scope="col"><span class="sr">Actions</span></th>
            </tr>
          </thead>
          <tbody>
            {#each line.moves as m (m.notation)}
              <tr>
                <td>
                  <button
                    class="bare pick notation"
                    onclick={() => goLine([...place.moves, m.notation])}
                    title="The moves after this one">{m.notation}</button
                  >
                </td>
                <td class="num"
                  >{m.tally.games.toLocaleString()}
                  <span class="share"
                    >{line.games.games ? `${((100 * m.tally.games) / line.games.games).toFixed(1)}%` : ''}</span
                  ></td
                >
                <td
                  class="num"
                  class:up={lineBetter(m.tally) > 0}
                  class:down={lineBetter(m.tally) < 0}
                  title={lineTitle(m.tally)}>{percent(m.tally.goldWins, m.tally.games)}</td
                >
                <td class="num muted" title={lineTitle(m.tally)}
                  >{percent(m.tally.expectedGoldWins, m.tally.ratedGames)}</td
                >
                <td class="num muted">{m.tally.rating == null ? '–' : Math.round(m.tally.rating)}</td>
                <td class="num muted">{m.tally.lastDate}</td>
                <td class="actions">
                  <button
                    class="bare"
                    disabled={findingGames != null}
                    onclick={() => lineGames([...place.moves, m.notation])}>Games</button
                  >
                </td>
              </tr>
            {/each}
          </tbody>
        </table>
        {#if line.moves.length < line.distinct && limit < 500}
          <button class="more" onclick={() => (limit = Math.min(500, limit * 4))} disabled={loading}>
            {loading ? 'Loading…' : `Show more (${(line.distinct - line.moves.length).toLocaleString()} moves left)`}
          </button>
        {/if}
      {:else if !loading}
        <p class="empty">No game went on from here.</p>
      {/if}
    {:else if loading}
      <p class="empty">Searching the games…</p>
    {/if}
  {:else if games === 0 && !loading}
    <p class="empty">No games with both setups match.</p>
  {:else}
    <table
      class="setups"
      aria-label={replies ? "Silver's replies" : place.level === 'gold' ? 'Gold setups' : 'Silver setups'}
    >
      <thead>
        <tr>
          <th scope="col">Setup</th>
          <th scope="col" class="num">Games</th>
          <th scope="col" class="num" title="Gold's wins">Gold won</th>
          <th scope="col" class="num" title="Gold's expected score from the ratings">Expected</th>
          <th scope="col" class="num" title="The average rating of the side playing the setup">Rating</th>
          <th scope="col" class="num">Last played</th>
          <th scope="col"><span class="sr">Actions</span></th>
        </tr>
      </thead>
      <tbody>
        {#each rows as row (row.setup)}
          <tr>
            <td>
              {#if place.level === 'gold' && !replies}
                <button
                  class="bare pick"
                  onclick={() => go({ level: 'gold', gold: row.setup })}
                  title="Silver's replies to this setup"
                  aria-label="Silver's replies to gold {setupWords(row.setup, 'gold')}"
                  ><SetupDiagram setup={row.setup} color="gold" /></button
                >
              {:else if replies}
                <button
                  class="bare pick"
                  onclick={() => go({ level: 'gold', gold: place.gold, reply: row.setup })}
                  title="The moves after this reply"
                  aria-label="Moves after silver {setupWords(row.setup, 'silver')}"
                  ><SetupDiagram setup={row.setup} color="silver" /></button
                >
              {:else}
                <button
                  class="bare pick"
                  onclick={() => games_of(row)}
                  title="The games with this setup"
                  aria-label="Games with {listed} {setupWords(row.setup, listed)}"
                  ><SetupDiagram setup={row.setup} color={listed} /></button
                >
              {/if}
            </td>
            <td class="num">{row.games.toLocaleString()} <span class="share">{share(row)}</span></td>
            <td class="num" class:up={better(row) > 0} class:down={better(row) < 0} title={expectedTitle(row)}
              >{percent(row.goldWins, row.games)}</td
            >
            <td class="num muted" title={expectedTitle(row)}>{percent(row.expectedGoldWins, row.ratedGames)}</td>
            <td class="num muted">{row.rating == null ? '–' : Math.round(row.rating)}</td>
            <td class="num muted">{row.lastDate}</td>
            <td class="actions">
              {#if place.level === 'gold' && !replies}
                <button class="bare" onclick={() => go({ level: 'gold', gold: row.setup })}>Replies</button>
              {:else if replies}
                <button class="bare" onclick={() => go({ level: 'gold', gold: place.gold, reply: row.setup })}
                  >Moves</button
                >
              {/if}
              <button class="bare" onclick={() => games_of(row)}>Games</button>
            </td>
          </tr>
        {/each}
      </tbody>
    </table>
    {#if rows.length < setups && limit < 1000}
      <button class="more" onclick={() => (limit = Math.min(1000, limit * 4))} disabled={loading}>
        {loading ? 'Loading…' : `Show more (${(setups - rows.length).toLocaleString()} setups left)`}
      </button>
    {:else if loading && rows.length === 0}
      <p class="empty">Loading…</p>
    {/if}
  {/if}
</div>

<style>
  .levels {
    display: flex;
    align-items: baseline;
    gap: 12px;
    margin-bottom: 8px;
  }
  .tabs {
    display: flex;
    gap: 4px;
  }
  .tabs button {
    background: none;
    border: none;
    border-bottom: 2px solid transparent;
    border-radius: 0;
    padding: 4px 8px;
    font-weight: 600;
    color: var(--muted);
  }
  .tabs button.on {
    color: var(--text);
    border-bottom-color: var(--accent);
  }
  .count,
  .muted,
  .share {
    color: var(--muted);
  }
  .share {
    font-size: 11px;
    display: inline-block;
    min-width: 3.2em;
  }
  .crumb {
    display: flex;
    align-items: center;
    gap: 10px;
    flex-wrap: wrap;
    margin-bottom: 6px;
  }
  .line-crumb {
    gap: 4px 8px;
  }
  .line-crumb .move {
    font-family: var(--mono, monospace);
    font-size: 12px;
    padding: 2px 4px;
    color: var(--accent);
  }
  .line-crumb .move.current {
    color: var(--text);
    font-weight: 600;
  }
  .line-crumb .label {
    color: var(--muted);
  }
  .notation {
    font-family: var(--mono, monospace);
    color: var(--accent);
    text-align: left;
  }
  .line-count {
    margin: 0 0 6px;
  }
  .against {
    font-weight: 600;
  }
  .hint {
    margin: 0 0 10px;
  }
  .setups {
    width: 100%;
    border-collapse: collapse;
    font-size: 13px;
  }
  .setups th,
  .setups td {
    padding: 3px 6px;
    text-align: left;
    border-bottom: 1px solid var(--border);
    vertical-align: middle;
  }
  .setups thead th {
    font-size: 12px;
    color: var(--muted);
    font-weight: 600;
  }
  .setups .num {
    text-align: right;
    font-variant-numeric: tabular-nums;
    white-space: nowrap;
  }
  .setups tbody tr:hover > * {
    background: var(--hover);
  }
  .bare {
    background: none;
    border: none;
  }
  .back {
    padding: 2px 4px;
    color: var(--accent);
  }
  .pick {
    padding: 2px;
    display: block;
  }
  .up {
    color: var(--clock-ok);
    font-weight: 600;
  }
  .down {
    color: var(--warn);
    font-weight: 600;
  }
  .actions {
    white-space: nowrap;
    text-align: right;
  }
  .actions button {
    padding: 2px 6px;
    color: var(--accent);
  }
  .sr {
    position: absolute;
    width: 1px;
    height: 1px;
    overflow: hidden;
    clip: rect(0 0 0 0);
  }
  .more {
    margin-top: 10px;
  }
  .error {
    margin-bottom: 10px;
  }
</style>
