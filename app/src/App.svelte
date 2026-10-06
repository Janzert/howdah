<script lang="ts">
  import { onMount } from 'svelte';
  import { analysisEngine, pvMoves } from './lib/analysis';
  import AnalysisPanel from './lib/AnalysisPanel.svelte';
  import { api, errorMessage } from './lib/api';
  import type { AnalysisLine } from './lib/bindings/AnalysisLine';
  import type { AnalysisView } from './lib/bindings/AnalysisView';
  import { requestAttention } from './lib/attention';
  import type { Color } from './lib/bindings/Color';
  import type { EngineSpec } from './lib/bindings/EngineSpec';
  import type { MatchSpec } from './lib/bindings/MatchSpec';
  import type { SessionView } from './lib/bindings/SessionView';
  import type { Square } from './lib/bindings/Square';
  import Board from './lib/board/Board.svelte';
  import { type AnimHooks, BoardModel } from './lib/board/boardModel.svelte';
  import { nextTick, turnTimeLeft } from './lib/clock';
  import EnginePanel from './lib/EnginePanel.svelte';
  import EnginesDialog from './lib/EnginesDialog.svelte';
  import EvalBar from './lib/EvalBar.svelte';
  import GameEndDialog from './lib/GameEndDialog.svelte';
  import HelpDialog from './lib/HelpDialog.svelte';
  import { registerApp } from './lib/devHooks';
  import { on } from './lib/events';
  import MoveList from './lib/MoveList.svelte';
  import CommentBox from './lib/CommentBox.svelte';
  import NewGameDialog from './lib/NewGameDialog.svelte';
  import ChatPanel from './lib/ChatPanel.svelte';
  import PlayerBar from './lib/PlayerBar.svelte';
  import RecordDialog from './lib/RecordDialog.svelte';
  import { justEnded } from './lib/result';
  import { settings, type HoverInput } from './lib/settings.svelte';
  import SettingsDialog from './lib/SettingsDialog.svelte';
  import { shortcutFor, type ShortcutId } from './lib/shortcuts';
  import { play, setMuted, setVolume, unlockOnInteraction } from './lib/sound';
  import { findTheme } from './lib/theme';
  import TurnBar from './lib/TurnBar.svelte';
  import type { GameroomGames } from './lib/bindings/GameroomGames';
  import type { WatchView } from './lib/bindings/WatchView';
  import WatchDialog from './lib/WatchDialog.svelte';
  import WatchPanel from './lib/WatchPanel.svelte';
  import { myTurn, newOpponentChat } from './lib/gameroom';

  function pref(key: string): string | null {
    try {
      return localStorage.getItem(key);
    } catch {
      return null;
    }
  }
  function savePref(key: string, value: string) {
    try {
      localStorage.setItem(key, value);
    } catch {
      /* preference just won't persist */
    }
  }

  const model = new BoardModel();
  let view = $state<SessionView | null>(null);
  const theme = $derived(findTheme(settings.theme));
  let flipped = $state(pref('flipped') === '1');
  let message = $state<string | null>(null);
  /** An engine's move that arrived while the user was looking elsewhere
   * (`3s ed7s …`), until they go back to the live game. */
  let missedMove = $state<string | null>(null);
  let record = $state<string | null>(null);
  let showNewGame = $state(false);
  let showEngines = $state(false);
  let showSettings = $state(false);
  let showHelp = $state(false);
  let showWatch = $state(false);
  /** The arimaa.com game this session follows, if any. */
  let watch = $state<WatchView | null>(null);
  /** Open invitations to the user (from the lobby watcher), and those
   * already announced. */
  let invitationCount = $state(0);
  const seenInvitations = new Set<string>();
  /** The user's postal games waiting on their move (from the lobby
   * watcher), by gid. */
  let postalTurns = $state<string[]>([]);
  // The game-end dialog waits for the final move's animation.
  let gameEndPending = $state(false);
  let showGameEnd = $state(false);
  /** The last match started from the new-game dialog, for a rematch. */
  let lastSpec = $state<MatchSpec | null>(null);
  let engines = $state<EngineSpec[]>([]);
  /** The latest analysis update. */
  let analysis = $state<AnalysisView | null>(null);
  let messageTimer: ReturnType<typeof setTimeout> | undefined;

  // Clock ticking: views carry elapsed time at send; count on from arrival.
  let receivedAt = $state(performance.now());
  let now = $state(performance.now());
  $effect(() => {
    if (!view?.clock?.running) return;
    const timer = setInterval(() => (now = performance.now()), 200);
    return () => clearInterval(timer);
  });

  // Bumped when a new match starts, to clear the engine output panel.
  let matchKey = $state(0);
  let matchSig = '';

  function setView(v: SessionView) {
    const sig = v.players ? `${v.players.gold.name}|${v.players.silver.name}` : '';
    if (sig !== matchSig || (v.players && v.moves.length === 0 && (view?.moves.length ?? 0) > 0)) {
      matchKey++;
      if (settings.humanAtBottom) {
        const side = loneHuman(v);
        if (side) flipped = side === 'silver';
      }
    }
    matchSig = sig;
    view = v;
    receivedAt = performance.now();
    now = receivedAt;
  }

  /** The only human side in a match, if there's exactly one. */
  function loneHuman(v: SessionView): Color | null {
    if (!v.players) return null;
    const gold = v.players.gold.kind === 'human';
    const silver = v.players.silver.kind === 'human';
    return gold === silver ? null : gold ? 'gold' : 'silver';
  }

  async function reloadEngines() {
    engines = await api.listEngines();
  }

  async function startGame(spec: MatchSpec): Promise<string | null> {
    try {
      const free =
        spec.gold.kind === 'human' && spec.silver.kind === 'human' && !spec.goldTimeControl && !spec.silverTimeControl;
      await (free ? api.newGame() : api.startMatch(spec));
      lastSpec = free ? null : spec;
      if (!free) play('gameStart');
      return null;
    } catch (e) {
      return errorMessage(e);
    }
  }

  /** The same game again: the last match while one is on, else free play. */
  function rematch(): Promise<string | null> {
    if (view?.players && lastSpec) return startGame(lastSpec);
    const human = { kind: 'human' } as const;
    return startGame({ gold: human, silver: human, goldTimeControl: null, silverTimeControl: null, takebacks: false });
  }

  const swappedSpec = $derived.by((): MatchSpec | null => {
    if (!view?.players || !lastSpec) return null;
    const s = lastSpec;
    const swapped = {
      gold: s.silver,
      silver: s.gold,
      goldTimeControl: s.silverTimeControl,
      silverTimeControl: s.goldTimeControl,
      takebacks: s.takebacks,
    };
    return JSON.stringify(swapped) === JSON.stringify(s) ? null : swapped;
  });

  $effect(() => {
    if (!view?.result) {
      // A new game (or going back) drops an announcement not yet made.
      gameEndPending = false;
      showGameEnd = false;
    } else if (gameEndPending && !model.animating) {
      gameEndPending = false;
      showGameEnd = true;
    }
  });

  // Low-time ticks while a human's clock runs, at the times in TICK_TIMES_MS.
  $effect(() => {
    const clock = view?.clock;
    const side = clock?.running;
    if (!clock || !side || view?.result || view?.players?.[side].kind !== 'human') return;
    const arrived = receivedAt;
    let timer: ReturnType<typeof setTimeout> | undefined;
    const schedule = (lastTick?: number) => {
      const left = turnTimeLeft(clock, performance.now() - arrived);
      const tick = nextTick(left, lastTick);
      if (tick == null) return;
      timer = setTimeout(() => {
        play('tick');
        schedule(tick);
      }, left - tick);
    };
    schedule();
    return () => clearTimeout(timer);
  });

  $effect(() => savePref('flipped', flipped ? '1' : '0'));
  $effect(() => setMuted(!settings.sound));
  $effect(() => setVolume(settings.volume));
  $effect(() => model.setBaseSpeed(settings.stepMs));
  $effect(() => {
    api.setContinueTurns(settings.continueTurns).catch(() => {});
  });

  function flash(text: string) {
    message = text;
    clearTimeout(messageTimer);
    messageTimer = setTimeout(() => (message = null), 3500);
  }

  /** Runs a command, showing any error. Returns whether it succeeded. */
  async function run(p: Promise<unknown>): Promise<boolean> {
    try {
      await p;
      return true;
    } catch (e) {
      flash(errorMessage(e));
      return false;
    }
  }

  if (import.meta.env.DEV) registerApp({ state: () => view, message: () => message, analysis: () => analysis, model });

  onMount(() => {
    unlockOnInteraction();
    const unlisten = on('game://changed', (u) => {
      const prev = view;
      const ended = justEnded(prev, u.view);
      const setupCommitted = view != null && view.phase === 'setup' && u.view.ply > view.ply && u.animation.length === 0;
      setView(u.view);
      model.apply(u.view.position.pieces, u.animation, animHooks(u.view.turn != null), u.animationBudgetMs);
      if (setupCommitted) play('lastStep');
      if (ended) {
        const human = loneHuman(u.view);
        play(human && u.view.result?.winner !== human ? 'loss' : 'win');
        gameEndPending = true;
      }
      if (prev && u.view.moves.length > prev.moves.length && awaitsHumanAgainstEngine(u.view)) {
        requestAttention();
      }
      if (!awayFromLive(u.view)) {
        // Back at the live position after missing a move: show it being
        // played, unless this update already animated it.
        if (missedMove && u.animation.length === 0) replayShownMove();
        missedMove = null;
      } else if (prev && u.view.liveMove && u.view.liveMove !== prev.liveMove && movedByOpponent(u.view)) {
        missedMove = u.view.liveMove;
        play('lastStep');
        // Only a player is asked to come back ("Your move"), not a spectator.
        if (loneHuman(u.view)) requestAttention();
      }
    });
    const unlistenAnalysis = on('analysis://update', (u) => (analysis = u));
    const unlistenWatch = on('gameroom://watch', (w) => {
      const chat = newOpponentChat(watch, w);
      watch = w.state === 'stopped' ? null : w;
      if (chat.length > 0) requestAttention('New chat');
    });
    const unlistenLobby = on('gameroom://lobby', lobbyGames);
    const unlistenInvitation = on('gameroom://invitation', (a) => {
      if (a.outcome.kind === 'accepted') {
        const { gid, side } = a.outcome;
        // Into the game, unless this window plays another one.
        if (watch?.side != null && watch.state !== 'ended') {
          flash(`${a.opponent} accepted your invitation: play it from Your games (arimaa.com)`);
        } else {
          flash(`${a.opponent} accepted your invitation`);
          run(api.playGameroomGame(gid, side));
        }
        requestAttention('Invitation accepted');
      } else if (a.outcome.kind === 'declined') {
        flash(a.outcome.message);
      } else {
        flash(`Your invitation to ${a.opponent} is gone`);
      }
    });
    api.watchStatus().then((w) => (watch = w));
    api.getState().then((v) => {
      setView(v);
      model.snap(v.position.pieces);
    });
    reloadEngines();
    return () => {
      unlisten.then((f) => f());
      unlistenAnalysis.then((f) => f());
      unlistenWatch.then((f) => f());
      unlistenLobby.then((f) => f());
      unlistenInvitation.then((f) => f());
    };
  });

  /** Counts the invitations to the user and their postal games waiting on
   * their move in the lobby's lists, and announces new ones. */
  function lobbyGames(g: GameroomGames) {
    const incoming = g.invitations.filter((i) => i.incoming);
    const fresh = incoming.filter((i) => !seenInvitations.has(i.created));
    invitationCount = incoming.length;
    for (const i of incoming) seenInvitations.add(i.created);
    if (fresh.length > 0 && !showWatch) {
      flash(`${fresh[0].opponent ?? 'Someone'} invites you to a game (arimaa.com)`);
      requestAttention('Invitation');
    }
    // The game this window plays doesn't count; its own alerts cover it.
    const turns = g.mine.filter((m) => m.postal && myTurn(m, g.user) && m.gid !== watch?.gid).map((m) => m.gid);
    const newTurns = turns.filter((gid) => !postalTurns.includes(gid));
    postalTurns = turns;
    if (newTurns.length > 0 && fresh.length === 0 && !showWatch) {
      flash(
        newTurns.length === 1
          ? 'A postal game waits on your move (arimaa.com)'
          : `${newTurns.length} postal games wait on your move (arimaa.com)`,
      );
      requestAttention('Your postal move');
    }
  }

  /** A new open game on arimaa.com with the time control and rating of
   * the online game the user just played, as `side`. */
  function newOnlineGame(side: Color, timeControl: string, rated: boolean): Promise<string | null> {
    return openGameroomGame(() => api.createGameroomGame(side, timeControl, rated));
  }

  /** The user's finished online game, for a new game like it. */
  const playedOnline = $derived(
    watch?.side != null && watch.timeControl != null
      ? { side: watch.side, timeControl: watch.timeControl, rated: watch.rated }
      : null,
  );
  const sideName = (c: Color) => (c === 'gold' ? 'Gold' : 'Silver');
  const otherSide = (c: Color): Color => (c === 'gold' ? 'silver' : 'gold');

  /** In a match, showing something other than the live position. */
  function awayFromLive(v: SessionView): boolean {
    return v.players != null && v.livePly !== v.ply;
  }

  /** Whether the latest move in the match came from an engine or a remote
   * player rather than the user. */
  function movedByOpponent(v: SessionView): boolean {
    const kind = v.players?.[liveMover(v)].kind;
    return kind != null && kind !== 'human';
  }

  /** The side that played the latest move (`3s …` is silver's). */
  function liveMover(v: SessionView): Color {
    return v.liveMove?.split(' ')[0].endsWith('g') ? 'gold' : 'silver';
  }

  /** Whether a human is to move at the live end, playing a non-human. */
  function awaitsHumanAgainstEngine(v: SessionView): boolean {
    if (!v.players || !v.playsLive) return false;
    const toMove = v.position.sideToMove;
    const other = toMove === 'gold' ? 'silver' : 'gold';
    return v.players[toMove].kind === 'human' && v.players[other].kind !== 'human';
  }

  const interactive = $derived(view != null && view.phase !== 'over' && view.canInput);
  const hasEngine = $derived(
    view?.players != null && (view.players.gold.kind === 'engine' || view.players.silver.kind === 'engine'),
  );

  const analysing = $derived(view?.analysisEngine != null);
  /** What analysis found for the shown node: this search's line once it
   * arrives, until then the one kept from an earlier visit. */
  const shown = $derived.by((): { line: AnalysisLine | null; stored: boolean } => {
    if (!view || !analysing) return { line: null, stored: false };
    if (analysis?.node === view.cursor && analysis.line) return { line: analysis.line, stored: analysis.stored };
    return { line: view.storedAnalysis, stored: view.storedAnalysis != null };
  });
  /** The PV's first turn, drawn on the board while no steps are taken. */
  const pvMove = $derived(view?.turn?.steps.length ? null : (shown.line?.pv[0]?.steps ?? null));
  /** A match with an engine is still being played (its live node has no result). */
  const matchRunning = $derived.by(() => {
    if (!hasEngine || view?.live == null) return false;
    const live = view.live;
    const result = live === view.cursor ? view.result : view.tree.find((n) => n.id === live)?.result;
    return result == null;
  });

  /** Turns analysis on with the engine used last (or the first), or off. */
  function toggleAnalysis() {
    if (analysing) {
      run(api.setAnalysis(null));
      return;
    }
    if (view && !view.analysisAllowed) {
      flash('No analysis during your arimaa.com game (after leaving one mid-game, load another game first)');
      return;
    }
    const engine = analysisEngine(engines, settings.analysisEngine);
    if (!engine) {
      flash('Add an engine first (Engines)');
      return;
    }
    useAnalysisEngine(engine.id);
  }

  function useAnalysisEngine(id: string) {
    settings.analysisEngine = id;
    run(api.setAnalysis(id));
  }

  function addPv(line: AnalysisLine, index: number) {
    run(api.addLine(line.node, pvMoves(line, index)));
  }

  function onDrop(from: Square, to: Square, path: Square[]): Promise<boolean> {
    if (!view) return Promise.resolve(false);
    return run(view.phase === 'setup' ? api.setupSwap(from, to) : api.tryRoute(from, to, path));
  }

  function commit() {
    if (!view) return;
    if (view.phase === 'setup') run(api.commitSetup());
    else if (view.turn) run(api.commitTurn()).then((ok) => ok && play('lastStep'));
    else if (view.planMove) run(api.commitTurn());
  }

  /** Sounds for an animation. Steps of a turn being entered are all soft;
   * the louder one comes when the move is played. */
  const animHooks = (entering: boolean): AnimHooks => ({
    onSlide: (last) => play(last && !entering ? 'lastStep' : 'step'),
    onCapture: (own) => play(own ? 'ownLoss' : 'capture'),
    onRestore: () => play('restore'),
  });

  function goto(ply: number) {
    if (!view) return;
    // Forward from the latest move replays it, since there's nothing after it.
    if (ply === view.ply + 1 && view.ply === view.moves.length) replayShownMove();
    else if (ply >= 0 && ply <= view.moves.length && ply !== view.ply) run(api.gotoPly(ply));
  }

  /** End: the live position in a match, otherwise the end of the line. */
  function gotoEnd() {
    if (!view) return;
    if (view.players && view.livePly !== view.ply) run(api.gotoLive());
    else goto(view.moves.length);
  }

  async function replayShownMove() {
    if (!view || view.turn?.steps.length || model.animating) return;
    const replay = await api.moveReplay();
    if (replay && view && !model.animating) {
      model.replay(replay.before, view.position.pieces, replay.animation, animHooks(false));
    }
  }

  async function openGameroomGame(start: () => Promise<void>): Promise<string | null> {
    try {
      await start();
      lastSpec = null;
      return null;
    } catch (e) {
      return errorMessage(e);
    }
  }

  async function openRecord() {
    record = await api.exportGame();
  }

  async function loadRecord(text: string): Promise<string | null> {
    try {
      await api.loadGame(text);
      return null;
    } catch (e) {
      return errorMessage(e);
    }
  }

  const HOVER_ORDER: HoverInput[] = ['off', 'arrows', 'step'];
  const HOVER_NAMES: Record<HoverInput, string> = { off: 'off', arrows: 'arrows', step: 'step mode' };

  const shortcutActions: Record<ShortcutId, (v: SessionView) => void> = {
    back: (v) => goto(v.ply - 1),
    forward: (v) => goto(v.ply + 1),
    start: () => goto(0),
    end: () => gotoEnd(),
    prevVariation: () => run(api.gotoSibling(-1)),
    nextVariation: () => run(api.gotoSibling(1)),
    prevBranch: () => run(api.gotoBranch(false)),
    nextBranch: () => run(api.gotoBranch(true)),
    commit: (v) => {
      if (v.canInput) commit();
    },
    planTurn: (v) => {
      if (v.turn && v.canInput) run(api.commitTurn(true));
    },
    undoStep: (v) => {
      if (v.canUndo) run(api.undoStep());
    },
    resetTurn: (v) => {
      if (v.turn && v.canInput) run(api.cancelTurn());
    },
    moveNow: (v) => {
      if (v.thinking) run(api.engineMoveNow());
      else if (shown.line?.pv.length && !v.turn?.steps.length) addPv(shown.line, 0);
    },
    analysis: () => toggleAnalysis(),
    flip: () => (flipped = !flipped),
    cycleHover: () => {
      const next = HOVER_ORDER[(HOVER_ORDER.indexOf(settings.hoverInput) + 1) % HOVER_ORDER.length];
      settings.hoverInput = next;
      flash(`Hover input: ${HOVER_NAMES[next]}`);
    },
    mute: () => {
      settings.sound = !settings.sound;
      flash(settings.sound ? 'Sound on' : 'Sound off');
    },
    help: () => (showHelp = true),
  };

  function onkeydown(e: KeyboardEvent) {
    if (!view || record != null || showNewGame || showEngines || showSettings || showGameEnd || showHelp || showWatch) return;
    const target = e.target as HTMLElement;
    if (target.closest('input, textarea, select')) return;
    const shortcut = shortcutFor(e);
    if (!shortcut) return;
    // Enter and Space on a focused button press the button instead.
    if ((e.key === 'Enter' || e.key === ' ') && target.closest('button')) return;
    shortcutActions[shortcut.id](view);
    e.preventDefault();
  }
</script>

<svelte:window {onkeydown} />

<main>
  <section class="board-area">
    {#if view}
      <PlayerBar
        {view}
        side={flipped ? 'gold' : 'silver'}
        {receivedAt}
        {now}
        {theme}
        away={watch?.away[flipped ? 0 : 1] ?? false}
      />
    {/if}
    <div class="board-wrap" class:with-bar={analysing}>
      {#if analysing}
        <div class="eval-box">
          <EvalBar evaluation={shown.line?.eval ?? null} {flipped} />
        </div>
      {/if}
      {#if missedMove && view?.players}
        {@const mover = liveMover(view)}
        <button class="live-alert" onclick={gotoEnd} aria-live="polite">
          <span class="dot {mover}"></span>
          <span>{view.players[mover].name} played <code>{missedMove}</code></span>
          <strong>Back to live game</strong> <kbd>End</kbd>
        </button>
      {/if}
      <div class="board-box">
        <Board
          {model}
          {theme}
          {flipped}
          {interactive}
          pushPending={view?.turn?.pushPending ?? null}
          lastMove={view?.turn?.steps.length ? null : (view?.lastMove ?? null)}
          {pvMove}
          coordinates={settings.coordinates}
          hoverInput={settings.hoverInput}
          positionKey={view ? `${view.ply}|${view.position.short}|${view.canInput}|${view.turn?.steps.length ?? 0}` : ''}
          onStep={(from, to) => run(api.tryStep(from, to))}
          {onDrop}
          legalTargets={(from) => api.legalTargets(from)}
          planRoute={(from, to, path) => (view?.phase === 'setup' ? Promise.resolve(null) : api.planRoute(from, to, path))}
        />
      </div>
    </div>
    {#if view}
      <PlayerBar
        {view}
        side={flipped ? 'silver' : 'gold'}
        {receivedAt}
        {now}
        {theme}
        away={watch?.away[flipped ? 1 : 0] ?? false}
      />
    {/if}
    <!-- The game's chat, below the board as in the arimaa.com web client. -->
    {#if view && watch && (watch.side != null || watch.chat.length > 0)}
      <ChatPanel {watch} {view} />
    {/if}
    {#if message}<div class="message" role="status">{message}</div>{/if}
  </section>

  <aside class="panel">
    {#if view}
      <TurnBar
        {view}
        onCommit={commit}
        onUndo={() => run(api.undoStep())}
        onTakeBack={() => run(api.takeBack())}
        onGotoLive={gotoEnd}
        onCancel={() => run(api.cancelTurn())}
        onMoveNow={() => run(api.engineMoveNow())}
        onEndMatch={() => run(api.endMatch())}
      />
      <MoveList {view} onGoto={goto} onGotoNode={(id) => run(api.gotoNode(id))} {run} />
      <CommentBox {view} {run} />
      {#if watch}
        <WatchPanel {watch} {view} {run} {receivedAt} {now} />
      {/if}
      {#if hasEngine && view.players}
        <EnginePanel players={view.players} resetKey={matchKey} />
      {/if}
      {#if analysing || analysis?.state === 'failed'}
        <AnalysisPanel
          {view}
          {analysis}
          line={shown.line}
          stored={shown.stored}
          {engines}
          {theme}
          {flipped}
          sharesCpu={matchRunning}
          onEngine={useAnalysisEngine}
          onClose={() => {
            if (analysing) run(api.setAnalysis(null));
            analysis = null;
          }}
          onAdd={addPv}
          preview={(line, i) => api.previewLine(line.node, pvMoves(line, i))}
        />
      {/if}
      <div class="nav">
        <button aria-label="Start" onclick={() => goto(0)} title="Start (Home or 0)">⏮</button>
        <button aria-label="Back" onclick={() => goto(view!.ply - 1)} title="Back (← or k)">◀</button>
        <button aria-label="Forward" onclick={() => goto(view!.ply + 1)} title="Forward (→ or j); at the latest move, replays it">▶</button>
        <button aria-label="End" onclick={gotoEnd} title="End (End or $)">⏭</button>
      </div>
    {/if}
    <div class="tools">
      <button onclick={() => (showNewGame = true)}>New game</button>
      <button onclick={() => (showWatch = true)} title="Play or watch games on arimaa.com, or open finished ones">
        arimaa.com{#if invitationCount > 0}<span class="badge" title="Invitations to you">{invitationCount}</span>{/if}{#if postalTurns.length > 0}<span
            class="badge"
            title="Postal games waiting on your move">{postalTurns.length}</span
          >{/if}
      </button>
      <button onclick={() => (showEngines = true)}>Engines</button>
      <button
        onclick={toggleAnalysis}
        aria-pressed={analysing}
        disabled={!analysing && view != null && !view.analysisAllowed}
        title={view && !view.analysisAllowed
          ? 'No analysis during your arimaa.com game (after leaving one mid-game, load another game first)'
          : 'Analyse the shown position with an engine (l)'}
      >
        Analysis
      </button>
      <button onclick={openRecord}>Record</button>
      <button onclick={() => (flipped = !flipped)} title="Flip the board (f)">Flip</button>
      <button onclick={() => (showSettings = true)}>Settings</button>
      <button onclick={() => (showHelp = true)} title="Keyboard and mouse help (?)">Help</button>
    </div>
  </aside>
</main>

{#if record != null}
  <RecordDialog
    initial={record}
    onExport={(mainLineOnly) => api.exportGame(mainLineOnly)}
    onLoad={loadRecord}
    onClose={() => (record = null)}
  />
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
  <EnginesDialog onChanged={reloadEngines} onClose={() => (showEngines = false)} />
{/if}
{#if showGameEnd && view?.result}
  {@const online = playedOnline}
  <GameEndDialog
    {view}
    onRematch={online
      ? () => newOnlineGame(online.side, online.timeControl, online.rated)
      : watch
        ? undefined
        : rematch}
    onSwapSides={online
      ? () => newOnlineGame(otherSide(online.side), online.timeControl, online.rated)
      : swappedSpec
        ? () => startGame(swappedSpec!)
        : undefined}
    rematchLabel={online ? `New game as ${sideName(online.side)}` : undefined}
    swapLabel={online ? `New game as ${sideName(otherSide(online.side))}` : undefined}
    newGameTitle={online
      ? `An open game on arimaa.com, ${online.timeControl}${online.rated ? ', rated' : ''}, for anyone to join`
      : undefined}
    onAnalyse={engines.length && !analysing ? toggleAnalysis : undefined}
    onClose={() => (showGameEnd = false)}
  />
{/if}
{#if showWatch}
  <WatchDialog onOpen={openGameroomGame} onClose={() => (showWatch = false)} onGames={lobbyGames} />
{/if}
{#if showSettings}
  <SettingsDialog onClose={() => (showSettings = false)} />
{/if}
{#if showHelp}
  <HelpDialog onClose={() => (showHelp = false)} />
{/if}

<style>
  main {
    display: grid;
    grid-template-columns: 1fr minmax(260px, 320px);
    height: 100vh;
    overflow: hidden;
  }
  .board-area {
    position: relative;
    display: flex;
    flex-direction: column;
    gap: 6px;
    padding: 12px 16px;
    min-width: 0;
    min-height: 0;
  }
  .board-wrap {
    position: relative;
    flex: 1;
    container-type: size;
    display: flex;
    justify-content: center;
    align-items: center;
    gap: 8px;
    min-height: 0;
  }
  .board-box {
    width: min(100cqw, 100cqh);
    height: min(100cqw, 100cqh);
  }
  /* The eval bar takes its width and gap from the board. */
  .with-bar .board-box {
    width: min(100cqw - 22px, 100cqh);
    height: min(100cqw - 22px, 100cqh);
  }
  .eval-box {
    height: min(100cqw - 22px, 100cqh);
  }
  [aria-pressed='true'] {
    background: var(--accent);
    color: var(--accent-text);
  }
  .panel {
    display: flex;
    flex-direction: column;
    gap: 10px;
    padding: 12px;
    background: var(--panel);
    border-left: 1px solid var(--border);
    min-height: 0;
  }
  .panel :global(.moves) {
    flex: 1;
    min-height: 0;
  }
  .nav {
    display: grid;
    grid-template-columns: repeat(4, 1fr);
    gap: 4px;
  }
  .badge {
    margin-left: 4px;
    font-size: 11px;
    padding: 0 5px;
    border-radius: 8px;
    background: var(--accent);
    color: var(--accent-text);
  }
  .tools {
    display: flex;
    flex-wrap: wrap;
    gap: 6px;
  }
  .live-alert {
    position: absolute;
    top: 10px;
    left: 50%;
    transform: translateX(-50%);
    z-index: 2;
    display: flex;
    align-items: center;
    gap: 8px;
    max-width: calc(100% - 32px);
    padding: 6px 12px;
    border: none;
    border-radius: 6px;
    background: var(--accent);
    color: var(--accent-text);
    font-size: 13px;
    white-space: nowrap;
    box-shadow: 0 2px 10px rgba(0, 0, 0, 0.45);
    animation: live-alert-in 0.9s ease-out;
    cursor: pointer;
  }
  .live-alert code {
    font-size: 12px;
    overflow: hidden;
    text-overflow: ellipsis;
  }
  .live-alert > span:not(.dot) {
    min-width: 0;
    overflow: hidden;
    text-overflow: ellipsis;
  }
  .live-alert .dot {
    flex: none;
    width: 10px;
    height: 10px;
    border-radius: 50%;
    border: 1px solid rgba(0, 0, 0, 0.4);
  }
  .live-alert .dot.gold {
    background: #e3b23c;
  }
  .live-alert .dot.silver {
    background: #c9ced6;
  }
  @keyframes live-alert-in {
    0% {
      transform: translate(-50%, -12px);
      opacity: 0;
    }
    30% {
      transform: translate(-50%, 0);
      opacity: 1;
    }
    45%,
    75% {
      box-shadow: 0 0 0 6px rgba(255, 255, 255, 0.35);
    }
    100% {
      box-shadow: 0 2px 10px rgba(0, 0, 0, 0.45);
    }
  }
  .message {
    position: absolute;
    bottom: 24px;
    left: 50%;
    transform: translateX(-50%);
    max-width: 80%;
    padding: 6px 12px;
    border-radius: 6px;
    background: rgba(30, 28, 25, 0.88);
    color: #fff;
    font-size: 13px;
    pointer-events: none;
  }
</style>
