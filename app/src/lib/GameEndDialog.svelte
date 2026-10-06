<script lang="ts">
  import type { SessionView } from './bindings/SessionView';
  import { describeResult } from './result';

  interface Props {
    view: SessionView;
    /** Starts the same game again; resolves with an error message, if any.
     * Absent for a game watched on arimaa.com; after one the user played
     * there, a new open game with its settings. */
    onRematch?: () => Promise<string | null>;
    /** Starts the same game with the players' colors swapped; absent when
     * swapping would change nothing. */
    onSwapSides?: () => Promise<string | null>;
    /** Turns analysis on at the final position; absent without an engine. */
    onAnalyse?: () => void;
    onClose: () => void;
    /** The buttons' labels ("Rematch" and "Swap sides" by default), and a
     * tooltip for both: an online game's new game is an open one. */
    rematchLabel?: string;
    swapLabel?: string;
    newGameTitle?: string;
  }
  let {
    view,
    onRematch,
    onSwapSides,
    onAnalyse,
    onClose,
    rematchLabel = 'Rematch',
    swapLabel = 'Swap sides',
    newGameTitle,
  }: Props = $props();

  const text = $derived(view.result ? describeResult(view.result) : null);
  // The winner's name, when it tells the players apart (not "Human" vs "Human").
  const winnerName = $derived.by(() => {
    const p = view.players;
    if (!view.result || !p || p.gold.name === p.silver.name) return null;
    return p[view.result.winner].name;
  });
  let error = $state<string | null>(null);
  let dialog: HTMLDialogElement;

  $effect(() => {
    dialog.showModal();
  });

  async function start(action: () => Promise<string | null>) {
    error = await action();
    if (!error) onClose();
  }
</script>

<dialog bind:this={dialog} onclose={onClose} aria-labelledby="game-end-title">
  {#if text}
    <h2 id="game-end-title">
      <span class="dot {view.result?.winner}"></span>
      {text.title}{#if winnerName}<span class="name"> · {winnerName}</span>{/if}
    </h2>
    <p class="reason">{text.reason}</p>
    {#if view.endDetail}<p class="detail">{view.endDetail}</p>{/if}
  {/if}
  {#if error}<p class="error">{error}</p>{/if}
  <div class="buttons">
    {#if onRematch}
      <button onclick={() => start(onRematch)} title={newGameTitle}>{rematchLabel}</button>
    {/if}
    {#if onSwapSides}
      <button onclick={() => start(onSwapSides)} title={newGameTitle}>{swapLabel}</button>
    {/if}
    <span class="spacer"></span>
    {#if onAnalyse}
      <button
        onclick={() => {
          onAnalyse();
          onClose();
        }}
        title="Close and analyse the final position with an engine"
      >
        Analyse
      </button>
    {/if}
    <!-- svelte-ignore a11y_autofocus -->
    <button class="primary" onclick={onClose} autofocus title="Close, leaving the game on the board to review">
      Review game
    </button>
  </div>
</dialog>

<style>
  dialog {
    width: min(420px, 90vw);
    border: 1px solid var(--border);
    border-radius: 8px;
    background: var(--panel);
    color: var(--text);
    padding: 16px;
  }
  dialog::backdrop {
    background: rgba(0, 0, 0, 0.25);
  }
  h2 {
    display: flex;
    align-items: center;
    gap: 8px;
    margin: 0 0 8px;
    font-size: 18px;
  }
  .name {
    font-weight: 400;
    color: var(--muted);
  }
  .dot {
    width: 14px;
    height: 14px;
    border-radius: 50%;
    border: 1px solid rgba(0, 0, 0, 0.4);
    flex: none;
  }
  .dot.gold {
    background: #e3b23c;
  }
  .dot.silver {
    background: #c9ced6;
  }
  .reason {
    margin: 0;
  }
  .detail {
    margin: 4px 0 0;
    font-size: 12px;
    color: var(--muted);
  }
  .error {
    color: var(--warn);
    font-size: 13px;
  }
  .buttons {
    display: flex;
    gap: 6px;
    margin-top: 16px;
    align-items: center;
  }
  .spacer {
    flex: 1;
  }
</style>
