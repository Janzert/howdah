<script lang="ts">
  // The main window: home (new games, an analysis board, opening a
  // record, and the app-wide dialogs), the open game windows, and the
  // arimaa.com lobby. It has no board or session of its own; every game
  // opens in a game window (docs/WINDOWS.md). Notifications about
  // arimaa.com (invitations, postal moves due) live here, so they come
  // once, also while it's hidden.
  import { onMount } from 'svelte';
  import { analysisEngine } from './lib/analysis';
  import { type Api, api, apiFor, errorMessage } from './lib/api';
  import { setAppearance } from './lib/appearance';
  import { requestAttention } from './lib/attention';
  import type { EngineSpec } from './lib/bindings/EngineSpec';
  import type { GameroomGames } from './lib/bindings/GameroomGames';
  import type { MatchSpec } from './lib/bindings/MatchSpec';
  import type { SessionId } from './lib/bindings/SessionId';
  import type { SessionView } from './lib/bindings/SessionView';
  import type { WatchView } from './lib/bindings/WatchView';
  import MiniBoard from './lib/board/MiniBoard.svelte';
  import { formatClock, sideTimes } from './lib/clock';
  import EnginesDialog from './lib/EnginesDialog.svelte';
  import { on, onEvery } from './lib/events';
  import { myTurn } from './lib/gameroom';
  import GameroomLobby from './lib/GameroomLobby.svelte';
  import HelpDialog from './lib/HelpDialog.svelte';
  import NewGameDialog from './lib/NewGameDialog.svelte';
  import RecordDialog from './lib/RecordDialog.svelte';
  import { settings } from './lib/settings.svelte';
  import SettingsDialog from './lib/SettingsDialog.svelte';
  import { play, setMuted, setVolume, unlockOnInteraction } from './lib/sound';
  import { findTheme } from './lib/theme';
  import { gameTitle, waitsOnUser } from './lib/windowList';
  import { focusGameWindow, openGameWindow } from './lib/windows';

  let engines = $state<EngineSpec[]>([]);
  let message = $state<string | null>(null);
  let messageTimer: ReturnType<typeof setTimeout> | undefined;
  let showNewGame = $state(false);
  let showEngines = $state(false);
  let showSettings = $state(false);
  let showHelp = $state(false);
  let showRecord = $state(false);

  /** The open game windows' sessions, in the order they were opened. */
  let sessions = $state<SessionId[]>([]);
  /** Each session's latest view, and when it arrived (for its clock). */
  let views = $state<Record<SessionId, { view: SessionView; at: number }>>({});
  /** Each session's followed arimaa.com game. */
  let watches = $state<Record<SessionId, WatchView>>({});
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
    if (!Object.values(views).some((v) => v.view.clock?.running)) return;
    const timer = setInterval(() => (now = performance.now()), 500);
    return () => clearInterval(timer);
  });

  function flash(text: string) {
    message = text;
    clearTimeout(messageTimer);
    messageTimer = setTimeout(() => (message = null), 3500);
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

  function startGame(spec: MatchSpec): Promise<string | null> {
    const free =
      spec.gold.kind === 'human' && spec.silver.kind === 'human' && !spec.goldTimeControl && !spec.silverTimeControl;
    if (free) return openWindow();
    return openWindow((a) => a.startMatch(spec), spec).then((error) => {
      if (!error) play('gameStart');
      return error;
    });
  }

  async function analysisBoard() {
    const engine = analysisEngine(engines, settings.analysisEngine);
    const error = await openWindow(engine ? (a) => a.setAnalysis(engine.id) : undefined);
    if (error) flash(error);
    else if (!engine) flash('Add an engine to analyse with (Engines)');
  }

  /** Follows a session's view; a new session is listed. */
  function track(id: SessionId, view: SessionView) {
    views[id] = { view, at: performance.now() };
    if (!sessions.includes(id)) sessions = [...sessions, id];
  }

  async function addSession(id: SessionId) {
    const a = apiFor(id);
    try {
      const [view, watch] = await Promise.all([a.getState(), a.watchStatus()]);
      if (!views[id]) track(id, view);
      if (watch) watches[id] = watch;
    } catch {
      /* closed meanwhile */
    }
  }

  function setSessions(list: SessionId[]) {
    sessions = sessions.filter((id) => list.includes(id));
    for (const id of Object.keys(views).map(Number)) {
      if (!list.includes(id)) {
        delete views[id];
        delete watches[id];
      }
    }
    for (const id of list) if (!views[id]) void addSession(id);
  }

  onMount(() => {
    unlockOnInteraction();
    const unlisteners = [
      on('sessions://changed', (s) => setSessions(s.sessions)),
      onEvery('game://changed', (u, id) => id != null && track(id, u.view)),
      onEvery('gameroom://watch', (w, id) => {
        if (id == null) return;
        if (w.state === 'stopped') delete watches[id];
        else watches[id] = w;
      }),
      on('gameroom://lobby', lobbyGames),
      on('gameroom://invitation', async (a) => {
        play('notification');
        if (a.outcome.kind === 'accepted') {
          const { gid, side } = a.outcome;
          flash(`${a.opponent} accepted your invitation`);
          requestAttention('Invitation accepted');
          const error = await openWindow((s) => s.playGameroomGame(gid, side));
          if (error) flash(error);
        } else if (a.outcome.kind === 'declined') {
          flash(a.outcome.message);
        } else {
          flash(`Your invitation to ${a.opponent} is gone`);
        }
      }),
    ];
    api.listSessions().then(setSessions);
    api.gameroomLastGames().then((g) => g && lobbyGames(g));
    reloadEngines();
    return () => unlisteners.forEach((u) => void u.then((f) => f()));
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
      flash(`${fresh[0].opponent ?? 'Someone'} invites you to a game (arimaa.com)`);
      play('notification');
      requestAttention('Invitation');
    }
    const open = new Set(Object.values(watches).map((w) => w.gid));
    const turns = g.mine.filter((m) => m.postal && myTurn(m, g.user) && !open.has(m.gid)).map((m) => m.gid);
    const newTurns = turns.filter((gid) => !postalTurns.includes(gid));
    postalTurns = turns;
    if (newTurns.length > 0 && fresh.length === 0) {
      flash(
        newTurns.length === 1
          ? 'A postal game waits on your move (arimaa.com)'
          : `${newTurns.length} postal games wait on your move (arimaa.com)`,
      );
      play('notification');
      requestAttention('Your postal move');
    }
  }

  /** A side's clock, move time and reserve as the player bars show it. */
  function clockText(entry: { view: SessionView; at: number }, side: 'gold' | 'silver'): string | null {
    const times = entry.view.clock && sideTimes(entry.view.clock, side, now - entry.at);
    return times && `${formatClock(times.move)} · ${formatClock(times.reserve)}`;
  }

  /** The open windows, those waiting on the user's move first. */
  const listed = $derived(
    sessions
      .filter((id) => views[id])
      .map((id) => ({ id, ...views[id], yourMove: waitsOnUser(views[id].view) }))
      .sort((a, b) => Number(b.yourMove) - Number(a.yourMove)),
  );

  function describe(v: SessionView, id: SessionId): string {
    const watch = watches[id];
    if (watch) return watch.side != null ? 'arimaa.com game' : 'Watching on arimaa.com';
    if (v.players) {
      const engine = v.players.gold.kind === 'engine' || v.players.silver.kind === 'engine';
      return engine ? 'Against an engine' : 'Game';
    }
    if (v.analysisEngine) return 'Analysis';
    return v.moves.length > 0 ? `${v.moves.length} moves` : 'Empty board';
  }

  function onkeydown(e: KeyboardEvent) {
    if (e.key !== '?' || showNewGame || showEngines || showSettings || showRecord || showHelp) return;
    if ((e.target as HTMLElement).closest('input, textarea, select')) return;
    showHelp = true;
    e.preventDefault();
  }
</script>

<svelte:window {onkeydown} />

<main>
  <header>
    <h1>Howdah</h1>
    <span class="spacer"></span>
    <button onclick={() => (showEngines = true)}>Engines</button>
    <button onclick={() => (showSettings = true)}>Settings</button>
    <button onclick={() => (showHelp = true)} title="Keyboard and mouse help (?)">Help</button>
  </header>

  <div class="columns">
    <div class="column">
      <section aria-labelledby="home-title">
        <h2 id="home-title">Play and analyse</h2>
        <div class="actions">
          <button class="primary" onclick={() => (showNewGame = true)}>New game</button>
          <button onclick={analysisBoard} title="An empty board, with analysis on">Analysis board</button>
          <button onclick={() => (showRecord = true)} title="A game record from a file or pasted text">
            Open record
          </button>
        </div>
      </section>

      <section aria-labelledby="windows-title">
        <h2 id="windows-title">
          Open windows
          {#if invitationCount > 0}<span class="badge" title="Invitations to you on arimaa.com">{invitationCount}</span>{/if}
          {#if postalTurns.length > 0}<span class="badge" title="Postal games waiting on your move">
              {postalTurns.length} postal</span
            >{/if}
        </h2>
        {#if listed.length === 0}
          <p class="hint">No games are open. Each game you start or open gets a window of its own.</p>
        {:else}
          <ul class="windows" aria-label="Open windows">
            {#each listed as w (w.id)}
              {@const title = gameTitle(w.view) ?? 'Untitled game'}
              <li>
                <button class="window" onclick={() => focusGameWindow(w.id)} aria-label="Show {title}">
                  <span class="mini"><MiniBoard position={w.view.position} {theme} flipped={false} /></span>
                  <span class="about">
                    <span class="title">
                      {#if w.yourMove}<span class="badge">Your move</span>{/if}
                      {title}
                    </span>
                    <span class="meta">{describe(w.view, w.id)}</span>
                    {#if w.view.clock}
                      <span class="meta clocks">
                        <span class="dot gold"></span>{clockText(w, 'gold') ?? '–'}
                        <span class="dot silver"></span>{clockText(w, 'silver') ?? '–'}
                      </span>
                    {/if}
                    {#if w.view.result}
                      <span class="meta">{w.view.result.winner === 'gold' ? 'Gold' : 'Silver'} won</span>
                    {/if}
                  </span>
                </button>
              </li>
            {/each}
          </ul>
        {/if}
      </section>
    </div>

    <div class="column">
      <GameroomLobby onOpen={(start) => openWindow(start)} onGames={lobbyGames} />
    </div>
  </div>

  {#if message}<div class="message" role="status">{message}</div>{/if}
</main>

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
  main {
    min-height: 100vh;
    box-sizing: border-box;
    padding: 12px 20px 24px;
  }
  header {
    display: flex;
    align-items: center;
    gap: 6px;
    margin-bottom: 12px;
  }
  h1 {
    margin: 0;
    font-size: 20px;
  }
  h2 {
    display: flex;
    align-items: center;
    gap: 6px;
    margin: 0 0 10px;
    font-size: 16px;
  }
  .columns {
    display: grid;
    grid-template-columns: repeat(auto-fit, minmax(340px, 1fr));
    gap: 20px;
    align-items: start;
  }
  .column {
    display: flex;
    flex-direction: column;
    gap: 20px;
    min-width: 0;
  }
  section,
  .column > :global(section) {
    background: var(--panel);
    border: 1px solid var(--border);
    border-radius: 8px;
    padding: 14px 16px;
  }
  .actions {
    display: flex;
    flex-wrap: wrap;
    gap: 8px;
  }
  .windows {
    list-style: none;
    margin: 0;
    padding: 0;
    display: flex;
    flex-direction: column;
    gap: 6px;
  }
  .window {
    display: flex;
    align-items: center;
    gap: 12px;
    width: 100%;
    padding: 6px;
    text-align: left;
  }
  .mini {
    flex: none;
    width: 72px;
    height: 72px;
  }
  .about {
    display: flex;
    flex-direction: column;
    gap: 2px;
    min-width: 0;
  }
  .title {
    display: flex;
    align-items: center;
    gap: 6px;
    font-weight: 600;
    overflow: hidden;
    white-space: nowrap;
    text-overflow: ellipsis;
  }
  .meta {
    color: var(--muted);
    font-size: 12px;
  }
  .clocks {
    display: flex;
    align-items: center;
    gap: 4px;
    font-variant-numeric: tabular-nums;
  }
  .clocks .dot.silver {
    margin-left: 8px;
  }
  .dot {
    flex: none;
    width: 9px;
    height: 9px;
    border-radius: 50%;
    border: 1px solid rgba(0, 0, 0, 0.4);
  }
  .dot.gold {
    background: #e3b23c;
  }
  .dot.silver {
    background: #c9ced6;
  }
  .badge {
    flex: none;
    font-size: 11px;
    font-weight: 400;
    padding: 0 6px;
    border-radius: 8px;
    background: var(--accent);
    color: var(--accent-text);
  }
  .hint {
    margin: 0;
    font-size: 12px;
    color: var(--muted);
  }
  .spacer {
    flex: 1;
  }
  .message {
    position: fixed;
    left: 50%;
    bottom: 20px;
    transform: translateX(-50%);
    padding: 8px 14px;
    border-radius: 6px;
    background: var(--text);
    color: var(--bg);
    font-size: 13px;
    max-width: 80vw;
  }
</style>
