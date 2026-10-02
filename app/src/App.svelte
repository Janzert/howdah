<script lang="ts">
  import { onMount } from 'svelte';
  import { api, errorMessage } from './lib/api';
  import { requestAttention } from './lib/attention';
  import type { Color } from './lib/bindings/Color';
  import type { EngineSpec } from './lib/bindings/EngineSpec';
  import type { MatchSpec } from './lib/bindings/MatchSpec';
  import type { SessionView } from './lib/bindings/SessionView';
  import type { Square } from './lib/bindings/Square';
  import Board from './lib/board/Board.svelte';
  import { BoardModel } from './lib/board/boardModel.svelte';
  import { nextTick, turnTimeLeft } from './lib/clock';
  import EnginePanel from './lib/EnginePanel.svelte';
  import EnginesDialog from './lib/EnginesDialog.svelte';
  import GameEndDialog from './lib/GameEndDialog.svelte';
  import { registerApp } from './lib/devHooks';
  import { on } from './lib/events';
  import MoveList from './lib/MoveList.svelte';
  import NewGameDialog from './lib/NewGameDialog.svelte';
  import PlayerBar from './lib/PlayerBar.svelte';
  import RecordDialog from './lib/RecordDialog.svelte';
  import { justEnded } from './lib/result';
  import { settings } from './lib/settings.svelte';
  import SettingsDialog from './lib/SettingsDialog.svelte';
  import { play, setMuted, unlockOnInteraction } from './lib/sound';
  import { findTheme } from './lib/theme';
  import TurnBar from './lib/TurnBar.svelte';

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
  let record = $state<string | null>(null);
  let showNewGame = $state(false);
  let showEngines = $state(false);
  let showSettings = $state(false);
  // The game-end dialog waits for the final move's animation.
  let gameEndPending = $state(false);
  let showGameEnd = $state(false);
  /** The last match started from the new-game dialog, for a rematch. */
  let lastSpec = $state<MatchSpec | null>(null);
  let engines = $state<EngineSpec[]>([]);
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
      return null;
    } catch (e) {
      return errorMessage(e);
    }
  }

  /** The same game again: the last match while one is on, else free play. */
  function rematch(): Promise<string | null> {
    if (view?.players && lastSpec) return startGame(lastSpec);
    const human = { kind: 'human' } as const;
    return startGame({ gold: human, silver: human, goldTimeControl: null, silverTimeControl: null });
  }

  const swappedSpec = $derived.by((): MatchSpec | null => {
    if (!view?.players || !lastSpec) return null;
    const s = lastSpec;
    const swapped = {
      gold: s.silver,
      silver: s.gold,
      goldTimeControl: s.silverTimeControl,
      silverTimeControl: s.goldTimeControl,
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

  if (import.meta.env.DEV) registerApp({ state: () => view, message: () => message, model });

  onMount(() => {
    unlockOnInteraction();
    const unlisten = on('game://changed', (u) => {
      const prev = view;
      const ended = justEnded(prev, u.view);
      const setupCommitted = view != null && view.phase === 'setup' && u.view.ply > view.ply && u.animation.length === 0;
      setView(u.view);
      model.apply(u.view.position.pieces, u.animation, animHooks, u.animationBudgetMs);
      if (setupCommitted) play('place');
      if (ended) {
        play('win');
        gameEndPending = true;
      }
      if (prev && u.view.moves.length > prev.moves.length && awaitsHumanAgainstEngine(u.view)) {
        requestAttention();
      }
    });
    api.getState().then((v) => {
      setView(v);
      model.snap(v.position.pieces);
    });
    reloadEngines();
    return () => {
      unlisten.then((f) => f());
    };
  });

  /** Whether a human is to move at the live end, playing a non-human. */
  function awaitsHumanAgainstEngine(v: SessionView): boolean {
    if (!v.players || !v.canInput || v.result || v.ply !== v.moves.length) return false;
    const toMove = v.position.sideToMove;
    const other = toMove === 'gold' ? 'silver' : 'gold';
    return v.players[toMove].kind === 'human' && v.players[other].kind !== 'human';
  }

  const interactive = $derived(view != null && view.phase !== 'over' && view.canInput);
  const hasEngine = $derived(
    view?.players != null && (view.players.gold.kind === 'engine' || view.players.silver.kind === 'engine'),
  );

  function onDrop(from: Square, to: Square, path: Square[]): Promise<boolean> {
    if (!view) return Promise.resolve(false);
    return run(view.phase === 'setup' ? api.setupSwap(from, to) : api.tryRoute(from, to, path));
  }

  function commit() {
    if (!view) return;
    if (view.phase === 'setup') run(api.commitSetup());
    else if (view.turn) run(api.commitTurn());
  }

  const animHooks = {
    // The arimaa.com clients play place.wav for every step.
    onSlide: () => play('place'),
    onCapture: () => play('trapped'),
  };

  function goto(ply: number) {
    if (!view) return;
    // Forward from the latest move replays it, since there's nothing after it.
    if (ply === view.ply + 1 && view.ply === view.moves.length) replayShownMove();
    else if (ply >= 0 && ply <= view.moves.length && ply !== view.ply) run(api.gotoPly(ply));
  }

  async function replayShownMove() {
    if (!view || view.turn?.steps.length || model.animating) return;
    const replay = await api.moveReplay();
    if (replay && view && !model.animating) {
      model.replay(replay.before, view.position.pieces, replay.animation, animHooks);
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

  function onkeydown(e: KeyboardEvent) {
    if (!view || record != null || showNewGame || showEngines || showSettings || showGameEnd) return;
    const target = e.target as HTMLElement;
    if (target.closest('input, textarea, select')) return;
    switch (e.key) {
      case 'ArrowLeft':
        goto(view.ply - 1);
        break;
      case 'ArrowRight':
        goto(view.ply + 1);
        break;
      case 'Home':
        goto(0);
        break;
      case 'End':
        goto(view.moves.length);
        break;
      case 'Enter':
        if (view.canInput) commit();
        break;
      case 'Backspace':
        if (view.turn && view.canInput) run(api.undoStep());
        break;
      case 'Escape':
        if (view.turn && view.canInput) run(api.cancelTurn());
        break;
      default:
        return;
    }
    e.preventDefault();
  }
</script>

<svelte:window {onkeydown} />

<main>
  <section class="board-area">
    {#if view}
      <PlayerBar {view} side={flipped ? 'gold' : 'silver'} {receivedAt} {now} {theme} />
    {/if}
    <div class="board-wrap">
      <div class="board-box">
        <Board
          {model}
          {theme}
          {flipped}
          {interactive}
          pushPending={view?.turn?.pushPending ?? null}
          lastMove={view?.turn?.steps.length ? null : (view?.lastMove ?? null)}
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
      <PlayerBar {view} side={flipped ? 'silver' : 'gold'} {receivedAt} {now} {theme} />
    {/if}
    {#if message}<div class="message" role="status">{message}</div>{/if}
  </section>

  <aside class="panel">
    {#if view}
      <TurnBar
        {view}
        onCommit={commit}
        onUndo={() => run(api.undoStep())}
        onCancel={() => run(api.cancelTurn())}
        onMoveNow={() => run(api.engineMoveNow())}
        onEndMatch={() => run(api.endMatch())}
      />
      <MoveList {view} onGoto={goto} />
      {#if hasEngine && view.players}
        <EnginePanel players={view.players} resetKey={matchKey} />
      {/if}
      <div class="nav">
        <button aria-label="Start" onclick={() => goto(0)} title="Start (Home)">⏮</button>
        <button aria-label="Back" onclick={() => goto(view!.ply - 1)} title="Back (←)">◀</button>
        <button aria-label="Forward" onclick={() => goto(view!.ply + 1)} title="Forward (→); at the latest move, replays it">▶</button>
        <button aria-label="End" onclick={() => goto(view!.moves.length)} title="End (End)">⏭</button>
      </div>
    {/if}
    <div class="tools">
      <button onclick={() => (showNewGame = true)}>New game…</button>
      <button onclick={() => (showEngines = true)}>Engines…</button>
      <button onclick={openRecord}>Record…</button>
      <button onclick={() => (flipped = !flipped)}>Flip</button>
      <button onclick={() => (showSettings = true)}>Settings…</button>
    </div>
  </aside>
</main>

{#if record != null}
  <RecordDialog initial={record} onLoad={loadRecord} onClose={() => (record = null)} />
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
  <GameEndDialog
    {view}
    onRematch={rematch}
    onSwapSides={swappedSpec ? () => startGame(swappedSpec!) : undefined}
    onClose={() => (showGameEnd = false)}
  />
{/if}
{#if showSettings}
  <SettingsDialog onClose={() => (showSettings = false)} />
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
    flex: 1;
    container-type: size;
    display: grid;
    place-items: center;
    min-height: 0;
  }
  .board-box {
    width: min(100cqw, 100cqh);
    height: min(100cqw, 100cqh);
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
  .tools {
    display: flex;
    flex-wrap: wrap;
    gap: 6px;
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
