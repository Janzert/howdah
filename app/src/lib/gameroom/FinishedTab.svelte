<script lang="ts">
  // Finished games: the gameroom's recently finished list, a player's
  // past games (search; an exact username goes straight to their games,
  // with "Older games" paging), and any game by id.
  import { pastMeta } from './format';
  import Matchup from './Matchup.svelte';
  import RecentGameRow from './RecentGameRow.svelte';
  import { gameroom } from './store.svelte';

  /** The id typed into "Open a game by id". */
  let gameId = $state('');
  const games = $derived(gameroom.games);
  const picked = $derived(gameroom.picked);
  const players = $derived(gameroom.players);

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
            {#each picked.games as g (g.gid)}
              <li class="pick">
                <button onclick={() => gameroom.open(g.gid)} disabled={gameroom.busy} title="Open game {g.gid}">
                  <span class="row-text">
                    <span class="row-line"><Matchup gold={g.gold} silver={g.silver} /></span>
                    <span class="meta">{pastMeta(g)}</span>
                  </span>
                  <span class="action">Open</span>
                </button>
              </li>
            {/each}
            {#if picked.next != null}
              <li class="more">
                <button class="subtle" onclick={() => gameroom.olderGames()} disabled={gameroom.busy}>Older games</button>
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
      <h2 id="gr-finished-title">Recently finished {#if games}<span class="count">{games.recent.length}</span>{/if}</h2>
    </div>
    {#if !games}
      <p class="empty">…</p>
    {:else if games.recent.length === 0}
      <p class="empty">The gameroom lists no finished games.</p>
    {:else}
      <ul class="list scroll" aria-label="Recently finished games">
        {#each games.recent as g (g.gid)}<RecentGameRow game={g} />{/each}
      </ul>
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
