<script lang="ts">
  // Who's logged in to the gameroom, with Invite for those who take
  // invitations (once seen using the lobby; see the store's `invitable`)
  // and their pages, and a search for any player's page (an exact
  // username opens it at once).
  import { showPlayerPage } from './player.svelte';
  import { gameroom } from './store.svelte';

  let filter = $state('');
  const shown = $derived.by(() => {
    const words = filter.trim().toLowerCase();
    return gameroom.online.filter((p) => !words || p.username.toLowerCase().includes(words));
  });

  async function find(e: SubmitEvent) {
    e.preventDefault();
    const exact = await gameroom.findPlayerPages(gameroom.pageQuery);
    if (exact) showPlayerPage(exact);
  }
</script>

<div class="columns">
  <section class="card" aria-labelledby="gr-players-title">
    <div class="card-head">
      <h2 id="gr-players-title">
        Online {#if gameroom.games}<span class="count">{gameroom.online.length}</span>{/if}
      </h2>
      <span class="spacer"></span>
      <input bind:value={filter} placeholder="Filter by name" aria-label="Filter players" autocomplete="off" />
    </div>
    {#if !gameroom.games}
      <p class="empty">…</p>
    {:else if gameroom.online.length === 0}
      <p class="empty">Nobody is logged in to the gameroom.</p>
    {:else if shown.length === 0}
      <p class="empty">No player matches.</p>
    {:else}
      <ul class="list" aria-label="Players online">
        {#each shown as p (p.username)}
          {@const me = p.username === gameroom.user}
          <li>
            <span class="row-text">
              <span class="row-line">
                <button
                  class="link name"
                  class:bot={p.bot}
                  onclick={() => showPlayerPage(p)}
                  title="{p.username}'s page{p.bot ? ' (bot)' : ''}">{p.username}</button
                >
                {#if p.title}<span class="tag">{p.title}</span>{/if}
                {#if p.rating}<span class="rating">{p.rating}</span>{/if}
                {#if me}<span class="tag">you</span>{/if}
                {#if p.busy && !me}<span class="tag">playing</span>{/if}
              </span>
              {#if p.country}<span class="meta">{p.country}</span>{/if}
            </span>
            {#if gameroom.invitable.has(p.username)}
              <button
                onclick={() => (gameroom.newGame = { opponent: p.username })}
                disabled={gameroom.busy}
                aria-label="Invite {p.username}"
                title="Invite {p.username} to a game">Invite</button
              >
            {/if}
          </li>
        {/each}
      </ul>
    {/if}
    <p class="hint">
      Invite is offered for players who take invitations, once they've been seen using the gameroom lobby.
    </p>
  </section>

  <section class="card" aria-labelledby="gr-find-title">
    <div class="card-head"><h2 id="gr-find-title">Find a player</h2></div>
    <form class="fields" onsubmit={find}>
      <input
        bind:value={gameroom.pageQuery}
        placeholder="Part of a username or name"
        aria-label="Find a player"
        autocomplete="off"
      />
      <button type="submit" disabled={gameroom.busy || !gameroom.pageQuery.trim()}>Search</button>
    </form>
    {#if gameroom.pageMatches && gameroom.pageMatches.length === 0}
      <p class="empty">No players match.</p>
    {:else if gameroom.pageMatches}
      <ul class="list scroll found" aria-label="Players found">
        {#each gameroom.pageMatches as p (p.id)}
          <li>
            <span class="row-text">
              <span class="row-line">
                <button class="link name" onclick={() => showPlayerPage(p)} title="{p.username}'s page"
                  >{p.username}</button
                >
              </span>
              {#if p.name}<span class="meta">{p.name}</span>{/if}
            </span>
          </li>
        {/each}
      </ul>
    {/if}
    <p class="hint">Searches every arimaa.com player, not just those online. An exact username opens their page.</p>
  </section>
</div>

<style>
  .columns {
    display: grid;
    grid-template-columns: repeat(auto-fit, minmax(360px, 1fr));
    gap: 16px;
    align-items: start;
  }
  .found {
    margin-top: 10px;
  }
  .name {
    font-size: inherit;
    font-weight: 600;
  }
  /* Bots are told apart by shade rather than a tag: their names already
   * start with bot_. */
  .name.bot {
    color: var(--muted);
  }
  .card-head input {
    width: 14em;
  }
  .hint {
    margin-top: 10px;
  }
</style>
