<script lang="ts">
  // Finished games: the gameroom's recently finished games (the lobby's
  // few at once, then the page of the last 100, shown a batch at a time), a player's
  // past games (search; an exact username goes straight to their games,
  // with "Older games" paging), and any game by id.
  import { finishedRows } from '../gameroom';
  import PastGameRow from './PastGameRow.svelte';
  import RecentGameRow from './RecentGameRow.svelte';
  import { gameroom } from './store.svelte';

  /** The id typed into "Open a game by id". */
  let gameId = $state('');
  const games = $derived(gameroom.games);
  const picked = $derived(gameroom.picked);
  const players = $derived(gameroom.players);
  const rows = $derived(finishedRows(games?.recent ?? [], gameroom.recentPage ?? []));

  /** How many rows of the recently finished list are shown. */
  const BATCH = 25;
  let shown = $state(BATCH);

  // Also reads the lists, so a new game can bring the page up to date.
  $effect(() => {
    if (games) void gameroom.loadRecentPage();
  });

  function search(e: SubmitEvent) {
    e.preventDefault();
    void gameroom.searchPlayers(gameroom.playerQuery);
  }

  function openById(e: SubmitEvent) {
    e.preventDefault();
    void gameroom.open(gameId);
  }
</script>

<div class="columns">
  <div class="column">
    <section class="card" aria-labelledby="gr-player-title">
      <div class="card-head"><h2 id="gr-player-title">A player's games</h2></div>
      <form class="fields" onsubmit={search}>
        <input
          id="gr-player"
          bind:value={gameroom.playerQuery}
          placeholder="Part of a username or name"
          aria-label="Player"
          autocomplete="off"
        />
        <button type="submit" disabled={gameroom.busy || !gameroom.playerQuery.trim()}>Search</button>
      </form>
      {#if picked}
        <div class="picked">
          Games of <strong>{picked.player.username}</strong>
          <span class="spacer"></span>
          {#if players && players.length > 1}
            <button class="link" onclick={() => (gameroom.picked = null)}>Back to the players found</button>
          {/if}
        </div>
        {#if picked.games.length === 0}
          <p class="empty">No finished games.</p>
        {:else}
          <ul class="list scroll" aria-label="Games of {picked.player.username}">
            {#each picked.games as g (g.gid)}<PastGameRow game={g} />{/each}
            {#if picked.next != null}
              <li class="more">
                <button class="subtle" onclick={() => gameroom.olderGames()} disabled={gameroom.busy}
                  >Older games</button
                >
              </li>
            {/if}
          </ul>
        {/if}
      {:else if players && players.length === 0}
        <p class="empty">No players match.</p>
      {:else if players}
        <ul class="list scroll found" aria-label="Players found">
          {#each players as p (p.id)}
            <li>
              <span class="row-text">
                <span class="row-line"><span class="name">{p.username}</span></span>
                {#if p.name}<span class="meta">{p.name}</span>{/if}
              </span>
              <button onclick={() => gameroom.pick(p)} disabled={gameroom.busy} aria-label="Games of {p.username}"
                >Games</button
              >
            </li>
          {/each}
        </ul>
      {/if}
    </section>

    <section class="card" aria-labelledby="gr-id-title">
      <div class="card-head"><h2 id="gr-id-title">Open a game by id</h2></div>
      <form class="fields" onsubmit={openById}>
        <input
          id="gr-game-id"
          bind:value={gameId}
          inputmode="numeric"
          placeholder="e.g. 671438"
          aria-label="Game id"
          autocomplete="off"
        />
        <button type="submit" disabled={gameroom.busy || !/^\s*\d+\s*$/.test(gameId)}>Open</button>
      </form>
      <p class="hint">
        A finished game's id (as in the game lists and the archive) loads the whole game; a live game's gameroom id
        watches it.
      </p>
    </section>
  </div>

  <section class="card" aria-labelledby="gr-finished-title">
    <div class="card-head">
      <h2 id="gr-finished-title">
        Recently finished {#if games}<span class="count">{rows.length}</span>{/if}
      </h2>
    </div>
    {#if !games}
      <p class="empty">…</p>
    {:else if rows.length === 0 && !gameroom.recentPageLoading}
      <p class="empty">The gameroom lists no finished games.</p>
    {:else}
      <ul class="list scroll" aria-label="Recently finished games">
        {#each rows.slice(0, shown) as row ('recent' in row ? row.recent.gid : row.past.gid)}
          {#if 'recent' in row}<RecentGameRow game={row.recent} />{:else}<PastGameRow game={row.past} />{/if}
        {/each}
        {#if rows.length > shown}
          <li class="more">
            <button class="subtle" onclick={() => (shown += BATCH)}>More games</button>
          </li>
        {/if}
      </ul>
      {#if gameroom.recentPageLoading && !gameroom.recentPage}
        <p class="hint">Loading more games…</p>
      {:else if gameroom.recentPageError}
        <p class="hint">
          Couldn't load more games: {gameroom.recentPageError}
          <button class="link" onclick={() => gameroom.loadRecentPage(true)}>Try again</button>
        </p>
      {:else if gameroom.recentPage && rows.length <= shown}
        <p class="hint">arimaa.com lists the last 100 games. For older ones, search for a player.</p>
      {/if}
    {/if}
  </section>
</div>

<style>
  .columns {
    display: grid;
    grid-template-columns: repeat(auto-fit, minmax(360px, 1fr));
    gap: 16px;
    align-items: start;
  }
  .column {
    display: flex;
    flex-direction: column;
    gap: 16px;
    min-width: 0;
  }
  .picked {
    display: flex;
    align-items: center;
    gap: 8px;
    font-size: 13px;
    margin: 12px 0 6px;
  }
  .found {
    margin-top: 10px;
  }
  .more {
    justify-content: center;
  }
  .hint {
    margin-top: 8px;
  }
</style>
