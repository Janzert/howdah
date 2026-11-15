<script lang="ts">
  // The turn's buttons, beside the bottom player bar: shown only while a
  // turn or a setup is being entered, close to the board.
  import type { SessionView } from './bindings/SessionView';

  interface Props {
    view: SessionView;
    onCommit: () => void;
    onUndo: () => void;
    onCancel: () => void;
  }
  let { view, onCommit, onUndo, onCancel }: Props = $props();

  const turn = $derived(view.turn);
  const inMatch = $derived(view.players != null);
  // In a match, a move that isn't the human's live move is a plan.
  const planning = $derived(inMatch && !view.playsLive && view.result == null);
</script>

<div class="actions" role="toolbar" aria-label="Turn">
  {#if view.phase === 'setup'}
    <button class="primary" onclick={onCommit} title="Confirm the setup (Enter)">Confirm setup</button>
  {:else if turn}
    <span class="steps">step {turn.steps.length}/4</span>
    <button onclick={onUndo} disabled={!view.canUndo} title="Undo the last step (Backspace)">Undo</button>
    <button onclick={onCancel} title="Take back this turn's steps (Esc)">Reset</button>
    <button
      class="primary"
      onclick={onCommit}
      disabled={turn.commitBlocker != null}
      title={turn.commitBlocker ??
        (planning ? 'End this turn; the other side moves next in the plan (Enter)' : 'Play this move (Enter)')}
    >
      {planning ? 'End turn' : 'Play'}
    </button>
  {/if}
</div>

<style>
  .actions {
    display: flex;
    align-items: center;
    gap: 4px;
    flex: none;
  }
  .steps {
    margin-right: 2px;
    font-size: 12px;
    color: var(--muted);
    font-variant-numeric: tabular-nums;
  }
  button {
    padding: 2px 10px;
    font-size: 13px;
  }
</style>
