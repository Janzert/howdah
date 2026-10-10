<script lang="ts">
  // A puzzle's question and how the solving goes (Session::load_puzzle):
  // the backend checks each move at the puzzle's position against the
  // solution and plays the replies; this shows where things stand.
  import { api } from './api';
  import type { SessionView } from './bindings/SessionView';
  import { puzzleStatus } from './puzzles';

  interface Props {
    view: SessionView;
    run: (p: Promise<unknown>) => Promise<boolean>;
    /** The next puzzle in arimaa.com's list, if there is one. */
    next: string | null;
    onNext: (id: string) => void;
  }
  let { view, run, next, onNext }: Props = $props();

  const p = $derived(view.puzzle!);
  let askingHint = $state(false);

  async function hint() {
    askingHint = true;
    await run(api.puzzleHint());
    askingHint = false;
  }
</script>

<section class="puzzle" aria-label="Puzzle">
  <div class="head">
    <span class="dot {p.solver}"></span>
    <strong>{p.title ?? 'Puzzle'}</strong>
    {#if p.id}<span class="id">{p.id}</span>{/if}
  </div>
  {#if p.question && p.question !== p.title}<p class="question">{p.question}</p>{/if}
  <p class="status {p.status}" role="status" aria-live="polite">{puzzleStatus(p)}</p>
  {#if p.hint != null}
    <p class="hint">{p.hint || 'This puzzle has no hint.'}</p>
  {/if}
  <div class="buttons">
    {#if !p.atFrontier && (p.status === 'solving' || p.status === 'wrong')}
      <button class="primary" onclick={() => run(api.puzzleRetry())} title="Back to the position to solve">Try again</button>
    {/if}
    {#if p.hintAvailable}
      <button onclick={hint} disabled={askingHint} title="arimaa.com's hint for this puzzle">Hint</button>
    {/if}
    {#if p.hasSolution && p.status !== 'solved' && p.status !== 'shown'}
      <button onclick={() => run(api.puzzleAnswer())} title="Add the rest of the solution to the move list">Show answer</button>
    {/if}
    {#if next}
      <button class:primary={p.status === 'solved' || p.status === 'shown'} onclick={() => onNext(next!)}>Next puzzle</button>
    {/if}
  </div>
  {#if p.author}<p class="author">Composed by {p.author}</p>{/if}
</section>

<style>
  .puzzle {
    display: flex;
    flex-direction: column;
    gap: 4px;
    padding: 8px;
    border: 1px solid var(--border);
    border-radius: 6px;
    font-size: 13px;
  }
  .head {
    display: flex;
    align-items: center;
    gap: 6px;
  }
  .id {
    margin-left: auto;
    color: var(--muted);
    font-size: 12px;
  }
  .dot {
    flex: none;
    width: 10px;
    height: 10px;
    border-radius: 50%;
    border: 1px solid rgba(0, 0, 0, 0.4);
  }
  .dot.gold {
    background: #e3b23c;
  }
  .dot.silver {
    background: #c9ced6;
  }
  p {
    margin: 0;
  }
  .status.solved {
    color: var(--clock-ok);
    font-weight: 600;
  }
  .status.wrong {
    color: var(--warn);
  }
  .hint {
    font-style: italic;
  }
  .author {
    color: var(--muted);
    font-size: 12px;
  }
  .buttons {
    display: flex;
    flex-wrap: wrap;
    gap: 6px;
    margin-top: 2px;
  }
</style>
