<script lang="ts">
  // Games to watch: every live game, and the postal games being played
  // (the lobby's live list leaves them out; one page fetch, made when the
  // tab first opens).
  import { onMount } from 'svelte';
  import { gameMeta } from './format';
  import LiveGameRow from './LiveGameRow.svelte';
  import Matchup from './Matchup.svelte';
  import { gameroom } from './store.svelte';

  const games = $derived(gameroom.games);
  const postal = $derived(gameroom.postal);
  let filter = $state('');

  onMount(() => {
    if (gameroom.postal == null && !gameroom.busy) void gameroom.loadPostal();
  });

  const matches = (...names: (string | null)[]) => {
    const words = filter.trim().toLowerCase();
    return !words || names.some((n) => n?.toLowerCase().includes(words));
  };
  const live = $derived((games?.live ?? []).filter((g) => matches(g.gold, g.silver)));
  const shownPostal = $derived((postal ?? []).filter((g) => matches(g.gold, g.silver)));
</script>

<div class="fields filter">
  <input bind:value={filter} placeholder="Filter by player" aria-label="Filter games by player" autocomplete="off" />
</div>

<div class="columns">
  <section class="card" aria-labelledby="gr-watch-live-title">
    <div class="card-head">
      <h2 id="gr-watch-live-title">Live games {#if games}<span class="count">{games.live.length}</span>{/if}</h2>
    </div>
    {#if !games}
      <p class="empty">…</p>
    {:else if games.live.length === 0}
      <p class="empty">No games are being played right now.</p>
    {:else if live.length === 0}
      <p class="empty">No live game matches.</p>
    {:else}
      <ul class="list scroll" aria-label="Live games">
        {#each live as g (g.gid)}<LiveGameRow game={g} />{/each}
      </ul>
    {/if}
  </section>

  <section class="card" aria-labelledby="gr-watch-postal-title">
    <div class="card-head">
      <h2 id="gr-watch-postal-title">Postal games {#if postal}<span class="count">{postal.length}</span>{/if}</h2>
      <span class="spacer"></span>
      <button class="subtle" onclick={() => gameroom.loadPostal()} disabled={gameroom.busy}>Reload</button>
    </div>
    {#if postal == null}
      <p class="empty">{gameroom.busy ? 'Fetching the postal games…' : 'Not loaded.'}</p>
    {:else if postal.length === 0}
      <p class="empty">No postal games are being played.</p>
    {:else if shownPostal.length === 0}
      <p class="empty">No postal game matches.</p>
    {:else}
      <ul class="list scroll" aria-label="Postal games">
        {#each shownPostal as g (g.gid)}
          <li>
            <span class="row-text">
              <span class="row-line">
                <Matchup gold={g.gold} silver={g.silver} goldRating={g.goldRating} silverRating={g.silverRating} />
              </span>
              <span class="meta">{gameMeta({ ...g, postal: false, turn: null })}</span>
            </span>
            <button onclick={() => gameroom.open(g.gid)} disabled={gameroom.busy} aria-label="Watch postal game {g.gid}"
              >Watch</button
            >
          </li>
        {/each}
      </ul>
    {/if}
  </section>
</div>

<style>
  .filter {
    margin-bottom: 12px;
    max-width: 360px;
  }
  .columns {
    display: grid;
    grid-template-columns: repeat(auto-fit, minmax(360px, 1fr));
    gap: 16px;
    align-items: start;
  }
</style>
