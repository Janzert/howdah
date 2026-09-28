<script lang="ts">
  import { onMount } from 'svelte';
  import { api, errorMessage } from './lib/api';
  import type { SessionView } from './lib/bindings/SessionView';
  import type { Square } from './lib/bindings/Square';
  import Board from './lib/board/Board.svelte';
  import { BoardModel } from './lib/board/boardModel.svelte';
  import { on } from './lib/events';
  import MoveList from './lib/MoveList.svelte';
  import RecordDialog from './lib/RecordDialog.svelte';
  import { isMuted, play, setMuted } from './lib/sound';
  import { findTheme, themes } from './lib/theme';
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
  let themeId = $state(findTheme(pref('theme')).id);
  const theme = $derived(findTheme(themeId));
  let flipped = $state(pref('flipped') === '1');
  let muted = $state(isMuted());
  let message = $state<string | null>(null);
  let record = $state<string | null>(null);
  let messageTimer: ReturnType<typeof setTimeout> | undefined;

  $effect(() => savePref('theme', themeId));
  $effect(() => savePref('flipped', flipped ? '1' : '0'));
  $effect(() => setMuted(muted));

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

  onMount(() => {
    const unlisten = on('game://changed', (u) => {
      const hadResult = view?.result != null;
      const setupCommitted = view != null && view.phase === 'setup' && u.view.ply > view.ply && u.animation.length === 0;
      view = u.view;
      model.apply(u.view.position.pieces, u.animation, {
        onSlide: () => play('slide'),
        onCapture: () => play('trapped'),
      });
      if (setupCommitted) play('place');
      if (!hadResult && u.view.result) play('win');
    });
    api.getState().then((v) => {
      view = v;
      model.snap(v.position.pieces);
    });
    return () => {
      unlisten.then((f) => f());
    };
  });

  const interactive = $derived(view != null && view.phase !== 'over');

  function onDrop(from: Square, to: Square): Promise<boolean> {
    if (!view) return Promise.resolve(false);
    return run(view.phase === 'setup' ? api.setupSwap(from, to) : api.tryStep(from, to));
  }

  function commit() {
    if (!view) return;
    if (view.phase === 'setup') run(api.commitSetup());
    else if (view.turn) run(api.commitTurn());
  }

  function goto(ply: number) {
    if (view && ply >= 0 && ply <= view.moves.length && ply !== view.ply) run(api.gotoPly(ply));
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
    if (!view || record != null) return;
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
        commit();
        break;
      case 'Backspace':
        if (view.turn) run(api.undoStep());
        break;
      case 'Escape':
        if (view.turn) run(api.cancelTurn());
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
    <div class="board-box">
      <Board
        {model}
        {theme}
        {flipped}
        {interactive}
        pushPending={view?.turn?.pushPending ?? null}
        {onDrop}
        legalTargets={(from) => api.legalTargets(from)}
      />
    </div>
    {#if message}<div class="message" role="status">{message}</div>{/if}
  </section>

  <aside class="panel">
    {#if view}
      <TurnBar
        {view}
        onCommit={commit}
        onUndo={() => run(api.undoStep())}
        onCancel={() => run(api.cancelTurn())}
      />
      <MoveList {view} onGoto={goto} />
      <div class="nav">
        <button onclick={() => goto(0)} title="Start (Home)">⏮</button>
        <button onclick={() => goto(view!.ply - 1)} title="Back (←)">◀</button>
        <button onclick={() => goto(view!.ply + 1)} title="Forward (→)">▶</button>
        <button onclick={() => goto(view!.moves.length)} title="End (End)">⏭</button>
      </div>
    {/if}
    <div class="tools">
      <button onclick={() => run(api.newGame())}>New game</button>
      <button onclick={openRecord}>Record…</button>
      <button onclick={() => (flipped = !flipped)}>Flip</button>
      <button onclick={() => (muted = !muted)}>{muted ? 'Sound off' : 'Sound on'}</button>
      <select bind:value={themeId} aria-label="Theme">
        {#each themes as t (t.id)}
          <option value={t.id}>{t.name}</option>
        {/each}
      </select>
    </div>
    {#if theme.attribution}<div class="attribution">{theme.attribution}</div>{/if}
  </aside>
</main>

{#if record != null}
  <RecordDialog initial={record} onLoad={loadRecord} onClose={() => (record = null)} />
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
    container-type: size;
    display: grid;
    place-items: center;
    padding: 16px;
    min-width: 0;
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
  .attribution {
    font-size: 11px;
    color: var(--muted);
  }
</style>
