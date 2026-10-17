<script lang="ts">
  // The lobby window's arimaa.com screen: the login, then tabs: Overview
  // (what needs the user and the top of each list), Bots, Watch (live and
  // postal), Finished (recent, a player's games, by id) and Players
  // (online, with Invite). State lives in the store, so it outlasts the
  // screen; the lists refresh every 20 s while it's shown.
  import Icon from '../Icon.svelte';
  import BotsTab from './BotsTab.svelte';
  import FinishedTab from './FinishedTab.svelte';
  import LoginForm from './LoginForm.svelte';
  import NewGameDialog from './NewGameDialog.svelte';
  import Overview from './Overview.svelte';
  import PlayersTab from './PlayersTab.svelte';
  import { gameroom, type GameroomTab } from './store.svelte';
  import WatchTab from './WatchTab.svelte';

  $effect(() => {
    if (gameroom.user) return gameroom.autoRefresh();
  });

  /** What waits on the user in the overview: invitations to them and
   * their games on their move. */
  const waiting = $derived(gameroom.incoming.length + gameroom.myGames.filter((g) => gameroom.myTurn(g)).length);

  const tabs = $derived<{ id: GameroomTab; label: string; count?: number }[]>([
    { id: 'overview', label: 'Overview', count: waiting },
    { id: 'bots', label: 'Bots' },
    { id: 'watch', label: 'Watch' },
    { id: 'finished', label: 'Finished games' },
    { id: 'players', label: 'Players' },
  ]);

  function onTabKey(e: KeyboardEvent) {
    const i = tabs.findIndex((t) => t.id === gameroom.tab);
    const step = e.key === 'ArrowRight' ? 1 : e.key === 'ArrowLeft' ? -1 : 0;
    if (!step) return;
    e.preventDefault();
    const next = tabs[(i + step + tabs.length) % tabs.length];
    gameroom.tab = next.id;
    document.getElementById(`gr-tab-${next.id}`)?.focus();
  }
</script>

<div class="screen">
  <header>
    <h1>arimaa.com</h1>
    <span class="spacer"></span>
    {#if gameroom.user}
      <span class="who">Logged in as <strong>{gameroom.user}</strong></span>
      <button class="icon" onclick={() => gameroom.refresh()} disabled={gameroom.busy} title="Refresh the lists"
        aria-label="Refresh"><Icon name="refresh" size={16} /></button
      >
      <button class="subtle" onclick={() => gameroom.logout()} disabled={gameroom.busy}>Log out</button>
    {/if}
  </header>

  {#if gameroom.user === undefined}
    <p class="empty">…</p>
  {:else if gameroom.user === null}
    <LoginForm />
  {:else}
    <div class="tabs" role="tablist" aria-label="arimaa.com" tabindex="-1" onkeydown={onTabKey}>
      {#each tabs as t (t.id)}
        <button
          id="gr-tab-{t.id}"
          role="tab"
          aria-selected={gameroom.tab === t.id}
          aria-controls="gr-panel"
          tabindex={gameroom.tab === t.id ? 0 : -1}
          class:on={gameroom.tab === t.id}
          onclick={() => (gameroom.tab = t.id)}
        >
          {t.label}
          {#if t.count}<span class="badge">{t.count}</span>{/if}
        </button>
      {/each}
    </div>

    {#if gameroom.error && gameroom.newGame == null}
      <div class="banner" role="alert">
        <span>{gameroom.error}</span>
        <span class="spacer"></span>
        <button class="subtle" onclick={() => (gameroom.error = null)} aria-label="Dismiss">
          <Icon name="close" size={14} />
        </button>
      </div>
    {/if}

    <div id="gr-panel" role="tabpanel" aria-labelledby="gr-tab-{gameroom.tab}">
      {#if gameroom.tab === 'overview'}
        <Overview />
      {:else if gameroom.tab === 'bots'}
        <BotsTab />
      {:else if gameroom.tab === 'watch'}
        <WatchTab />
      {:else if gameroom.tab === 'finished'}
        <FinishedTab />
      {:else}
        <PlayersTab />
      {/if}
    </div>
  {/if}
</div>

{#if gameroom.newGame}
  <NewGameDialog initial={gameroom.newGame} onClose={() => (gameroom.newGame = null)} />
{/if}

<style>
  header {
    display: flex;
    align-items: center;
    gap: 8px;
    margin-bottom: 14px;
    min-height: 30px;
  }
  h1 {
    margin: 0;
    font-size: 20px;
  }
  .who {
    font-size: 13px;
    color: var(--muted);
  }
  .who strong {
    color: var(--text);
  }
  header button.icon {
    padding: 4px 6px;
  }
  .tabs {
    display: flex;
    gap: 2px;
    border-bottom: 1px solid var(--border);
    margin-bottom: 16px;
    overflow-x: auto;
  }
  .tabs button {
    display: flex;
    align-items: center;
    gap: 6px;
    border: none;
    border-bottom: 2px solid transparent;
    border-radius: 0;
    background: none;
    padding: 6px 12px 7px;
    color: var(--muted);
    white-space: nowrap;
  }
  .tabs button:hover {
    color: var(--text);
    background: none;
  }
  .tabs button.on {
    color: var(--text);
    border-bottom-color: var(--accent);
    font-weight: 600;
  }
  .banner {
    display: flex;
    align-items: center;
    gap: 8px;
    padding: 6px 10px;
    margin-bottom: 14px;
    border: 1px solid var(--warn);
    border-radius: 8px;
    color: var(--warn);
    font-size: 13px;
  }
  .banner button {
    display: flex;
    padding: 3px;
  }
</style>
