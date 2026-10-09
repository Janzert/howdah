<script lang="ts">
  // The arimaa.com lobby, a section of the lobby window: the login, the
  // user's games and invitations, open, live, postal and finished games, a
  // player's games, and a new game. Playing, watching or opening a game
  // opens a game window for it (`onOpen`).
  import { onMount } from 'svelte';
  import { type Api, api, errorMessage } from './api';
  import type { Color } from './bindings/Color';
  import type { GameResult } from './bindings/GameResult';
  import type { LiveGameView } from './bindings/LiveGameView';
  import type { GameroomGames } from './bindings/GameroomGames';
  import type { InvitationView } from './bindings/InvitationView';
  import type { PastGameView } from './bindings/PastGameView';
  import type { PlayerMatchView } from './bindings/PlayerMatchView';
  import type { PostalGameView } from './bindings/PostalGameView';
  import type { RecentGameView } from './bindings/RecentGameView';
  import type { WinReason } from './bindings/WinReason';
  import { on } from './events';
  import { mySide as sideIn, myTurn as turnIn } from './gameroom';

  interface Props {
    /** Opens a game window whose session `start` makes a gameroom game
     * (opening, joining or creating one), or brings forward the window
     * that has `game` already. Resolves to an error message, or null on
     * success. */
    onOpen: (start: (a: Api) => Promise<void>, game?: { gid: string; play: boolean }) => Promise<string | null>;
    /** The lists, whenever they're fetched here (the lobby's poll comes
     * as an event too). */
    onGames?: (games: GameroomGames) => void;
  }
  let { onOpen, onGames }: Props = $props();

  function pref(key: string): string {
    try {
      return localStorage.getItem(key) ?? '';
    } catch {
      return '';
    }
  }
  function savePref(key: string, value: string) {
    try {
      localStorage.setItem(key, value);
    } catch {
      /* not persisted */
    }
  }

  /** Who is logged in; undefined until the backend says. */
  let user = $state<string | null | undefined>(undefined);
  let username = $state(pref('gameroom.username'));
  let password = $state('');
  let remember = $state(false);
  /** The username whose password the backend has saved. */
  let savedUser = $state<string | null>(null);
  const usesSaved = $derived(savedUser != null && username.trim() === savedUser && password === '');
  let games = $state<GameroomGames | null>(null);
  $effect(() => {
    if (games) onGames?.(games);
  });
  /** The id typed into "Open game". */
  let gameId = $state('');
  let busy = $state(false);
  let error = $state<string | null>(null);

  /** The text typed into the player search. */
  let playerQuery = $state('');
  /** The players the last search found. */
  let players = $state<PlayerMatchView[] | null>(null);
  /** The player whose games are shown, with the pages loaded so far. */
  let picked = $state<{ player: PlayerMatchView; games: PastGameView[]; next: number | null } | null>(null);

  /** The new game form. */
  let newSide = $state<Color | 'random'>((pref('gameroom.newSide') as Color | 'random') || 'random');
  let newTc = $state(pref('gameroom.newTc') || '2m/5m/100/0/30m');
  let newRated = $state(pref('gameroom.newRated') === '1');
  /** A player to invite, instead of opening the game to anyone. */
  let newOpponent = $state('');
  let newMessage = $state('');
  /** Time controls offered in the form; any other can be typed. The
   * postal ones are the gameroom's own ("No time limit" is its
   * `0/0/0/0/0`). */
  const TIME_CONTROLS: [string, string][] = [
    ['15s/3m/100/0/15m', ''],
    ['1m/2m/100/0/10m', ''],
    ['2m/5m/100/0/30m', ''],
    ['1d/60d/100/0/300d/21d', 'Postal: the most popular'],
    ['1d/14d/100/14d/0', 'Postal: a day a move, 14 day reserve'],
    ['0/0/0/0/0', 'Postal: no time limit'],
  ];

  /** The postal games being played, once asked for. */
  let postal = $state<PostalGameView[] | null>(null);

  function loadPostal() {
    attempt(async () => {
      postal = await api.gameroomPostalGames();
    });
  }

  /** The user's games, those waiting on their move first. */
  const myGames = $derived(
    games ? [...games.mine].sort((a, b) => Number(myTurn(b)) - Number(myTurn(a))) : [],
  );

  /** How often the lists are refreshed while the lobby is shown, as the
   * browser lobby does. */
  const REFRESH_MS = 20_000;

  onMount(() => {
    // The backend's poll, every minute while logged in.
    const unlisten = on('gameroom://lobby', (g) => {
      if (user && g.user === user) games = g;
    });
    return () => void unlisten.then((f) => f());
  });

  $effect(() => {
    api.gameroomStatus().then((s) => {
      savedUser = s.savedUsername;
      if (savedUser) {
        username = savedUser;
        remember = true;
      }
      user = s.username;
      if (user) refresh();
    });
  });

  /** Runs a request, showing its error. A `quiet` one (the timed
   * refresh) leaves the buttons alone. After an error the login is
   * checked again, since an expired one logs out. */
  async function attempt(f: () => Promise<void>, quiet = false) {
    if (!quiet) {
      busy = true;
      error = null;
    }
    try {
      await f();
      if (quiet) error = null;
    } catch (e) {
      error = errorMessage(e);
      user = (await api.gameroomStatus()).username;
    } finally {
      if (!quiet) busy = false;
    }
  }

  function refresh(quiet = false) {
    return attempt(async () => {
      games = await api.gameroomGames();
    }, quiet);
  }

  $effect(() => {
    if (!user) return;
    const timer = setInterval(() => {
      if (!busy && !document.hidden) refresh(true);
    }, REFRESH_MS);
    return () => clearInterval(timer);
  });

  function searchPlayers(e: SubmitEvent) {
    e.preventDefault();
    const text = playerQuery.trim();
    attempt(async () => {
      const found = await api.searchGameroomPlayers(text);
      players = found;
      picked = null;
      // An exact username goes straight to that player's games.
      const exact = found.filter((p) => p.username.toLowerCase() === text.toLowerCase());
      if (exact.length === 1) await pick(exact[0]);
    });
  }

  async function pick(player: PlayerMatchView) {
    const page = await api.gameroomPlayerGames(player.id, 0);
    picked = { player, games: page.games, next: page.next };
  }

  function olderGames() {
    const p = picked;
    if (!p || p.next == null) return;
    attempt(async () => {
      const page = await api.gameroomPlayerGames(p.player.id, p.next!);
      picked = { player: p.player, games: [...p.games, ...page.games], next: page.next };
    });
  }

  function login(e: SubmitEvent) {
    e.preventDefault();
    attempt(async () => {
      const s = await api.gameroomLogin(username, password, remember);
      password = '';
      savedUser = s.savedUsername;
      user = s.username;
      savePref('gameroom.username', username.trim());
      games = await api.gameroomGames();
    });
  }

  function logout() {
    attempt(async () => {
      await api.gameroomLogout();
      user = null;
      games = null;
    });
  }

  async function enter(start: (a: Api) => Promise<void>, game?: { gid: string; play: boolean }) {
    busy = true;
    error = await onOpen(start, game);
    busy = false;
  }

  function open(gid: string) {
    return enter((a) => a.openGameroomGame(gid.trim()), { gid: gid.trim(), play: false });
  }

  function openById(e: SubmitEvent) {
    e.preventDefault();
    open(gameId);
  }

  const mySide = (g: LiveGameView) => sideIn(g, user);
  const myTurn = (g: LiveGameView) => turnIn(g, user);

  /** The free seat of an open game. */
  const freeSide = (g: LiveGameView): Color => (g.gold == null ? 'gold' : 'silver');

  function createGame(e: SubmitEvent) {
    e.preventDefault();
    savePref('gameroom.newSide', newSide);
    savePref('gameroom.newTc', newTc.trim());
    savePref('gameroom.newRated', newRated ? '1' : '0');
    const side: Color = newSide === 'random' ? (Math.random() < 0.5 ? 'gold' : 'silver') : newSide;
    if (newOpponent.trim()) {
      // An invitation: it waits for the answer; accepted, the game opens.
      attempt(async () => {
        await api.inviteGameroomPlayer(newOpponent.trim(), side, newTc.trim(), newRated, newMessage);
        newOpponent = '';
        newMessage = '';
        games = await api.gameroomGames();
      });
      return;
    }
    enter((a) => a.createGameroomGame(side, newTc.trim(), newRated));
  }

  const incoming = $derived(games?.invitations.filter((i) => i.incoming) ?? []);
  const outgoing = $derived(games?.invitations.filter((i) => !i.incoming) ?? []);

  function invitationMeta(i: InvitationView): string {
    return [`as ${i.side === 'gold' ? 'Gold' : 'Silver'}`, i.timeControl ?? '', i.rated ? 'rated' : 'unrated']
      .filter((s) => s)
      .join(' · ');
  }

  function decline(i: InvitationView) {
    attempt(async () => {
      await api.declineGameroomInvitation(i.otherId, i.created, '');
      games = await api.gameroomGames();
    });
  }

  function cancelInvitation(i: InvitationView) {
    attempt(async () => {
      await api.cancelGameroomInvitation(i.otherId, i.created);
      games = await api.gameroomGames();
    });
  }

  function cancelGame(gid: string) {
    attempt(async () => {
      await api.cancelGameroomGame(gid);
      games = await api.gameroomGames();
    });
  }

  function gameMeta(g: LiveGameView): string {
    return [g.timeControl ?? '', g.rated ? 'rated' : '', g.postal ? 'postal' : ''].filter((s) => s).join(' · ');
  }

  const reasons: Record<WinReason, string> = {
    goal: 'goal',
    elimination: 'elimination',
    immobilization: 'immobilization',
    timeout: 'time',
    resignation: 'resignation',
    illegalMove: 'illegal move',
    score: 'score',
    forfeit: 'forfeit',
  };

  function resultText(result: GameResult | null): string[] {
    return result ? [`${result.winner === 'gold' ? 'Gold' : 'Silver'} won by ${reasons[result.reason]}`] : [];
  }

  /** "Gold won by goal · 33 moves · Jan 4, 2014 10:38 am" */
  function pastMeta(g: PastGameView): string {
    const parts = resultText(g.result);
    if (g.moves != null) parts.push(`${g.moves} moves`);
    if (g.finished) parts.push(g.finished);
    if (g.timeControl) parts.push(g.timeControl);
    if (g.rated) parts.push('rated');
    return parts.join(' · ');
  }

  /** "Gold won by goal · 33 moves · Oct 4, 10:23 · rated" */
  function recentMeta(g: RecentGameView): string {
    const parts = resultText(g.result);
    if (g.moves != null) parts.push(`${g.moves} moves`);
    if (g.endedMs != null) {
      parts.push(
        new Date(g.endedMs).toLocaleString(undefined, {
          month: 'short',
          day: 'numeric',
          hour: 'numeric',
          minute: '2-digit',
        }),
      );
    }
    if (g.rated) parts.push('rated');
    return parts.join(' · ');
  }
</script>

<section aria-labelledby="watch-title">
  <h2 id="watch-title">arimaa.com</h2>
  {#if user === null}
    <form onsubmit={login}>
      <div class="grid">
        <label for="gr-user">Username</label>
        <input id="gr-user" bind:value={username} autocomplete="username" />
        <label for="gr-password">Password</label>
        <input
          id="gr-password"
          type="password"
          bind:value={password}
          autocomplete="current-password"
          placeholder={savedUser != null && username.trim() === savedUser ? 'Saved password' : ''}
        />
        <span></span>
        <label class="check">
          <input type="checkbox" bind:checked={remember} />
          Remember password
        </label>
      </div>
      <p class="hint">
        Playing, watching and opening games needs a gameroom login (a human account). The password goes over plain HTTP, as arimaa.com requires.
        {#if remember}
          It's saved on this computer obfuscated, not encrypted: anyone who can read your files could recover it.
        {:else}
          It isn't saved{savedUser ? ', and logging in forgets the saved one' : ''}.
        {/if}
      </p>
      {#if error}<p class="error">{error}</p>{/if}
      <div class="buttons">
        <span class="spacer"></span>
        <button class="primary" type="submit" disabled={busy || (!password && !usesSaved)}>Log in</button>
      </div>
    </form>
  {:else if user}
    <div class="who">
      Logged in as <strong>{user}</strong>
      <button class="subtle" onclick={logout} disabled={busy}>Log out</button>
    </div>
    {#if incoming.length > 0 || outgoing.length > 0}
      <h3>Invitations</h3>
      <ul class="games" aria-label="Invitations">
        {#each incoming as i (i.created + i.otherId)}
          <li>
            <span class="game">
              <span class="players">
                <span class="badge">Invites you</span>{i.opponent ?? '?'}
                {#if i.opponentRating}<span class="rating">{i.opponentRating}</span>{/if}
              </span>
              <span class="meta">{invitationMeta(i)}{i.message ? ` · “${i.message}”` : ''}</span>
            </span>
            <button onclick={() => enter((a) => a.acceptGameroomInvitation(i.otherId, i.created))} disabled={busy}
              aria-label="Accept the invitation from {i.opponent}">Accept</button
            >
            <button onclick={() => decline(i)} disabled={busy} aria-label="Decline the invitation from {i.opponent}"
              >Decline</button
            >
          </li>
        {/each}
        {#each outgoing as i (i.created + i.otherId)}
          <li>
            <span class="game">
              <span class="players">Waiting for {i.opponent ?? '?'} to answer</span>
              <span class="meta">{invitationMeta(i)}</span>
            </span>
            <button onclick={() => cancelInvitation(i)} disabled={busy}
              aria-label="Cancel the invitation to {i.opponent}">Cancel</button
            >
          </li>
        {/each}
      </ul>
    {/if}
    {#if games && games.mine.length > 0}
      <h3>Your games</h3>
      <ul class="games" aria-label="Your games">
        {#each myGames as g (g.gid)}
          {@const side = mySide(g)}
          <li>
            <span class="game">
              <span class="players">
                {#if myTurn(g)}<span class="badge">Your move</span>{/if}
                <span class="dot gold"></span>{g.gold ?? 'open seat'}
                <span class="vs">vs</span>
                <span class="dot silver"></span>{g.silver ?? 'open seat'}
              </span>
              <span class="meta">{gameMeta(g)}</span>
            </span>
            {#if side}
              <button onclick={() => enter((a) => a.playGameroomGame(g.gid, side), { gid: g.gid, play: true })} disabled={busy}
                aria-label="Play game {g.gid}">Play</button
              >
            {/if}
            {#if g.gold == null || g.silver == null}
              <button onclick={() => cancelGame(g.gid)} disabled={busy} aria-label="Cancel game {g.gid}"
                >Cancel</button
              >
            {/if}
          </li>
        {/each}
      </ul>
    {/if}
    <h3>Open games</h3>
    {#if games && games.open.length === 0}
      <p class="hint">Nobody is waiting for an opponent.</p>
    {:else if games}
      <ul class="games" aria-label="Open games">
        {#each games.open as g (g.gid)}
          {@const side = freeSide(g)}
          <li>
            <span class="game">
              <span class="players">
                <span class="dot {side === 'gold' ? 'silver' : 'gold'}"></span>{g.gold ?? g.silver ?? '?'}
                <span class="vs">wants an opponent</span>
              </span>
              <span class="meta">{gameMeta(g)}</span>
            </span>
            <button onclick={() => enter((a) => a.playGameroomGame(g.gid, side), { gid: g.gid, play: true })} disabled={busy}
              aria-label="Sit as {side} in game {g.gid}">Play as {side === 'gold' ? 'Gold' : 'Silver'}</button
            >
          </li>
        {/each}
      </ul>
    {/if}
    <h3>New game</h3>
    <form class="new-game" onsubmit={createGame}>
      <label for="gr-new-side">Side</label>
      <select id="gr-new-side" bind:value={newSide}>
        <option value="random">Random</option>
        <option value="gold">Gold</option>
        <option value="silver">Silver</option>
      </select>
      <label for="gr-new-tc">Time</label>
      <input
        id="gr-new-tc"
        bind:value={newTc}
        list="gr-tcs"
        autocomplete="off"
        title="Per move / reserve / percent added / reserve limit / game limit"
      />
      <datalist id="gr-tcs">
        {#each TIME_CONTROLS as [tc, label] (tc)}<option value={tc} {label}></option>{/each}
      </datalist>
      <label class="check"><input type="checkbox" bind:checked={newRated} /> Rated</label>
      <label for="gr-new-opponent">Opponent</label>
      <input id="gr-new-opponent" bind:value={newOpponent} placeholder="Anyone" autocomplete="off" />
      {#if newOpponent.trim()}
        <input bind:value={newMessage} placeholder="Message (optional)" aria-label="Message" autocomplete="off" />
      {/if}
      <button type="submit" disabled={busy || !newTc.trim()}>{newOpponent.trim() ? 'Invite' : 'Create'}</button>
    </form>
    <p class="hint">
      {#if newOpponent.trim()}
        Invites {newOpponent.trim()}; if they accept, the game opens in a new window.
      {:else}
        The game waits in the gameroom's open games until someone sits; your first move goes once they do.
      {/if}
    </p>
    <h3>Live</h3>
    {#if games && games.live.length === 0}
      <p class="hint">No games are being played right now.</p>
    {:else if games}
      <ul class="games" aria-label="Live games">
        {#each games.live as g (g.gid)}
          <li>
            <span class="players">
              <span class="dot gold"></span>{g.gold ?? '?'}
              <span class="vs">vs</span>
              <span class="dot silver"></span>{g.silver ?? '?'}
            </span>
            <span class="meta">
              {g.timeControl ?? ''}{g.rated ? ' · rated' : ''}{g.postal ? ' · postal' : ''}
            </span>
            <button onclick={() => open(g.gid)} disabled={busy} aria-label="Watch game {g.gid}">Watch</button>
          </li>
        {/each}
      </ul>
    {/if}
    <h3>Postal games</h3>
    {#if postal == null}
      <button class="subtle" onclick={loadPostal} disabled={busy}>Show postal games</button>
    {:else if postal.length === 0}
      <p class="hint">No postal games are being played.</p>
    {:else}
      <ul class="games" aria-label="Postal games">
        {#each postal as g (g.gid)}
          <li>
            <span class="game">
              <span class="players">
                <span class="dot gold"></span>{g.gold}
                {#if g.goldRating}<span class="rating">{g.goldRating}</span>{/if}
                <span class="vs">vs</span>
                <span class="dot silver"></span>{g.silver}
                {#if g.silverRating}<span class="rating">{g.silverRating}</span>{/if}
              </span>
              <span class="meta">{[g.timeControl ?? '', g.rated ? 'rated' : ''].filter((s) => s).join(' · ')}</span>
            </span>
            <button onclick={() => open(g.gid)} disabled={busy} aria-label="Watch postal game {g.gid}">Watch</button>
          </li>
        {/each}
      </ul>
    {/if}
    <h3>Recently finished</h3>
    {#if games && games.recent.length === 0}
      <p class="hint">The gameroom lists no finished games.</p>
    {:else if games}
      <ul class="games" aria-label="Recently finished games">
        {#each games.recent as g (g.gid)}
          <li>
            <span class="game">
              <span class="players">
                <span class="dot gold"></span>{g.gold ?? '?'}
                {#if g.goldRating}<span class="rating">{g.goldRating}</span>{/if}
                <span class="vs">vs</span>
                <span class="dot silver"></span>{g.silver ?? '?'}
                {#if g.silverRating}<span class="rating">{g.silverRating}</span>{/if}
              </span>
              <span class="meta">{recentMeta(g)}</span>
            </span>
            <button onclick={() => open(g.gid)} disabled={busy} aria-label="Open game {g.gid}">Open</button>
          </li>
        {/each}
      </ul>
    {/if}
    <h3>A player's games</h3>
    <form class="by-id" onsubmit={searchPlayers}>
      <label for="gr-player">Player</label>
      <input id="gr-player" bind:value={playerQuery} placeholder="Part of a username or name" autocomplete="off" />
      <button type="submit" disabled={busy || !playerQuery.trim()}>Search</button>
    </form>
    {#if picked}
      <div class="picked">
        Games of <strong>{picked.player.username}</strong>
        {#if players && players.length > 1}
          <button class="subtle" onclick={() => (picked = null)}>Back to the players</button>
        {/if}
      </div>
      {#if picked.games.length === 0}
        <p class="hint">No finished games.</p>
      {:else}
        <ul class="games" aria-label="Games of {picked.player.username}">
          {#each picked.games as g (g.gid)}
            <li>
              <span class="game">
                <span class="players">
                  <span class="dot gold"></span>{g.gold}
                  <span class="vs">vs</span>
                  <span class="dot silver"></span>{g.silver}
                </span>
                <span class="meta">{pastMeta(g)}</span>
              </span>
              <button onclick={() => open(g.gid)} disabled={busy} aria-label="Open game {g.gid}">Open</button>
            </li>
          {/each}
          {#if picked.next != null}
            <li class="more"><button class="subtle" onclick={olderGames} disabled={busy}>Older games</button></li>
          {/if}
        </ul>
      {/if}
    {:else if players && players.length === 0}
      <p class="hint">No players match.</p>
    {:else if players}
      <ul class="games" aria-label="Players found">
        {#each players as p (p.id)}
          <li>
            <span class="game">
              <span class="players">{p.username}</span>
              {#if p.name}<span class="meta">{p.name}</span>{/if}
            </span>
            <button onclick={() => attempt(() => pick(p))} disabled={busy} aria-label="Games of {p.username}"
              >Games</button
            >
          </li>
        {/each}
      </ul>
    {/if}
    <h3>By id</h3>
    <form class="by-id" onsubmit={openById}>
      <label for="gr-game-id">Game id</label>
      <input id="gr-game-id" bind:value={gameId} inputmode="numeric" placeholder="e.g. 671438" autocomplete="off" />
      <button type="submit" disabled={busy || !/^\s*\d+\s*$/.test(gameId)}>Open</button>
    </form>
    <p class="hint">
      A finished game's id (as in the gameroom's game lists and the archive) loads the whole game; a live game's
      gameroom id watches it.
    </p>
    {#if error}<p class="error">{error}</p>{/if}
    <div class="buttons">
      <button onclick={() => refresh()} disabled={busy}>Refresh</button>
    </div>
  {:else}
    <p class="hint">…</p>
  {/if}
</section>

<style>
  h2 {
    margin: 0 0 12px;
    font-size: 16px;
  }
  h3 {
    margin: 12px 0 6px;
    font-size: 13px;
    color: var(--muted);
    font-weight: 600;
  }
  .grid {
    display: grid;
    grid-template-columns: auto 1fr;
    gap: 8px 12px;
    align-items: center;
  }
  .check {
    display: flex;
    align-items: center;
    gap: 6px;
    font-size: 13px;
  }
  .who {
    display: flex;
    align-items: center;
    gap: 8px;
    font-size: 13px;
    margin-bottom: 8px;
  }
  .games {
    list-style: none;
    margin: 0;
    padding: 0;
    max-height: 30vh;
    overflow-y: auto;
    border: 1px solid var(--border);
    border-radius: 6px;
  }
  .games li {
    display: flex;
    align-items: center;
    gap: 10px;
    padding: 6px 8px;
  }
  .games li + li {
    border-top: 1px solid var(--border);
  }
  .players {
    display: flex;
    align-items: center;
    gap: 5px;
    flex: 1;
    min-width: 0;
    overflow: hidden;
    white-space: nowrap;
    text-overflow: ellipsis;
  }
  .game {
    display: flex;
    flex-direction: column;
    gap: 2px;
    flex: 1;
    min-width: 0;
  }
  .game .meta {
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
  }
  .rating {
    color: var(--muted);
    font-size: 11px;
  }
  .badge {
    flex: none;
    font-size: 11px;
    padding: 0 6px;
    border-radius: 8px;
    background: var(--accent);
    color: var(--accent-text);
  }
  .new-game {
    display: flex;
    align-items: center;
    flex-wrap: wrap;
    gap: 8px;
    font-size: 13px;
  }
  .new-game input:not([type='checkbox']) {
    flex: 1;
    min-width: 8em;
  }
  .by-id {
    display: flex;
    align-items: center;
    gap: 8px;
    font-size: 13px;
  }
  .picked {
    display: flex;
    align-items: center;
    gap: 8px;
    font-size: 13px;
    margin: 8px 0 6px;
  }
  .games li.more {
    justify-content: center;
  }
  .by-id input {
    flex: 1;
    min-width: 0;
  }
  .vs,
  .meta {
    color: var(--muted);
    font-size: 12px;
  }
  .dot {
    flex: none;
    width: 10px;
    height: 10px;
    border-radius: 50%;
    border: 1px solid rgba(0, 0, 0, 0.4);
  }
  .dot.gold {
    background: #e3b23c;
  }
  .dot.silver {
    background: #c9ced6;
  }
  .hint {
    margin: 8px 0 0;
    font-size: 12px;
    color: var(--muted);
  }
  .error {
    color: var(--warn);
    font-size: 13px;
  }
  .buttons {
    display: flex;
    gap: 6px;
    margin-top: 14px;
  }
  .spacer {
    flex: 1;
  }
  .subtle {
    font-size: 12px;
    padding: 2px 8px;
    color: var(--muted);
  }
</style>
