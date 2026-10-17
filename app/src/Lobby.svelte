<script lang="ts">
  // The main window: a sidebar beside one panel. The sidebar (it folds to
  // icons and boards) lists the panels: Home (new games, an analysis
  // board, opening a record, the position editor and puzzles), arimaa.com
  // (lib/gameroom/) with the arimaa.com panels opened from its overview
  // under it, Engine tests with the tests opened under it, the open
  // boards, and Engines, Settings and Help. A board is a session: docked
  // here as a panel (an iframe of the game window's page; lib/frames.ts)
  // or in a game window of its own, and it can move between the two
  // (docs/WINDOWS.md). Engine tests run in their own daemon
  // (docs/TOURNAMENTS.md). Notifications about arimaa.com (invitations,
  // postal moves due) live here, so they come once, also while it's
  // hidden.
  import { onMount, tick } from 'svelte';
  import { analysisEngine } from './lib/analysis';
  import { type Api, api, apiFor, errorMessage } from './lib/api';
  import { setAppearance } from './lib/appearance';
  import { requestAttention } from './lib/attention';
  import type { EngineSpec } from './lib/bindings/EngineSpec';
  import type { GameroomGames } from './lib/bindings/GameroomGames';
  import type { MatchSpec } from './lib/bindings/MatchSpec';
  import type { SessionId } from './lib/bindings/SessionId';
  import MiniBoard from './lib/board/MiniBoard.svelte';
  import CloseWindowDialog from './lib/CloseWindowDialog.svelte';
  import EnginesDialog from './lib/EnginesDialog.svelte';
  import { on } from './lib/events';
  import { type FrameRequest, requestClose, serveFrames } from './lib/frames';
  import { myTurn } from './lib/gameroom';
  import GameroomPanel from './lib/gameroom/GameroomPanel.svelte';
  import GameroomScreen from './lib/gameroom/GameroomScreen.svelte';
  import NewGameroomGameDialog from './lib/gameroom/NewGameDialog.svelte';
  import { GAMEROOM_PANELS, type GameroomTab, gameroom, pref, savePref } from './lib/gameroom/store.svelte';
  import HelpDialog from './lib/HelpDialog.svelte';
  import HomeScreen from './lib/HomeScreen.svelte';
  import Icon, { type IconName } from './lib/Icon.svelte';
  import { type ListedWindow, listWindow } from './lib/lobbyWindows';
  import NewGameDialog from './lib/NewGameDialog.svelte';
  import { OpenGames } from './lib/openGames.svelte';
  import PuzzlesDialog from './lib/PuzzlesDialog.svelte';
  import RecordDialog from './lib/RecordDialog.svelte';
  import { settings } from './lib/settings.svelte';
  import SettingsPanel from './lib/SettingsPanel.svelte';
  import { play, setMuted, setVolume, unlockOnInteraction } from './lib/sound';
  import TestPanel from './lib/TestPanel.svelte';
  import { llrPosition, scoreText, testState } from './lib/tests';
  import TestsScreen from './lib/TestsScreen.svelte';
  import { findTheme } from './lib/theme';
  import { tourney } from './lib/tourney.svelte';
  import { matchGoesOn } from './lib/windowList';
  import { focusGameWindow, inTauri, openBoard, openWindowFor, showBoard } from './lib/windows';
  import './lobby.css';

  /** The panel shown: `home`, `gameroom`, `gameroom:<panel>`, `tests`,
   * `test:<id>`, `board:<session>`, `engines`, `settings` or `help`. */
  let selected = $state(pref('lobby.panel') || 'home');
  let collapsed = $state(pref('lobby.sidebar') === 'collapsed');

  function readList(key: string): string[] {
    try {
      const v = JSON.parse(pref(key) || '[]');
      return Array.isArray(v) ? v.filter((x) => typeof x === 'string') : [];
    } catch {
      return [];
    }
  }

  /** The arimaa.com panels and tests opened, as the sidebar lists them. */
  let gameroomOpen = $state(
    readList('lobby.gameroomPanels').filter((p): p is GameroomTab => GAMEROOM_PANELS.some((g) => g.id === p)),
  );
  let testsOpen = $state(readList('lobby.testPanels'));
  $effect(() => savePref('lobby.gameroomPanels', JSON.stringify(gameroomOpen)));
  $effect(() => savePref('lobby.testPanels', JSON.stringify(testsOpen)));

  /** The panel shown before the current one, to go back to when it
   * closes. */
  let previous = 'home';

  function select(key: string) {
    if (key !== selected) previous = selected;
    selected = key;
    // Boards come and go with the backend's sessions; don't come back to one.
    if (!key.startsWith('board:')) savePref('lobby.panel', key);
    if (key.startsWith('board:')) void tick().then(() => frames[Number(key.slice(6))]?.contentWindow?.focus());
  }

  /** Leaves panel `key` if it's shown, for the one shown before (or
   * `fallback`). */
  function leave(key: string, fallback: string) {
    if (selected !== key) return;
    selected = previous !== key && exists(previous) ? previous : fallback;
    savePref('lobby.panel', selected.startsWith('board:') ? fallback : selected);
  }

  /** Whether panel `key` can be shown now. */
  function exists(key: string): boolean {
    if (key.startsWith('gameroom:')) return gameroomOpen.includes(key.slice(9) as GameroomTab);
    if (key.startsWith('test:')) return testsOpen.includes(key.slice(5));
    if (key.startsWith('board:')) return games.docked.includes(Number(key.slice(6)));
    return ['home', 'gameroom', 'tests', 'engines', 'settings', 'help'].includes(key);
  }

  function openGameroomPanel(panel: GameroomTab) {
    if (!gameroomOpen.includes(panel)) gameroomOpen = [...gameroomOpen, panel];
    select(`gameroom:${panel}`);
  }

  function closeGameroomPanel(panel: GameroomTab) {
    gameroomOpen = gameroomOpen.filter((p) => p !== panel);
    leave(`gameroom:${panel}`, 'gameroom');
  }

  function openTest(id: string) {
    if (!testsOpen.includes(id)) testsOpen = [...testsOpen, id];
    select(`test:${id}`);
  }

  function closeTest(id: string) {
    testsOpen = testsOpen.filter((t) => t !== id);
    leave(`test:${id}`, 'tests');
  }

  /** The arimaa.com panel, at its overview. */
  function showGameroom() {
    select('gameroom');
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
  let showRecord = $state(false);
  /** The question before the main window closes on boards still playing. */
  let closing = $state<{ reason: string; answer: (close: boolean) => void } | null>(null);

  /** The open boards. */
  const games = new OpenGames();
  let now = $state(performance.now());
  /** Each docked board's frame, by session. */
  const frames: Record<SessionId, HTMLIFrameElement> = $state({});

  /** Open invitations to the user, and those already announced. */
  let invitationCount = $state(0);
  const seenInvitations = new Set<string>();
  /** The user's postal games waiting on their move without a board. */
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

  // A board docked since the last change (opened here, or docked from its
  // window) is shown; one that's gone (closed, popped out) isn't.
  let knownDocked: Set<SessionId> | null = null;
  $effect(() => {
    const docked = games.docked;
    if (knownDocked) {
      const fresh = docked.filter((id) => !knownDocked!.has(id));
      if (fresh.length > 0) select(`board:${fresh[fresh.length - 1]}`);
    }
    knownDocked = new Set(docked);
    if (selected.startsWith('board:') && !docked.includes(Number(selected.slice(6)))) leave(selected, 'home');
  });

  // Tests archived or deleted elsewhere close their panels.
  $effect(() => {
    if (!tourney.status?.connected) return;
    const gone = testsOpen.filter((id) => !tourney.test(id));
    if (gone.length > 0 && tourney.tests.length > 0) gone.forEach(closeTest);
  });

  function flash(text: string, action?: { label: string; run: () => void }) {
    message = { text, action };
    clearTimeout(messageTimer);
    messageTimer = setTimeout(() => (message = null), action ? 8000 : 3500);
  }

  async function reloadEngines() {
    engines = await api.listEngines();
  }

  /** Opens a board set up by `prepare` (a panel or a window, as the
   * Settings say); resolves to an error message, or null. */
  async function openNew(
    prepare?: (a: Api) => Promise<void>,
    spec: MatchSpec | null = null,
  ): Promise<string | null> {
    try {
      await openBoard(prepare, { spec });
      return null;
    } catch (e) {
      return errorMessage(e);
    }
  }

  /** Opens a board following engine test `id`, on its newest running game
   * without a board; resolves to an error message, or null. */
  async function followTest(id: string): Promise<string | null> {
    try {
      const prepare = async (a: Api) => {
        if (!(await a.followTourneyTest(id))) throw new Error('Every game playing now has a board');
      };
      await openBoard(prepare, { spec: null, follow: true });
      return null;
    } catch (e) {
      return errorMessage(e);
    }
  }

  /** Opens an arimaa.com game, unless it has a board already (one board
   * per game): then that board is shown, taking the user's seat first if
   * they asked to play and it only watches. */
  async function openGameroom(
    start: (a: Api) => Promise<void>,
    game?: { gid: string; play: boolean },
  ): Promise<string | null> {
    const id = game ? await api.gameroomGameSession(game.gid).catch(() => null) : null;
    if (id == null) return openNew(start);
    try {
      if (game?.play && games.watches[id]?.side == null) await start(apiFor(id));
      showListed(id);
      return null;
    } catch (e) {
      return errorMessage(e);
    }
  }

  function startGame(spec: MatchSpec): Promise<string | null> {
    const free =
      spec.gold.kind === 'human' && spec.silver.kind === 'human' && !spec.goldTimeControl && !spec.silverTimeControl;
    if (free) return openNew();
    return openNew((a) => a.startMatch(spec), spec).then((error) => {
      if (!error) play('gameStart');
      return error;
    });
  }

  /** The position editor on a new board, starting from both default setups. */
  async function positionEditor() {
    try {
      await openBoard(undefined, { spec: null, editor: await api.defaultPosition() });
    } catch (e) {
      flash(errorMessage(e));
    }
  }

  async function analysisBoard() {
    const engine = analysisEngine(engines, settings.analysisEngine);
    const error = await openNew(engine ? (a) => a.setAnalysis(engine.id) : undefined);
    if (error) flash(error);
    else if (!engine) flash('Add an engine to analyse with (Engines)');
  }

  /** Shows board `id`: its panel, or its window. */
  function showListed(id: SessionId) {
    if (games.docked.includes(id)) select(`board:${id}`);
    else void focusGameWindow(id);
  }

  /** Closes docked board `id`, after its own question if closing would
   * stop a match or leave a game. */
  function closeDocked(id: SessionId) {
    const frame = frames[id];
    if (!frame) return;
    select(`board:${id}`);
    requestClose(frame);
  }

  /** What a docked board asks for. */
  async function frameRequest(session: SessionId, request: FrameRequest) {
    try {
      if (request.kind === 'show') {
        if (!request.fresh) await showBoard(request.session);
        else if (settings.boardsInWindows) await openWindowFor(request.session);
        else await api.setSessionDocked(request.session, true);
      } else if (request.kind === 'popOut') {
        await api.setSessionDocked(session, false);
        // The frame goes first, so only the window shows the board.
        await tick();
        await openWindowFor(session).catch(async (e) => {
          await api.setSessionDocked(session, true);
          throw e;
        });
      } else if (request.kind === 'close') {
        await api.closeSession(session);
      }
    } catch (e) {
      flash(errorMessage(e));
    }
  }

  /** Why closing the main window needs asking: docked boards with a match
   * or an arimaa.com game going on, which closing (and quitting) ends. */
  function closeWarning(): string | null {
    const playing = games.docked.filter((id) => games.views[id] && matchGoesOn(games.views[id].view));
    if (playing.length === 0) return null;
    const one = playing.length === 1;
    return `${one ? 'A board' : `${playing.length} boards`} here ${one ? 'has' : 'have'} a match or an arimaa.com game going on. Quitting Howdah stops ${one ? 'it' : 'them'} (an arimaa.com game goes on there, with your clock running).`;
  }

  /** In Tauri: asks before Howdah quits with docked boards still playing:
   * when the main window closes with no game windows open (with some, it
   * only hides, as `lib.rs` does, and the boards go on), and when the last
   * game window closes while it's hidden (`lib.rs` leaves that to it,
   * `lobby://last-window-closed`): then it comes back to ask. */
  async function guardMainWindow() {
    if (!inTauri) return;
    const { getCurrentWindow } = await import('@tauri-apps/api/window');
    const { getAllWebviewWindows } = await import('@tauri-apps/api/webviewWindow');
    const { listen } = await import('@tauri-apps/api/event');
    const win = getCurrentWindow();
    const quit = async (reason: string | null) => {
      if (reason && !(await new Promise<boolean>((answer) => (closing = { reason, answer })))) return;
      await win.destroy();
    };
    await win.onCloseRequested(async (e) => {
      e.preventDefault();
      if ((await getAllWebviewWindows()).some((w) => w.label.startsWith('game-'))) {
        await win.hide();
        return;
      }
      await quit(closeWarning());
    });
    await listen('lobby://last-window-closed', async () => {
      const reason = closeWarning();
      if (reason) {
        await win.show();
        await win.unminimize();
        await win.setFocus();
      }
      await quit(reason);
    });
  }

  onMount(() => {
    unlockOnInteraction();
    const stopFollowing = games.follow();
    const stopTourney = tourney.start();
    gameroom.setOpener(openGameroom);
    gameroom.setNavigator(openGameroomPanel);
    // The store passes on the lobby poll's lists (`lobbyGames`).
    const stopGameroom = gameroom.start(lobbyGames);
    const stopFrames = serveFrames(
      () => games.docked.filter((id) => frames[id]).map((id) => ({ session: id, frame: frames[id] })),
      frameRequest,
    );
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
      // A board's Next game, from a game window.
      on('sessions://show', (s) => {
        if (games.docked.includes(s.board)) select(`board:${s.board}`);
      }),
    ];
    api.gameroomLastGames().then((g) => g && lobbyGames(g));
    reloadEngines();
    void guardMainWindow();
    if (!exists(selected)) selected = 'home';
    return () => {
      stopFollowing();
      stopTourney();
      stopGameroom();
      stopFrames();
      unlisteners.forEach((u) => void u.then((f) => f()));
    };
  });

  /** Counts the invitations to the user and their postal games waiting on
   * their move (leaving out those open on a board), and announces new
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

  /** The open boards, those waiting on the user's move first. */
  const listed = $derived(
    games.ids
      .filter((id) => games.views[id])
      .map((id) => listWindow(id, games.views[id], games.watches[id], now))
      .sort((a, b) => Number(b.yourMove) - Number(a.yourMove)),
  );
  const isDockedBoard = (w: ListedWindow) => games.docked.includes(w.id);

  const anyDialog = $derived(showNewGame || showRecord || showPuzzles || closing != null || gameroom.newGame != null);

  function onkeydown(e: KeyboardEvent) {
    if (anyDialog) return;
    if ((e.target as HTMLElement).closest('input, textarea, select')) return;
    if (e.key === '?') {
      select('help');
      e.preventDefault();
    }
  }

  const nav = $derived<
    { id: string; label: string; icon: IconName; badge: number; badgeTitle: string; offline?: boolean }[]
  >([
    { id: 'home', label: 'Home', icon: 'home', badge: 0, badgeTitle: '' },
    {
      id: 'gameroom',
      label: 'arimaa.com',
      icon: 'globe',
      badge: invitationCount + postalTurns.length,
      badgeTitle: 'Invitations and postal games waiting on you',
      // Known to be logged out (not while the backend hasn't said yet).
      offline: gameroom.user === null,
    },
    { id: 'tests', label: 'Engine tests', icon: 'flask', badge: tourney.playing, badgeTitle: 'Test games playing now' },
  ]);

  const tools: { id: string; label: string; icon: IconName }[] = [
    { id: 'engines', label: 'Engines', icon: 'cpu' },
    { id: 'settings', label: 'Settings', icon: 'gear' },
    { id: 'help', label: 'Help', icon: 'help' },
  ];
</script>

<svelte:window {onkeydown} />

{#snippet navItem(item: {
  id: string;
  label: string;
  icon: IconName;
  badge?: number;
  badgeTitle?: string;
  offline?: boolean;
})}
  <button
    class="nav-item bare"
    class:on={selected === item.id}
    aria-current={selected === item.id ? 'page' : undefined}
    onclick={() => select(item.id)}
    title={collapsed ? `${item.label}${item.offline ? ' (not logged in)' : ''}` : undefined}
    aria-label={item.offline ? `${item.label}, not logged in` : item.label}
  >
    <Icon name={item.icon} />
    {#if !collapsed}<span class="label">{item.label}</span>{/if}
    {#if item.offline}<span class="offline" title="Not logged in"><Icon name="offline" size={16} /></span>{/if}
    {#if item.badge}<span class="badge" title={item.badgeTitle}>{item.badge}</span>{/if}
  </button>
{/snippet}

{#snippet closeButton(label: string, onclick: () => void)}
  {#if !collapsed}
    <button class="close bare" {onclick} title={label} aria-label={label}><Icon name="close" size={13} /></button>
  {/if}
{/snippet}

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

    <div class="scroll">
      <ul class="nav">
        {#each nav as item (item.id)}
          <li>
            {@render navItem(item)}
            {#if item.id === 'gameroom' && gameroomOpen.length > 0}
              <ul class="sub" aria-label="arimaa.com panels">
                {#each gameroomOpen as id (id)}
                  {@const p = GAMEROOM_PANELS.find((g) => g.id === id)!}
                  <li class="sub-row" class:on={selected === `gameroom:${id}`}>
                    <button
                      class="sub-item bare"
                      aria-current={selected === `gameroom:${id}` ? 'page' : undefined}
                      onclick={() => select(`gameroom:${id}`)}
                      title={collapsed ? `arimaa.com: ${p.label}` : undefined}
                      aria-label="arimaa.com: {p.label}"
                    >
                      <Icon name={p.icon} size={16} />
                      {#if !collapsed}<span class="label">{p.label}</span>{/if}
                    </button>
                    {@render closeButton(`Close ${p.label}`, () => closeGameroomPanel(id))}
                  </li>
                {/each}
              </ul>
            {/if}
            {#if item.id === 'tests' && testsOpen.length > 0}
              <ul class="sub" aria-label="Engine tests open">
                {#each testsOpen as id (id)}
                  {@const t = tourney.test(id)}
                  {@const pos = t ? llrPosition(t) : null}
                  <li class="sub-row" class:on={selected === `test:${id}`}>
                    <button
                      class="sub-item test bare"
                      aria-current={selected === `test:${id}` ? 'page' : undefined}
                      onclick={() => select(`test:${id}`)}
                      title={collapsed ? (t?.name ?? id) : undefined}
                      aria-label="Engine test {t?.name ?? id}"
                    >
                      {#if collapsed}
                        <span class="test-dot {t ? testState(t) : ''}"><Icon name="flask" size={16} /></span>
                      {:else}
                        <span class="test-text">
                          <span class="test-title">
                            <span class="label">{t?.name ?? id}</span>
                            {#if t}<span class="state {testState(t)}">{testState(t)}</span>{/if}
                          </span>
                          {#if t}
                            <span class="test-meta">{scoreText(t)}{t.running.length > 0 ? `, ${t.running.length} playing` : ''}</span>
                            {#if pos != null}<span class="llr"><span class="mark" style="left: {pos * 100}%"></span></span>{/if}
                          {/if}
                        </span>
                      {/if}
                    </button>
                    {@render closeButton(`Close ${t?.name ?? id}`, () => closeTest(id))}
                  </li>
                {/each}
              </ul>
            {/if}
          </li>
        {/each}
      </ul>

      <div class="boards-head">
        {#if collapsed}<span class="rule"></span>{:else}Open boards{/if}
      </div>
      {#if listed.length === 0}
        {#if !collapsed}<p class="none">No boards open</p>{/if}
      {:else}
        <ul class="boards" aria-label="Open boards">
          {#each listed as w (w.id)}
            {@const docked = isDockedBoard(w)}
            <li class="sub-row" class:on={selected === `board:${w.id}`}>
              <button
                class="board bare"
                class:waiting={w.yourMove}
                aria-current={selected === `board:${w.id}` ? 'page' : undefined}
                onclick={() => showListed(w.id)}
                title="{w.title} ({w.kind}{docked ? '' : ', in its own window'}){w.yourMove ? ': your move' : ''}"
                aria-label="Show {w.title}"
              >
                <span class="mini">
                  <MiniBoard position={w.view.position} {theme} flipped={false} />
                  {#if w.yourMove}<span class="move-dot" aria-hidden="true"></span>{/if}
                </span>
                {#if !collapsed}
                  <span class="board-text">
                    <span class="board-title">{w.title}</span>
                    <span class="board-meta">
                      {#if !docked}<span class="in-window" title="In its own window"><Icon name="popout" size={11} /></span>{/if}
                      {w.yourMove ? 'Your move' : w.kind}
                    </span>
                  </span>
                {/if}
              </button>
              {#if docked}{@render closeButton(`Close ${w.title}`, () => closeDocked(w.id))}{/if}
            </li>
          {/each}
        </ul>
      {/if}
    </div>

    <div class="tools">
      {#each tools as item (item.id)}{@render navItem(item)}{/each}
    </div>
  </nav>

  <main>
    {#if !selected.startsWith('board:')}
      <div class="content">
        {#if selected === 'home'}
          <HomeScreen
            windows={listed}
            {theme}
            docked={games.docked}
            onWindow={(w) => showListed(w.id)}
            invitations={invitationCount}
            postalTurns={postalTurns.length}
            onNewGame={() => (showNewGame = true)}
            onAnalysis={analysisBoard}
            onRecord={() => (showRecord = true)}
            onEditor={positionEditor}
            onPuzzles={() => (showPuzzles = true)}
            onGameroom={showGameroom}
          />
        {:else if selected === 'gameroom'}
          <GameroomScreen />
        {:else if selected.startsWith('gameroom:')}
          {@const panel = selected.slice(9) as GameroomTab}
          {#key panel}<GameroomPanel {panel} onClose={() => closeGameroomPanel(panel)} />{/key}
        {:else if selected === 'tests'}
          <TestsScreen {engines} onOpenTest={openTest} />
        {:else if selected.startsWith('test:')}
          {@const id = selected.slice(5)}
          {#key id}
            <TestPanel
              {id}
              onOpenGame={(test, pair, game) => openNew((a) => a.openTourneyGame(test, pair, game))}
              onFollow={followTest}
              onRemoved={(zip) => {
                tourney.forget(id);
                tourney.notice = zip ? `Archived ${id} to ${zip}` : `Deleted ${id}`;
                closeTest(id);
              }}
              onClose={() => closeTest(id)}
            />
          {/key}
        {:else if selected === 'engines'}
          <EnginesDialog panel onChanged={reloadEngines} />
        {:else if selected === 'settings'}
          <SettingsPanel />
        {:else if selected === 'help'}
          <HelpDialog panel />
        {/if}
      </div>
    {/if}
    {#each games.docked as id (id)}
      <iframe
        class="docked"
        class:shown={selected === `board:${id}`}
        src="index.html?session={id}&docked"
        title={listed.find((w) => w.id === id)?.title ?? 'Board'}
        bind:this={frames[id]}
      ></iframe>
    {/each}
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
  <PuzzlesDialog onOpen={(id) => openNew((a) => a.openPuzzle(id))} onClose={() => (showPuzzles = false)} />
{/if}
{#if showNewGame}
  <NewGameDialog
    {engines}
    onStart={startGame}
    onClose={() => (showNewGame = false)}
    onManageEngines={() => {
      showNewGame = false;
      select('engines');
    }}
  />
{/if}
{#if showRecord}
  <RecordDialog
    initial=""
    onLoad={(text) => openNew((a) => a.loadGame(text))}
    onClose={() => (showRecord = false)}
  />
{/if}
{#if gameroom.newGame}
  <NewGameroomGameDialog initial={gameroom.newGame} onClose={() => (gameroom.newGame = null)} />
{/if}
{#if closing}
  <CloseWindowDialog
    title="Quit Howdah?"
    reason={closing.reason}
    confirm="Quit"
    onAnswer={(close) => {
      closing?.answer(close);
      closing = null;
    }}
  />
{/if}

<style>
  .lobby {
    display: grid;
    grid-template-columns: 240px 1fr;
    height: 100vh;
  }
  .lobby.collapsed {
    grid-template-columns: 64px 1fr;
  }
  .sidebar {
    display: flex;
    flex-direction: column;
    min-height: 0;
    padding: 10px 8px;
    border-right: 1px solid var(--border);
    background: var(--panel);
    overflow: hidden;
  }
  .scroll {
    flex: 1;
    min-height: 0;
    overflow-y: auto;
    overflow-x: hidden;
    margin: 0 -8px;
    padding: 0 8px;
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
  .sub,
  .boards {
    list-style: none;
    margin: 0;
    padding: 0;
  }
  .nav > li + li {
    margin-top: 2px;
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
  .nav-item.on,
  .sub-row.on {
    background: var(--hover);
    border-radius: 7px;
  }
  .nav-item.on {
    font-weight: 600;
  }
  .nav-item.on::before,
  .sub-row.on::before {
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
  .offline {
    display: flex;
    color: var(--muted);
  }
  .collapsed .offline {
    position: absolute;
    bottom: 1px;
    right: 8px;
    padding: 1px;
    border-radius: 50%;
    background: var(--panel);
  }
  .collapsed .badge {
    position: absolute;
    top: 2px;
    right: 6px;
    padding: 0 4px;
    font-size: 10px;
    line-height: 14px;
  }
  .sub {
    margin: 2px 0 4px;
  }
  .sub-row {
    position: relative;
    display: flex;
    align-items: center;
  }
  .sub-row + .sub-row {
    margin-top: 1px;
  }
  .sub-item {
    display: flex;
    align-items: center;
    gap: 8px;
    flex: 1;
    min-width: 0;
    padding: 5px 8px 5px 30px;
    font-size: 13px;
    text-align: left;
    color: var(--text);
  }
  .collapsed .sub-item {
    justify-content: center;
    padding: 6px 0;
    color: var(--muted);
  }
  .collapsed .sub-row.on .sub-item {
    color: var(--text);
  }
  .close {
    flex: none;
    display: flex;
    padding: 4px;
    margin-right: 2px;
    color: var(--muted);
    opacity: 0;
  }
  .sub-row:hover .close,
  .sub-row:focus-within .close,
  .sub-row.on .close {
    opacity: 1;
  }
  .test-text {
    display: flex;
    flex-direction: column;
    gap: 1px;
    min-width: 0;
    flex: 1;
  }
  .test-title {
    display: flex;
    align-items: center;
    gap: 6px;
    min-width: 0;
  }
  .state {
    flex: none;
    font-size: 10px;
    line-height: 14px;
    padding: 0 5px;
    border-radius: 7px;
    border: 1px solid var(--border);
    color: var(--muted);
  }
  .state.active {
    background: var(--accent);
    border-color: var(--accent);
    color: var(--accent-text);
  }
  .state.passed {
    color: var(--clock-ok);
    border-color: var(--clock-ok);
  }
  .state.failed,
  .state.halted {
    color: var(--warn);
    border-color: var(--warn);
  }
  .test-meta {
    font-size: 11px;
    color: var(--muted);
    overflow: hidden;
    white-space: nowrap;
    text-overflow: ellipsis;
  }
  .llr {
    position: relative;
    height: 3px;
    margin: 4px 0 2px;
    border-radius: 2px;
    background: linear-gradient(to right, var(--warn), var(--border) 50%, var(--clock-ok));
  }
  .mark {
    position: absolute;
    top: -2px;
    width: 2px;
    height: 7px;
    margin-left: -1px;
    background: var(--text);
  }
  .test-dot.active {
    color: var(--accent);
  }
  .test-dot.passed {
    color: var(--clock-ok);
  }
  .test-dot.failed,
  .test-dot.halted {
    color: var(--warn);
  }
  .boards-head {
    margin: 14px 0 4px;
    padding: 0 10px;
    font-size: 11px;
    font-weight: 600;
    letter-spacing: 0.04em;
    text-transform: uppercase;
    color: var(--muted);
  }
  .collapsed .boards-head {
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
  .board {
    display: flex;
    align-items: center;
    gap: 9px;
    flex: 1;
    min-width: 0;
    padding: 5px 6px;
    text-align: left;
    color: var(--text);
  }
  .collapsed .board {
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
  .board-text {
    display: flex;
    flex-direction: column;
    min-width: 0;
  }
  .board-title {
    font-size: 13px;
    overflow: hidden;
    white-space: nowrap;
    text-overflow: ellipsis;
  }
  .board-meta {
    display: flex;
    align-items: center;
    gap: 4px;
    font-size: 11px;
    color: var(--muted);
    overflow: hidden;
    white-space: nowrap;
    text-overflow: ellipsis;
  }
  .in-window {
    display: flex;
  }
  .board.waiting .board-meta {
    color: var(--accent);
    font-weight: 600;
  }
  .tools {
    padding-top: 8px;
    margin-top: 8px;
    border-top: 1px solid var(--border);
    display: flex;
    flex-direction: column;
    gap: 2px;
  }
  main {
    position: relative;
    min-width: 0;
    min-height: 0;
    overflow: hidden;
  }
  .content {
    height: 100%;
    box-sizing: border-box;
    overflow-y: auto;
    padding: 18px 24px 32px;
  }
  .docked {
    position: absolute;
    inset: 0;
    width: 100%;
    height: 100%;
    border: none;
    display: none;
    background: var(--bg);
  }
  .docked.shown {
    display: block;
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
    z-index: 5;
  }
  .message button {
    padding: 2px 10px;
  }
</style>
