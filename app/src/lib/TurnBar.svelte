<script lang="ts">
  import type { SessionView } from './bindings/SessionView';
  import type { WinReason } from './bindings/WinReason';
  import { squareName } from './geometry';

  interface Props {
    view: SessionView;
    onCommit: () => void;
    onUndo: () => void;
    onCancel: () => void;
    onMoveNow: () => void;
    onEndMatch: () => void;
  }
  let { view, onCommit, onUndo, onCancel, onMoveNow, onEndMatch }: Props = $props();
  const inMatch = $derived(view.players != null);
  const browsing = $derived(inMatch && view.livePly !== view.ply && view.result == null);
  // In a match, a move that isn't the human's live move is a plan.
  const planning = $derived(inMatch && !view.playsLive && view.result == null);

  const side = $derived(view.position.sideToMove === 'gold' ? 'Gold' : 'Silver');
  const turn = $derived(view.turn);
  const hasDraft = $derived(view.phase === 'setup' && view.movesAfterCursor === 0 && view.canInput);
  const reason: Record<WinReason, string> = {
    goal: 'goal',
    elimination: 'elimination',
    immobilization: 'immobilization',
    timeout: 'timeout',
    resignation: 'resignation',
    illegalMove: 'illegal move',
    score: 'score',
    forfeit: 'forfeit',
  };
</script>

<div class="turnbar">
  {#if view.result}
    <div class="status">
      <strong>{view.result.winner === 'gold' ? 'Gold' : 'Silver'} wins</strong> by {reason[view.result.reason]}
    </div>
    {#if view.endDetail}<div class="note">{view.endDetail}</div>{/if}
  {/if}
  {#if view.phase === 'over'}
    <!-- Decided on the board: nothing can follow. -->
  {:else if view.thinking && !view.turn}
    <div class="status">
      <span class="dot {view.thinking}"></span>
      <strong>{view.players?.[view.thinking].name ?? 'Engine'}</strong> is thinking…
    </div>
    {#if browsing}<div class="note">Away from the live game; press End to follow it.</div>{/if}
    <div class="buttons">
      <button onclick={onMoveNow}>Move now <kbd>Space</kbd></button>
    </div>
  {:else if browsing && !view.turn}
    <div class="status">Away from the live game</div>
    <div class="note">Press End to return to the live game.</div>
    {#if view.planMove}
      <div class="buttons">
        <button class="primary" onclick={onCommit} title="Play this plan's first move in the game">
          Play {view.planMove} <kbd>⏎</kbd>
        </button>
      </div>
    {/if}
  {:else if view.phase === 'setup'}
    <div class="status">
      <span class="dot {view.position.sideToMove}"></span>
      {#if hasDraft}
        <strong>{side} setup</strong> — drag pieces to swap them
      {:else}
        {side} setup
      {/if}
    </div>
    {#if hasDraft}
      <div class="buttons">
        <button class="primary" onclick={onCommit}>Confirm setup <kbd>⏎</kbd></button>
      </div>
    {/if}
  {:else}
    <div class="status">
      <span class="dot {view.position.sideToMove}"></span>
      <strong>{side} to move</strong>{#if view.result}&nbsp;(analysis){/if}
      {#if turn}· step {turn.steps.length}/4{/if}
    </div>
    {#if turn?.pushPending != null}
      <div class="note warn">Finish the push: step a stronger piece into {squareName(turn.pushPending)}</div>
    {:else if turn?.commitBlocker}
      <div class="note">{turn.commitBlocker}</div>
    {/if}
    {#if turn && view.result}
      <div class="note">The game is over; committing adds analysis after it.</div>
    {:else if turn && planning}
      <div class="note">Planning: committing adds a variation; it isn't played.</div>
    {:else if turn && view.movesAfterCursor > 0}
      <div class="note">Committing adds a variation.</div>
    {/if}
    <div class="buttons">
      <button onclick={onUndo} disabled={!turn}>Undo step <kbd>⌫</kbd></button>
      <button onclick={onCancel} disabled={!turn}>Reset <kbd>Esc</kbd></button>
      <button class="primary" onclick={onCommit} disabled={!turn || turn.commitBlocker != null}>
        Commit <kbd>⏎</kbd>
      </button>
    </div>
  {/if}
  {#if inMatch && !view.result}
    <div class="buttons">
      <button class="subtle" onclick={onEndMatch} title="Stop the match; the game stays for analysis">Stop match</button>
    </div>
  {/if}
</div>

<style>
  .turnbar {
    display: flex;
    flex-direction: column;
    gap: 6px;
  }
  .status {
    display: flex;
    align-items: center;
    gap: 6px;
  }
  .dot {
    width: 12px;
    height: 12px;
    border-radius: 50%;
    border: 1px solid rgba(0, 0, 0, 0.4);
  }
  .dot.gold {
    background: #e3b23c;
  }
  .dot.silver {
    background: #c9ced6;
  }
  .note {
    font-size: 12px;
    color: var(--muted);
  }
  .note.warn {
    color: var(--warn);
  }
  .buttons {
    display: flex;
    gap: 6px;
    flex-wrap: wrap;
  }
  .subtle {
    font-size: 12px;
    padding: 2px 8px;
    color: var(--muted);
  }
  kbd {
    font-size: 10px;
    opacity: 0.7;
  }
</style>
