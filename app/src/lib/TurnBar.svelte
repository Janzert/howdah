<script lang="ts">
  import type { SessionView } from './bindings/SessionView';
  import type { WinReason } from './bindings/WinReason';
  import { squareName } from './geometry';

  interface Props {
    view: SessionView;
    onCommit: () => void;
    onUndo: () => void;
    onCancel: () => void;
  }
  let { view, onCommit, onUndo, onCancel }: Props = $props();

  const side = $derived(view.position.sideToMove === 'gold' ? 'Gold' : 'Silver');
  const turn = $derived(view.turn);
  const hasDraft = $derived(view.phase === 'setup' && view.movesAfterCursor === 0);
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
      <strong>{side} to move</strong>
      {#if turn}· step {turn.steps.length}/4{/if}
    </div>
    {#if turn?.pushPending != null}
      <div class="note warn">Finish the push: step a stronger piece into {squareName(turn.pushPending)}</div>
    {:else if turn?.commitBlocker}
      <div class="note">{turn.commitBlocker}</div>
    {/if}
    {#if turn && view.movesAfterCursor > 0}
      <div class="note warn">
        Committing replaces {view.movesAfterCursor} later move{view.movesAfterCursor === 1 ? '' : 's'}
      </div>
    {/if}
    <div class="buttons">
      <button onclick={onUndo} disabled={!turn}>Undo step <kbd>⌫</kbd></button>
      <button onclick={onCancel} disabled={!turn}>Reset <kbd>Esc</kbd></button>
      <button class="primary" onclick={onCommit} disabled={!turn || turn.commitBlocker != null}>
        Commit <kbd>⏎</kbd>
      </button>
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
  kbd {
    font-size: 10px;
    opacity: 0.7;
  }
</style>
