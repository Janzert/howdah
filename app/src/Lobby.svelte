<script lang="ts">
  // The main window: a sidebar (the screens, the open game windows, and
  // the app-wide dialogs: Engines, Settings, Help; it folds to icons and
  // boards) beside one screen: Home (new games, an analysis board, opening
  // a record, the position editor and puzzles, and the open windows),
  // arimaa.com (lib/gameroom/) or Engine tests. It has no board or session
  // of its own; every game opens in a game window (docs/WINDOWS.md).
  // Engine tests run in their own daemon (docs/TOURNAMENTS.md).
  // Notifications about arimaa.com (invitations, postal moves due) live
  // here, so they come once, also while it's hidden.
  import { onMount } from 'svelte';
  import { analysisEngine } from './lib/analysis';
  import { type Api, api, apiFor, errorMessage } from './lib/api';
  import { setAppearance } from './lib/appearance';
  import { requestAttention } from './lib/attention';
  import type { EngineSpec } from './lib/bindings/EngineSpec';
  import type { GameroomGames } from './lib/bindings/GameroomGames';
  import type { MatchSpec } from './lib/bindings/MatchSpec';
  import MiniBoard from './lib/board/MiniBoard.svelte';
  import EnginesDialog from './lib/EnginesDialog.svelte';
  import { on } from './lib/events';
  import { myTurn } from './lib/gameroom';
  import GameroomScreen from './lib/gameroom/GameroomScreen.svelte';
  import { gameroom, pref, savePref } from './lib/gameroom/store.svelte';
  import HelpDialog from './lib/HelpDialog.svelte';
  import HomeScreen from './lib/HomeScreen.svelte';
  import Icon, { type IconName } from './lib/Icon.svelte';
  import { listWindow } from './lib/lobbyWindows';
  import NewGameDialog from './lib/NewGameDialog.svelte';
  import { OpenGames } from './lib/openGames.svelte';
  import PuzzlesDialog from './lib/PuzzlesDialog.svelte';
  import RecordDialog from './lib/RecordDialog.svelte';
  import { settings } from './lib/settings.svelte';
  import SettingsDialog from './lib/SettingsDialog.svelte';
  import { play, setMuted, setVolume, unlockOnInteraction } from './lib/sound';
  import TestsScreen from './lib/TestsScreen.svelte';
  import { findTheme } from './lib/theme';
  import { focusGameWindow, openGameWindow } from './lib/windows';
  import './lobby.css';

  type Screen = 'home' | 'gameroom' | 'tests';
  const SCREENS: Screen[] = ['home', 'gameroom', 'tests'];

  let screen = $state<Screen>(SCREENS.find((s) => s === pref('lobby.screen')) ?? 'home');
  let collapsed = $state(pref('lobby.sidebar') === 'collapsed');

  function show(s: Screen) {
    screen = s;
    savePref('lobby.screen', s);
  }

  /** The arimaa.com screen, at its overview. */
  function showGameroom() {
    gameroom.tab = 'overview';
    show('gameroom');
  }

  function setCollapsed(on: boolean) {
    collapsed = on;
    savePref('lobby.sidebar', on ? 'collapsed' : '');
  }

  let engines = $state<EngineSpec[]>([]);
  /** A passing message, with a button that acts on it. */
  let message = $state<{ text: string; action?: { label: string; run: () => void } } | null>(null);
  let messageTimer: ReturnType<typeof setTimeout> | undefined;
  let showNewGame = $state(false);
  let showPuzzles = $state(false);
  let showEngines = $state(false);
  let showSettings = $state(false);
  let showHelp = $state(false);
  let showRecord = $state(false);
  /** Engine test games playing now. */
  let testsPlaying = $state(0);

  /** The open game windows. */
  const games = new OpenGames();
  let now = $state(performance.now());

  /** Open invitations to the user, and those already announced. */
  let invitationCount = $state(0);
  const seenInvitations = new Set<string>();
  /** The user's postal games waiting on their move without a window. */
  let postalTurns = $state<string[]>([]);

  const theme = $derived(findTheme(settings.theme));

  $effect(() => setAppearance(settings.appearance));
  $effect(() => setMuted(!settings.sound));
  $effect(() => setVolume(settings.volume));

  // Clocks tick while any game's runs.
  $effect(() => {
    if (!Object.values(games.views).some((v) => v.view.clock?.running)) return;
    const timer = setInterval(() => (now = performance.now()), 500);
    return () => clearInterval(timer);
  });

  function flash(text: string, action?: { label: string; run: () => void }) {
    message = { text, action };
    clearTimeout(messageTimer);
    messageTimer = setTimeout(() => (message = null), action ? 8000 : 3500);
  }

  async function reloadEngines() {
    engines = await api.listEngines();
  }

  /** Opens a game window set up by `prepare`; resolves to an error
   * message, or null. */
  async function openWindow(
    prepare?: (a: Api) => Promise<void>,
    spec: MatchSpec | null = null,
  ): Promise<string | null> {
    try {
      await openGameWindow(prepare, { spec });
      return null;
    } catch (e) {
      return errorMessage(e);
    }
  }

  /** Opens a game window following engine test `id`, on its newest
   * running game without a window; resolves to an error message, or null. */
  async function followTest(id: string): Promise<string | null> {
    try {
      const prepare = async (a: Api) => {
        if (!(await a.followTourneyTest(id))) throw new Error('Every game playing now has a window');
      };
      await openGameWindow(prepare, { spec: null, follow: true });
      return null;
    } catch (e) {
      return errorMessage(e);
    }
  }

  /** Opens an arimaa.com game, unless it has a window already (one window
   * per game): then that window comes forward, taking the user's seat
   * first if they asked to play and it only watches. */
  async function openGameroom(
    start: (a: Api) => Promise<void>,
    game?: { gid: string; play: boolean },
  ): Promise<string | null> {
    const id = game ? await api.gameroomGameSession(game.gid).catch(() => null) : null;
    if (id == null) return openWindow(start);
    try {
      if (game?.play && games.watches[id]?.side == null) await start(apiFor(id));
      await focusGameWindow(id);
      return null;
    } catch (e) {
      return errorMessage(e);
    }
  }

  function startGame(spec: MatchSpec): Promise<string | null> {
    const free =
      spec.gold.kind === 'human' && spec.silver.kind === 'human' && !spec.goldTimeControl && !spec.silverTimeControl;
    if (free) return openWindow();
    return openWindow((a) => a.startMatch(spec), spec).then((error) => {
      if (!error) play('gameStart');
      return error;
    });
  }

  /** The position editor in a new window, starting from both default setups. */
  async function positionEditor() {
    try {
      await openGameWindow(undefined, { spec: null, editor: await api.defaultPosition() });
    } catch (e) {
      flash(errorMessage(e));
    }
  }

  async function analysisBoard() {
    const engine = analysisEngine(engines, settings.analysisEngine);
    const error = await openWindow(engine ? (a) => a.setAnalysis(engine.id) : undefined);
    if (error) flash(error);
    else if (!engine) flash('Add an engine to analyse with (Engines)');
  }


  onMount(() => {
    unlockOnInteraction();
    const stopFollowing = games.follow();
    gameroom.setOpener(openGameroom);
    const stopGameroom = gameroom.start(lobbyGames);
    // The store passes on the lobby poll's lists (`lobbyGames`).
    const unlisteners = [
      on('gameroom://invitation', async (a) => {
        play('notification');
        if (a.outcome.kind === 'accepted') {
          const { gid, side } = a.outcome;
          flash(`${a.opponent} accepted your invitation`);
          requestAttention('Invitation accepted');
          const error = await openGameroom((s) => s.playGameroomGame(gid, side), { gid, play: true });
          if (error) flash(error);
        } else if (a.outcome.kind === 'declined') {
          flash(a.outcome.message);
        } else {
          flash(`Your invitation to ${a.opponent} is gone`);
        }
      }),
    ];
    api.gameroomLastGames().then((g) => g && lobbyGames(g));
    reloadEngines();
    return () => {
      stopFollowing();
      stopGameroom();
      unlisteners.forEach((u) => void u.then((f) => f()));
    };
  });

  /** Counts the invitations to the user and their postal games waiting on
   * their move (leaving out those open in a window), and announces new
   * ones. */
  function lobbyGames(g: GameroomGames) {
    const incoming = g.invitations.filter((i) => i.incoming);
    const fresh = incoming.filter((i) => !seenInvitations.has(i.created));
    invitationCount = incoming.length;
    for (const i of incoming) seenInvitations.add(i.created);
    if (fresh.length > 0) {
      flash(`${fresh[0].opponent ?? 'Someone'} invites you to a game (arimaa.com)`, {
        label: 'Show',
        run: showGameroom,
      });
      play('notification');
      requestAttention('Invitation');
    }
    const open = games.openGids();
    const turns = g.mine.filter((m) => m.postal && myTurn(m, g.user) && !open.has(m.gid)).map((m) => m.gid);
    const newTurns = turns.filter((gid) => !postalTurns.includes(gid));
    postalTurns = turns;
    if (newTurns.length > 0 && fresh.length === 0) {
      flash(
        newTurns.length === 1
          ? 'A postal game waits on your move (arimaa.com)'
          : `${newTurns.length} postal games wait on your move (arimaa.com)`,
        { label: 'Show', run: showGameroom },
      );
      play('notification');
      requestAttention('Your postal move');
    }
  }

  /** The open windows, those waiting on the user's move first. */
  const listed = $derived(
    games.ids
      .filter((id) => games.views[id])
      .map((id) => listWindow(id, games.views[id], games.watches[id], now))
      .sort((a, b) => Number(b.yourMove) - Number(a.yourMove)),
  );

  const anyDialog = $derived(showNewGame || showEngines || showSettings || showRecord || showHelp || showPuzzles);

  function onkeydown(e: KeyboardEvent) {
    if (anyDialog || gameroom.newGame) return;
    if ((e.target as HTMLElement).closest('input, textarea, select')) return;
    if (e.key === '?') {
      showHelp = true;
      e.preventDefault();
    }
  }

  const nav = $derived<{ id: Screen; label: string; icon: IconName; badge: number; badgeTitle: string }[]>([
    { id: 'home', label: 'Home', icon: 'home', badge: 0, badgeTitle: '' },
    {
      id: 'gameroom',
      label: 'arimaa.com',
      icon: 'globe',
      badge: invitationCount + postalTurns.length,
      badgeTitle: 'Invitations and postal games waiting on you',
    },
    { id: 'tests', label: 'Engine tests', icon: 'flask', badge: testsPlaying, badgeTitle: 'Test games playing now' },
  ]);
</script>

<svelte:window {onkeydown} />

<div class="lobby" class:collapsed>
  <nav class="sidebar" aria-label="Lobby">
    <div class="brand">
      {#if !collapsed}<span class="app-name">Howdah</span>{/if}
      <button
        class="bare"
        onclick={() => setCollapsed(!collapsed)}
        title={collapsed ? 'Expand the sidebar' : 'Collapse the sidebar'}
        aria-label={collapsed ? 'Expand the sidebar' : 'Collapse the sidebar'}
        aria-expanded={!collapsed}
      >
        <Icon name={collapsed ? 'expand' : 'collapse'} />
      </button>
    </div>

    <ul class="nav">
      {#each nav as item (item.id)}
        <li>
          <button
            class="nav-item bare"
            class:on={screen === item.id}
            aria-current={screen === item.id ? 'page' : undefined}
            onclick={() => show(item.id)}
            title={collapsed ? item.label : undefined}
            aria-label={item.label}
          >
            <Icon name={item.icon} />
            {#if !collapsed}<span class="label">{item.label}</span>{/if}
            {#if item.badge > 0}<span class="badge" title={item.badgeTitle}>{item.badge}</span>{/if}
          </button>
        </li>
      {/each}
    </ul>

    <div class="windows-head">
      {#if collapsed}<span class="rule"></span>{:else}Windows{/if}
    </div>
    {#if listed.length === 0}
      {#if !collapsed}<p class="none">No game windows</p>{/if}
    {:else}
      <ul class="windows" aria-label="Game windows">
        {#each listed as w (w.id)}
          <li>
            <button
              class="window bare"
              class:waiting={w.yourMove}
              onclick={() => focusGameWindow(w.id)}
              title="{w.title} ({w.kind}){w.yourMove ? ': your move' : ''}"
              aria-label="Show {w.title}"
            >
              <span class="mini">
                <MiniBoard position={w.view.position} {theme} flipped={false} />
                {#if w.yourMove}<span class="move-dot" aria-hidden="true"></span>{/if}
              </span>
              {#if !collapsed}
                <span class="window-text">
                  <span class="window-title">{w.title}</span>
                  <span class="window-meta">{w.yourMove ? 'Your move' : w.kind}</span>
                </span>
              {/if}
            </button>
          </li>
        {/each}
      </ul>
    {/if}

    <div class="tools">
      <button class="nav-item bare" onclick={() => (showEngines = true)} title={collapsed ? 'Engines' : undefined} aria-label="Engines">
        <Icon name="cpu" />{#if !collapsed}<span class="label">Engines</span>{/if}
      </button>
      <button class="nav-item bare" onclick={() => (showSettings = true)} title={collapsed ? 'Settings' : undefined} aria-label="Settings">
        <Icon name="gear" />{#if !collapsed}<span class="label">Settings</span>{/if}
      </button>
      <button class="nav-item bare" onclick={() => (showHelp = true)} title="Keyboard and mouse help (?)" aria-label="Help">
        <Icon name="help" />{#if !collapsed}<span class="label">Help</span>{/if}
      </button>
    </div>
  </nav>

  <main>
    {#if screen === 'home'}
      <HomeScreen
        windows={listed}
        {theme}
        invitations={invitationCount}
        postalTurns={postalTurns.length}
        onNewGame={() => (showNewGame = true)}
        onAnalysis={analysisBoard}
        onRecord={() => (showRecord = true)}
        onEditor={positionEditor}
        onPuzzles={() => (showPuzzles = true)}
        onWindow={(w) => focusGameWindow(w.id)}
        onGameroom={() => showGameroom()}
      />
    {:else if screen === 'gameroom'}
      <GameroomScreen />
    {/if}
    <!-- Kept while hidden: it follows the daemon's events for the badge. -->
    <div hidden={screen !== 'tests'}>
      <TestsScreen
        {engines}
        onOpenGame={(test, pair, game) => openWindow((a) => a.openTourneyGame(test, pair, game))}
        onFollow={followTest}
        onPlaying={(n) => (testsPlaying = n)}
      />
    </div>
  </main>

  {#if message}
    <div class="message" role="status">
      {message.text}
      {#if message.action}
        {@const action = message.action}
        <button
          onclick={() => {
            action.run();
            message = null;
          }}>{action.label}</button
        >
      {/if}
    </div>
  {/if}
</div>

{#if showPuzzles}
  <PuzzlesDialog onOpen={(id) => openWindow((a) => a.openPuzzle(id))} onClose={() => (showPuzzles = false)} />
{/if}
{#if showNewGame}
  <NewGameDialog
    {engines}
    onStart={startGame}
    onClose={() => (showNewGame = false)}
    onManageEngines={() => {
      showNewGame = false;
      showEngines = true;
    }}
  />
{/if}
{#if showEngines}
  <EnginesDialog
    onChanged={reloadEngines}
    onClose={() => {
      showEngines = false;
      reloadEngines();
    }}
  />
{/if}
{#if showSettings}
  <SettingsDialog onClose={() => (showSettings = false)} />
{/if}
{#if showHelp}
  <HelpDialog onClose={() => (showHelp = false)} />
{/if}
{#if showRecord}
  <RecordDialog
    initial=""
    onLoad={(text) => openWindow((a) => a.loadGame(text))}
    onClose={() => (showRecord = false)}
  />
{/if}

<style>
  .lobby {
    display: grid;
    grid-template-columns: 232px 1fr;
    height: 100vh;
  }
  .lobby.collapsed {
    grid-template-columns: 64px 1fr;
  }
  .sidebar {
    display: flex;
    flex-direction: column;
    gap: 2px;
    min-height: 0;
    padding: 10px 8px;
    border-right: 1px solid var(--border);
    background: var(--panel);
    overflow: hidden;
  }
  button.bare {
    border: none;
    background: none;
    border-radius: 7px;
  }
  button.bare:hover:not(:disabled) {
    background: var(--hover);
  }
  .brand {
    display: flex;
    align-items: center;
    justify-content: space-between;
    padding: 0 0 8px 8px;
    min-height: 32px;
  }
  .collapsed .brand {
    justify-content: center;
    padding-left: 0;
  }
  .brand button {
    display: flex;
    padding: 6px;
    color: var(--muted);
  }
  .app-name {
    font-weight: 700;
    font-size: 17px;
  }
  .nav,
  .windows {
    list-style: none;
    margin: 0;
    padding: 0;
  }
  .nav-item {
    display: flex;
    align-items: center;
    gap: 10px;
    width: 100%;
    padding: 7px 10px;
    font-size: 14px;
    text-align: left;
    color: var(--text);
    position: relative;
  }
  .collapsed .nav-item {
    justify-content: center;
    padding: 9px 0;
  }
  .nav-item.on {
    background: var(--hover);
    font-weight: 600;
  }
  .nav-item.on::before {
    content: '';
    position: absolute;
    left: -8px;
    top: 6px;
    bottom: 6px;
    width: 3px;
    border-radius: 0 3px 3px 0;
    background: var(--accent);
  }
  .label {
    flex: 1;
    overflow: hidden;
    white-space: nowrap;
    text-overflow: ellipsis;
  }
  .collapsed .nav-item .badge {
    position: absolute;
    top: 2px;
    right: 6px;
    padding: 0 4px;
    font-size: 10px;
    line-height: 14px;
  }
  .windows-head {
    margin: 14px 0 4px;
    padding: 0 10px;
    font-size: 11px;
    font-weight: 600;
    letter-spacing: 0.04em;
    text-transform: uppercase;
    color: var(--muted);
  }
  .collapsed .windows-head {
    padding: 0 6px;
  }
  .rule {
    display: block;
    border-top: 1px solid var(--border);
  }
  .none {
    margin: 0;
    padding: 2px 10px;
    font-size: 12px;
    color: var(--muted);
  }
  .windows {
    flex: 1;
    min-height: 0;
    overflow-y: auto;
    display: flex;
    flex-direction: column;
    gap: 2px;
  }
  .window {
    display: flex;
    align-items: center;
    gap: 9px;
    width: 100%;
    padding: 5px 6px;
    text-align: left;
    color: var(--text);
  }
  .collapsed .window {
    justify-content: center;
    padding: 4px 0;
  }
  .mini {
    position: relative;
    flex: none;
    width: 40px;
    height: 40px;
  }
  .collapsed .mini {
    width: 44px;
    height: 44px;
  }
  .move-dot {
    position: absolute;
    top: -3px;
    right: -3px;
    width: 11px;
    height: 11px;
    border-radius: 50%;
    background: var(--accent);
    border: 2px solid var(--panel);
  }
  .window-text {
    display: flex;
    flex-direction: column;
    min-width: 0;
  }
  .window-title {
    font-size: 13px;
    overflow: hidden;
    white-space: nowrap;
    text-overflow: ellipsis;
  }
  .window-meta {
    font-size: 11px;
    color: var(--muted);
    overflow: hidden;
    white-space: nowrap;
    text-overflow: ellipsis;
  }
  .window.waiting .window-meta {
    color: var(--accent);
    font-weight: 600;
  }
  .tools {
    margin-top: auto;
    padding-top: 8px;
    border-top: 1px solid var(--border);
  }
  .windows + .tools {
    margin-top: 8px;
  }
  main {
    min-width: 0;
    overflow-y: auto;
    padding: 18px 24px 32px;
  }
  .message {
    position: fixed;
    left: 50%;
    bottom: 20px;
    transform: translateX(-50%);
    display: flex;
    align-items: center;
    gap: 10px;
    padding: 8px 10px 8px 14px;
    border-radius: 8px;
    background: var(--text);
    color: var(--bg);
    font-size: 13px;
    max-width: 80vw;
    box-shadow: 0 4px 16px rgba(0, 0, 0, 0.25);
  }
  .message button {
    padding: 2px 10px;
  }
</style>
