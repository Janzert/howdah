<script lang="ts">
  import type { SessionView } from './bindings/SessionView';
  import { describeResult } from './result';

  interface Props {
    view: SessionView;
    /** Starts the same game again; resolves with an error message, if any.
     * Absent for an arimaa.com game. */
    onRematch?: () => Promise<string | null>;
    /** Starts the same game with the players' colors swapped; absent when
     * swapping would change nothing. */
    onSwapSides?: () => Promise<string | null>;
    /** Turns analysis on at the final position; absent without an engine. */
    onAnalyse?: () => void;
    onClose: () => void;
    /** A line under the result (an arimaa.com game's chat staying open). */
    note?: string;
    /** In a window following an engine test: `onNext` goes to its next
     * game, after a countdown unless the user chooses first; `waiting`
     * says why it hasn't opened yet. Closing the dialog stops following. */
    follow?: { waiting: string | null; onNext: () => void };
  }
  let { view, onRematch, onSwapSides, onAnalyse, onClose, note, follow }: Props = $props();

  /** Seconds before a followed test's next game opens. */
  const FOLLOW_SECONDS = 15;
  let countdown = $state(FOLLOW_SECONDS);
  let movedOn = $state(false);

  function next() {
    movedOn = true;
    follow?.onNext();
  }

  $effect(() => {
    if (!follow || movedOn) return;
    const timer = setInterval(() => {
      countdown -= 1;
      if (countdown <= 0) next();
    }, 1000);
    return () => clearInterval(timer);
  });

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
  {#if note}<p class="detail">{note}</p>{/if}
  {#if follow}
    <p class="detail" role="status">
      {movedOn ? (follow.waiting ?? 'Opening the next game…') : `Next game of the test in ${countdown} s`}
    </p>
  {/if}
  {#if error}<p class="error">{error}</p>{/if}
  <div class="buttons">
    {#if onRematch}
      <button onclick={() => start(onRematch)}>Rematch</button>
    {/if}
    {#if onSwapSides}
      <button onclick={() => start(onSwapSides)}>Swap sides</button>
    {/if}
    {#if follow && !movedOn}
      <button onclick={next} title="Go on to the test's next game now">Next game</button>
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
    <button class="primary" onclick={onClose} autofocus title={follow
        ? 'Close and stay on this game, no longer following the test'
        : 'Close, leaving the game on the board to review'}>
      Review game
    </button>
  </div>
</dialog>

<style>
  dialog {
    width: min(420px, 90vw);
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
