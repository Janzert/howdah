<script lang="ts">
  import type { SessionView } from './bindings/SessionView';
  import type { WinReason } from './bindings/WinReason';
  import { squareName } from './geometry';

  interface Props {
    view: SessionView;
    onCommit: () => void;
    onUndo: () => void;
    onTakeBack: () => void;
    onGotoLive: () => void;
    onCancel: () => void;
    onMoveNow: () => void;
    onEndMatch: () => void;
  }
  let { view, onCommit, onUndo, onTakeBack, onGotoLive, onCancel, onMoveNow, onEndMatch }: Props = $props();
  const inMatch = $derived(view.players != null);
  /** Both sides play elsewhere: a game watched on arimaa.com. */
  const watching = $derived(view.players?.gold.kind === 'remote' && view.players.silver.kind === 'remote');
  /** The user plays a game on a server (arimaa.com) against a remote side. */
  const online = $derived(
    !watching && (view.players?.gold.kind === 'remote' || view.players?.silver.kind === 'remote'),
  );
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
    <div class="buttons">
      {#if browsing}<button onclick={onGotoLive}>Back to live game <kbd>End</kbd></button>{/if}
      <button onclick={onMoveNow}>Move now <kbd>Space</kbd></button>
      {#if view.canTakeBack}<button onclick={onTakeBack}>Take back</button>{/if}
    </div>
  {:else if browsing && !view.turn}
    <div class="status">Away from the live game</div>
    <div class="buttons">
      <button onclick={onGotoLive}>Back to live game <kbd>End</kbd></button>
      {#if view.planMove}
        <button class="primary" onclick={onCommit} title="Play this plan's first move in the game">
          Play {view.planMove} <kbd>⏎</kbd>
        </button>
      {/if}
    </div>
  {:else if view.phase === 'setup'}
    <div class="status">
      <span class="dot {view.position.sideToMove}"></span>
      {#if hasDraft}<strong>{side} setup</strong>{:else}{side} setup{/if}
    </div>
    {#if hasDraft}
      <div class="note">Drag pieces to swap them.</div>
      <div class="buttons">
        <button class="primary" onclick={onCommit}>Confirm setup <kbd>⏎</kbd></button>
      </div>
    {/if}
  {:else}
    <div class="status">
      <span class="dot {view.position.sideToMove}"></span>
      <strong>{watching && !view.result ? view.players![view.position.sideToMove].name : side} to move</strong
      >{#if view.result}&nbsp;(analysis){/if}
      {#if turn}· step {turn.steps.length}/4{/if}
    </div>
    {#if turn?.pushPending != null}
      <div class="note warn">Finish the push: step a stronger piece into {squareName(turn.pushPending)}</div>
    {:else if turn?.commitBlocker}
      <div class="note">{turn.commitBlocker}</div>
    {/if}
    {#if turn && view.result}
      <div class="note">The game is over; moves are added as analysis after it.</div>
    {:else if turn && planning}
      <div class="note">Planning: this turn is added as a variation, not played.</div>
    {:else if turn && view.movesAfterCursor > 0}
      <div class="note">Playing this adds a variation.</div>
    {/if}
    <!-- Watching, the buttons wait until the user starts planning a move. -->
    {#if !watching || turn}
      <div class="buttons">
        <button onclick={onUndo} disabled={!view.canUndo}>Undo step <kbd>⌫</kbd></button>
        <button onclick={onCancel} disabled={!turn}>Reset <kbd>Esc</kbd></button>
        <button
          class="primary"
          onclick={onCommit}
          disabled={!turn || turn.commitBlocker != null}
          title={planning ? 'End this turn; the other side moves next in the plan' : 'Play this move'}
        >
          {planning ? 'End turn' : 'Play'} <kbd>⏎</kbd>
        </button>
      </div>
    {/if}
    {#if browsing}
      <div class="buttons">
        <button onclick={onGotoLive}>Back to live game <kbd>End</kbd></button>
      </div>
    {/if}
  {/if}
  {#if inMatch && !view.result}
    <div class="buttons">
      {#if view.canTakeBack && !view.thinking}
        <button class="subtle" onclick={onTakeBack} title="Undo played moves back to your last move">Take back</button>
      {/if}
      {#if watching}
        <button class="subtle" onclick={onEndMatch} title="Stop following the game; it stays for analysis">
          Stop watching
        </button>
      {:else if online}
        <button
          class="subtle"
          onclick={onEndMatch}
          title="Stop playing here. The game goes on at arimaa.com with your clock running; take your seat again from Your games in the lobby's arimaa.com list. Analysis stays off until you load another game."
          >Leave game</button
        >
      {:else}
        <button class="subtle" onclick={onEndMatch} title="Stop the match; the game stays for analysis">Stop match</button>
      {/if}
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
