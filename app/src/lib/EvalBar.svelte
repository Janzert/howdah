<script lang="ts">
  // The analysis eval as a bar beside the board: gold's share fills from
  // gold's end, so it follows the board when it's flipped. With no eval
  // for the shown position yet it shows no fill rather than an even game,
  // and keeps the hidden fill at the nearest earlier move's eval, so the
  // first eval grows from there instead of from an end.
  import { formatEval, goldShare } from './analysis';
  import type { Eval } from './bindings/Eval';

  let {
    evaluation,
    earlier = null,
    flipped,
  }: { evaluation: Eval | null; earlier?: Eval | null; flipped: boolean } = $props();

  const share = $derived(goldShare(evaluation));
  const fill = $derived(evaluation ? share : goldShare(earlier));
  const text = $derived(evaluation ? formatEval(evaluation) : 'no eval yet');
</script>

<div
  class="bar"
  class:flipped
  class:unknown={!evaluation}
  role="meter"
  aria-label="Evaluation for gold"
  aria-valuemin="0"
  aria-valuemax="100"
  aria-valuenow={evaluation ? Math.round(share * 100) : undefined}
  aria-valuetext={text}
  title="Evaluation, from gold's side: {text}"
>
  <div class="gold" style:height="{fill * 100}%"></div>
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
  /* No eval yet: neither side's color, hatched. */
  .bar.unknown {
    background: repeating-linear-gradient(-45deg, #6f737a 0 4px, #5d6168 4px 8px);
  }
  .bar.unknown .gold {
    visibility: hidden;
  }
  /* The middle mark: even. */
  .bar:not(.unknown)::after {
    content: '';
    position: absolute;
    left: 0;
    right: 0;
    top: 50%;
    border-top: 1px solid rgba(0, 0, 0, 0.35);
  }
</style>
