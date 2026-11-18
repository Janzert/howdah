<script lang="ts">
  // The Games panel's setup explorer: the games the filter lets through,
  // by gold's setup, then silver's replies to one, or by silver's setup
  // alone. A setup and its mirror image count as one (the backend writes
  // it with the elephant on files e–h), and a reply keeps its place
  // relative to gold's pieces. Each row shows how gold did and what the
  // ratings predicted; Games lists the games with that setup.
  import { api, errorMessage } from './api';
  import type { LibraryFilter } from './bindings/LibraryFilter';
  import type { SetupStatView } from './bindings/SetupStatView';
  import { percent, setupWords } from './library';
  import SetupDiagram from './SetupDiagram.svelte';
  import { pref, savePref } from './gameroom/store.svelte';

  interface Props {
    filter: LibraryFilter;
    /** Shows the games with these setups in the list. */
    onGames: (gold: string | null, silver: string | null) => void;
  }
  let { filter, onGames }: Props = $props();

  type Level = 'gold' | 'silver';
  interface Place {
    level: Level;
    /** The gold setup whose replies are shown. */
    gold: string | null;
  }

  function readPlace(): Place {
    try {
      const p = JSON.parse(pref('lobby.setupExplorer') || '{}');
      return { level: p.level === 'silver' ? 'silver' : 'gold', gold: typeof p.gold === 'string' ? p.gold : null };
    } catch {
      return { level: 'gold', gold: null };
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

  const replies = $derived(place.level === 'gold' && place.gold != null);
  /** The color of the setups listed. */
  const listed = $derived(place.level === 'gold' && !replies ? 'gold' : 'silver');

  async function load() {
    const g = ++generation;
    loading = true;
    error = null;
    try {
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

  function go(p: Place) {
    place = p;
    limit = FIRST;
  }

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
        onclick={() => go({ level: 'gold', gold: null })}>Gold setups</button
      >
      <button
        role="tab"
        aria-selected={place.level === 'silver'}
        class:on={place.level === 'silver'}
        onclick={() => go({ level: 'silver', gold: null })}>Silver setups</button
      >
    </div>
    <span class="count" aria-live="polite">
      {#if games != null}
        {games.toLocaleString()} games, {setups.toLocaleString()} {setups === 1 ? 'setup' : 'setups'}
      {/if}
    </span>
  </div>

  {#if replies && place.gold}
    <div class="crumb">
      <button class="bare back" onclick={() => go({ level: 'gold', gold: null })}>← Gold setups</button>
      <span class="against">Silver's replies to</span>
      <SetupDiagram setup={place.gold} color="gold" size={30} />
      <button onclick={() => onGames(place.gold, null)}>Games</button>
    </div>
  {/if}

  <p class="hint">
    {#if replies}
      Silver's setups against this one, placed as they stood against it.
    {:else}
      A setup and its mirror image count as one, shown with the elephant on the right.
      {#if place.level === 'gold'}Choose one to see silver's replies.{/if}
    {/if}
    Gold's score is beside the score the players' ratings predicted.
  </p>

  {#if error}<p class="error" role="alert">{error}</p>{/if}
  {#if !available}
    <p class="empty">There's no game database: it couldn't be opened.</p>
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
