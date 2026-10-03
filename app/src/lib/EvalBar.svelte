<script lang="ts">
  // The analysis eval as a bar beside the board: gold's share fills from
  // gold's end, so it follows the board when it's flipped.
  import { formatEval, goldShare } from './analysis';
  import type { Eval } from './bindings/Eval';

  let { evaluation, flipped }: { evaluation: Eval | null; flipped: boolean } = $props();

  const share = $derived(goldShare(evaluation));
  const text = $derived(evaluation ? formatEval(evaluation) : 'no eval yet');
</script>

<div
  class="bar"
  class:flipped
  role="meter"
  aria-label="Evaluation for gold"
  aria-valuemin="0"
  aria-valuemax="100"
  aria-valuenow={Math.round(share * 100)}
  aria-valuetext={text}
  title="Evaluation, from gold's side: {text}"
>
  <div class="gold" style:height="{share * 100}%"></div>
</div>

<style>
  .bar {
    position: relative;
    width: 14px;
    height: 100%;
    border-radius: 3px;
    overflow: hidden;
    background: #5d6168;
    border: 1px solid var(--border);
    display: flex;
    flex-direction: column-reverse;
  }
  .bar.flipped {
    flex-direction: column;
  }
  .gold {
    background: #e8c35a;
    transition: height 0.4s ease-out;
  }
  /* The middle mark: even. */
  .bar::after {
    content: '';
    position: absolute;
    left: 0;
    right: 0;
    top: 50%;
    border-top: 1px solid rgba(0, 0, 0, 0.35);
  }
</style>
