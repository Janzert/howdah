<script lang="ts">
  import { api, errorMessage } from './api';
  import type { LiveGameView } from './bindings/LiveGameView';

  interface Props {
    /** Follows a game; resolves to an error message, or null on success. */
    onWatch: (gid: string) => Promise<string | null>;
    onClose: () => void;
  }
  let { onWatch, onClose }: Props = $props();

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
  let games = $state<LiveGameView[] | null>(null);
  let busy = $state(false);
  let error = $state<string | null>(null);
  let dialog: HTMLDialogElement;

  $effect(() => {
    dialog.showModal();
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

  /** Runs a request, showing its error. */
  async function attempt(f: () => Promise<void>) {
    busy = true;
    error = null;
    try {
      await f();
    } catch (e) {
      error = errorMessage(e);
    } finally {
      busy = false;
    }
  }

  function refresh() {
    return attempt(async () => {
      games = await api.liveGames();
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
      games = await api.liveGames();
    });
  }

  function logout() {
    attempt(async () => {
      await api.gameroomLogout();
      user = null;
      games = null;
    });
  }

  async function watch(gid: string) {
    busy = true;
    error = await onWatch(gid);
    busy = false;
    if (!error) onClose();
  }
</script>

<dialog bind:this={dialog} onclose={onClose} aria-labelledby="watch-title">
  <h2 id="watch-title">Watch a game on arimaa.com</h2>
  {#if user === null}
    <form onsubmit={login}>
      <div class="grid">
        <label for="gr-user">Username</label>
        <!-- svelte-ignore a11y_autofocus -->
        <input id="gr-user" bind:value={username} autocomplete="username" autofocus={!username} />
        <label for="gr-password">Password</label>
        <!-- svelte-ignore a11y_autofocus -->
        <input
          id="gr-password"
          type="password"
          bind:value={password}
          autocomplete="current-password"
          placeholder={savedUser != null && username.trim() === savedUser ? 'Saved password' : ''}
          autofocus={!!username}
        />
        <span></span>
        <label class="check">
          <input type="checkbox" bind:checked={remember} />
          Remember password
        </label>
      </div>
      <p class="hint">
        Watching needs a gameroom login (a human account). The password goes over plain HTTP, as arimaa.com requires.
        {#if remember}
          It's saved on this computer obfuscated, not encrypted: anyone who can read your files could recover it.
        {:else}
          It isn't saved{savedUser ? ', and logging in forgets the saved one' : ''}.
        {/if}
      </p>
      {#if error}<p class="error">{error}</p>{/if}
      <div class="buttons">
        <span class="spacer"></span>
        <button type="button" onclick={onClose}>Cancel</button>
        <button class="primary" type="submit" disabled={busy || (!password && !usesSaved)}>Log in</button>
      </div>
    </form>
  {:else if user}
    <div class="who">
      Logged in as <strong>{user}</strong>
      <button class="subtle" onclick={logout} disabled={busy}>Log out</button>
    </div>
    {#if games && games.length === 0}
      <p class="hint">No games are being played right now.</p>
    {:else if games}
      <ul class="games" aria-label="Live games">
        {#each games as g (g.gid)}
          <li>
            <span class="players">
              <span class="dot gold"></span>{g.gold ?? '?'}
              <span class="vs">vs</span>
              <span class="dot silver"></span>{g.silver ?? '?'}
            </span>
            <span class="meta">
              {g.timeControl ?? ''}{g.rated ? ' · rated' : ''}{g.postal ? ' · postal' : ''}
            </span>
            <button onclick={() => watch(g.gid)} disabled={busy} aria-label="Watch game {g.gid}">Watch</button>
          </li>
        {/each}
      </ul>
    {/if}
    {#if error}<p class="error">{error}</p>{/if}
    <div class="buttons">
      <button onclick={refresh} disabled={busy}>Refresh</button>
      <span class="spacer"></span>
      <button onclick={onClose}>Close</button>
    </div>
  {:else}
    <p class="hint">…</p>
  {/if}
</dialog>

<style>
  dialog {
    width: min(520px, 92vw);
    border: 1px solid var(--border);
    border-radius: 8px;
    background: var(--panel);
    color: var(--text);
    padding: 16px;
  }
  dialog::backdrop {
    background: rgba(0, 0, 0, 0.4);
  }
  h2 {
    margin: 0 0 12px;
    font-size: 16px;
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
    max-height: 50vh;
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
