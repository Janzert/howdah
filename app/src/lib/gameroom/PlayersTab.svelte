<script lang="ts">
  // Who's logged in to the gameroom, with Invite for those who take
  // invitations (once seen using the lobby; see the store's `invitable`)
  // and their past games.
  import { gameroom } from './store.svelte';

  let filter = $state('');
  const shown = $derived.by(() => {
    const words = filter.trim().toLowerCase();
    return gameroom.online.filter((p) => !words || p.username.toLowerCase().includes(words));
  });
</script>

<section class="card" aria-labelledby="gr-players-title">
  <div class="card-head">
    <h2 id="gr-players-title">Online {#if gameroom.games}<span class="count">{gameroom.online.length}</span>{/if}</h2>
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
              <span class="name">{p.username}</span>
              {#if p.title}<span class="tag">{p.title}</span>{/if}
              {#if p.rating}<span class="rating">{p.rating}</span>{/if}
              {#if p.bot}<span class="tag">bot</span>{/if}
              {#if me}<span class="tag">you</span>{/if}
              {#if p.busy && !me}<span class="tag">playing</span>{/if}
            </span>
            {#if p.country}<span class="meta">{p.country}</span>{/if}
          </span>
          <button class="subtle" onclick={() => gameroom.showPlayer(p.username)} disabled={gameroom.busy}
            aria-label="Past games of {p.username}">Games</button
          >
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

<style>
  .card-head input {
    width: 14em;
  }
  .hint {
    margin-top: 10px;
  }
</style>
