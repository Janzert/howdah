<script lang="ts">
  // The arimaa.com screen's first tab: what needs the user (invitations,
  // their games, fresh announcements), ways to start a game, and the top
  // of each list, with links to the tabs holding the rest.
  import Icon from '../Icon.svelte';
  import type { Color } from '../bindings/Color';
  import type { LiveGameView } from '../bindings/LiveGameView';
  import Announcement from './Announcement.svelte';
  import { announcementKey } from '../gameroom';
  import { gameMeta, invitationMeta } from './format';
  import LiveGameRow from './LiveGameRow.svelte';
  import Matchup from './Matchup.svelte';
  import RecentGameRow from './RecentGameRow.svelte';
  import { gameroom } from './store.svelte';

  /** How many of each list the overview shows. */
  const LIVE_SHOWN = 6;
  const RECENT_SHOWN = 5;
  const ONLINE_SHOWN = 24;

  const games = $derived(gameroom.games);
  const busy = $derived(gameroom.busy);
  let showOlder = $state(false);

  /** The free seat of an open game. */
  const freeSide = (g: LiveGameView): Color => (g.gold == null ? 'gold' : 'silver');
  const sideName = (c: Color) => (c === 'gold' ? 'Gold' : 'Silver');

  const nothingMine = $derived(
    gameroom.myGames.length === 0 && gameroom.incoming.length === 0 && gameroom.outgoing.length === 0,
  );
</script>

{#if gameroom.announcements.fresh.length > 0}
  <div class="announcements">
    {#each gameroom.announcements.fresh as a (announcementKey(a))}
      <Announcement announcement={a} />
    {/each}
  </div>
{/if}

<div class="columns">
  <div class="column">
    <section class="card" aria-labelledby="gr-mine-title">
      <div class="card-head">
        <h2 id="gr-mine-title">Your games</h2>
        <span class="spacer"></span>
        <button class="icon" onclick={() => (gameroom.tab = 'bots')}><Icon name="bot" size={16} /> Play a bot</button>
        <button class="primary icon" onclick={() => (gameroom.newGame = {})} disabled={busy}>
          <Icon name="plus" size={16} /> New game
        </button>
      </div>
      {#if nothingMine}
        <p class="empty">
          No games in progress. Create a game, sit at one of the open games below, or play one of the server's
          bots.
        </p>
      {:else}
        <ul class="list" aria-label="Your games and invitations">
          {#each gameroom.incoming as i (i.created + i.otherId)}
            <li class="highlight">
              <span class="row-text">
                <span class="row-line">
                  <span class="badge">Invitation</span>
                  <span class="name">{i.opponent ?? '?'}</span>
                  {#if i.opponentRating}<span class="rating">{i.opponentRating}</span>{/if}
                </span>
                <span class="meta">{invitationMeta(i)}{i.message ? ` · “${i.message}”` : ''}</span>
              </span>
              <button
                class="primary"
                onclick={() => gameroom.accept(i)}
                disabled={busy}
                aria-label="Accept the invitation from {i.opponent}">Accept</button
              >
              <button onclick={() => gameroom.decline(i)} disabled={busy} aria-label="Decline the invitation from {i.opponent}"
                >Decline</button
              >
            </li>
          {/each}
          {#each gameroom.myGames as g (g.gid)}
            {@const side = gameroom.mySide(g)}
            {@const turn = gameroom.myTurn(g)}
            <li class:highlight={turn}>
              <span class="row-text">
                <span class="row-line">
                  {#if turn}<span class="badge">Your move</span>{/if}
                  <Matchup gold={g.gold} silver={g.silver} empty="open seat" />
                </span>
                <span class="meta">{gameMeta(g)}</span>
              </span>
              {#if g.gold == null || g.silver == null}
                <button class="subtle" onclick={() => gameroom.cancelGame(g.gid)} disabled={busy} aria-label="Cancel game {g.gid}"
                  >Cancel</button
                >
              {/if}
              {#if side}
                <button class:primary={turn} onclick={() => gameroom.play(g.gid, side)} disabled={busy}
                  aria-label="Play game {g.gid}">Play</button
                >
              {/if}
            </li>
          {/each}
          {#each gameroom.outgoing as i (i.created + i.otherId)}
            <li>
              <span class="row-text">
                <span class="row-line">Waiting for <span class="name">{i.opponent ?? '?'}</span> to answer</span>
                <span class="meta">{invitationMeta(i)}</span>
              </span>
              <button class="subtle" onclick={() => gameroom.cancelInvitation(i)} disabled={busy}
                aria-label="Cancel the invitation to {i.opponent}">Cancel</button
              >
            </li>
          {/each}
        </ul>
      {/if}
    </section>

    <section class="card" aria-labelledby="gr-open-title">
      <div class="card-head">
        <h2 id="gr-open-title">
          Open games {#if games}<span class="count">{games.open.length}</span>{/if}
        </h2>
      </div>
      {#if !games}
        <p class="empty">…</p>
      {:else if games.open.length === 0}
        <p class="empty">Nobody is waiting for an opponent.</p>
      {:else}
        <ul class="list scroll" aria-label="Open games">
          {#each games.open as g (g.gid)}
            {@const side = freeSide(g)}
            <li>
              <span class="row-text">
                <span class="row-line">
                  <span class="dot {side === 'gold' ? 'silver' : 'gold'}"></span>
                  <span class="name">{g.gold ?? g.silver ?? '?'}</span>
                  <span class="vs">wants an opponent</span>
                </span>
                <span class="meta">{gameMeta(g)}</span>
              </span>
              <button onclick={() => gameroom.play(g.gid, side)} disabled={busy} aria-label="Sit as {side} in game {g.gid}"
                >Play as {sideName(side)}</button
              >
            </li>
          {/each}
        </ul>
      {/if}
    </section>
  </div>

  <div class="column">
    <section class="card" aria-labelledby="gr-live-title">
      <div class="card-head">
        <h2 id="gr-live-title">Live now {#if games}<span class="count">{games.live.length}</span>{/if}</h2>
        <span class="spacer"></span>
        <button class="link" onclick={() => (gameroom.tab = 'watch')}>Live and postal games</button>
      </div>
      {#if !games}
        <p class="empty">…</p>
      {:else if games.live.length === 0}
        <p class="empty">No games are being played right now.</p>
      {:else}
        <ul class="list" aria-label="Live games">
          {#each games.live.slice(0, LIVE_SHOWN) as g (g.gid)}<LiveGameRow game={g} />{/each}
        </ul>
        {#if games.live.length > LIVE_SHOWN}
          <button class="link more" onclick={() => (gameroom.tab = 'watch')}>All {games.live.length} live games</button>
        {/if}
      {/if}
    </section>

    <section class="card" aria-labelledby="gr-recent-title">
      <div class="card-head">
        <h2 id="gr-recent-title">Recently finished</h2>
        <span class="spacer"></span>
        <button class="link" onclick={() => (gameroom.tab = 'finished')}>More, and a player's games</button>
      </div>
      {#if !games}
        <p class="empty">…</p>
      {:else if games.recent.length === 0}
        <p class="empty">The gameroom lists no finished games.</p>
      {:else}
        <ul class="list" aria-label="Recently finished games">
          {#each games.recent.slice(0, RECENT_SHOWN) as g (g.gid)}<RecentGameRow game={g} />{/each}
        </ul>
      {/if}
    </section>

    <section class="card" aria-labelledby="gr-online-title">
      <div class="card-head">
        <h2 id="gr-online-title">Online {#if games}<span class="count">{gameroom.online.length}</span>{/if}</h2>
        <span class="spacer"></span>
        <button class="link" onclick={() => (gameroom.tab = 'players')}>Players</button>
      </div>
      {#if games && gameroom.online.length === 0}
        <p class="empty">Nobody is logged in to the gameroom.</p>
      {:else if games}
        <ul class="chips" aria-label="Players online">
          {#each gameroom.online.slice(0, ONLINE_SHOWN) as p (p.username)}
            <li>
              {#if gameroom.invitable.has(p.username)}
                <button
                  class="chip invitable"
                  onclick={() => (gameroom.newGame = { opponent: p.username })}
                  title="Invite {p.username} to a game"
                  aria-label="Invite {p.username}"
                >
                  {p.username}{#if p.rating}<span class="rating">{p.rating}</span>{/if}<Icon name="plus" size={12} />
                </button>
              {:else}
                <span class="chip" class:me={p.username === gameroom.user}>
                  {p.username}{#if p.rating}<span class="rating">{p.rating}</span>{/if}{#if p.bot}<span
                      class="tag">bot</span
                    >{/if}
                </span>
              {/if}
            </li>
          {/each}
          {#if gameroom.online.length > ONLINE_SHOWN}
            <li>
              <button class="link" onclick={() => (gameroom.tab = 'players')}
                >and {gameroom.online.length - ONLINE_SHOWN} more</button
              >
            </li>
          {/if}
        </ul>
      {/if}
    </section>
  </div>
</div>

{#if gameroom.announcements.older.length > 0}
  <div class="older">
    <button class="subtle" onclick={() => (showOlder = !showOlder)} aria-expanded={showOlder}
      >{showOlder ? 'Hide older announcements' : `Older announcements (${gameroom.announcements.older.length})`}</button
    >
    {#if showOlder}
      {#each gameroom.announcements.older as a (announcementKey(a))}
        <Announcement announcement={a} older />
      {/each}
    {/if}
  </div>
{/if}

<style>
  .announcements {
    display: flex;
    flex-direction: column;
    gap: 8px;
    margin-bottom: 16px;
  }
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
  .more {
    margin-top: 8px;
  }
  .chips {
    list-style: none;
    margin: 0;
    padding: 0;
    display: flex;
    flex-wrap: wrap;
    gap: 6px;
    align-items: center;
  }
  .chip {
    display: inline-flex;
    align-items: center;
    gap: 4px;
    font-size: 12px;
    padding: 2px 8px;
    border: 1px solid var(--border);
    border-radius: 12px;
  }
  .chip.me {
    border-style: dashed;
  }
  .chip.invitable {
    color: var(--text);
    border-color: var(--accent);
  }
  .chip .rating {
    margin-left: 2px;
  }
  .older {
    display: flex;
    flex-direction: column;
    align-items: flex-start;
    gap: 8px;
    margin-top: 16px;
  }
</style>
