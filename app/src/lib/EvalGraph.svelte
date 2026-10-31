<script lang="ts">
  // The main line's evals as a graph: gold's share of the eval bar over the
  // game, gold's side up. Clicking shows the move nearest the pointer.
  import { evalGraph, formatEval } from './analysis';
  import type { MoveNodeView } from './bindings/MoveNodeView';
  import type { NodeId } from './bindings/NodeId';

  interface Props {
    tree: MoveNodeView[];
    cursor: NodeId;
    onGoto: (id: NodeId) => void;
  }
  let { tree, cursor, onGoto }: Props = $props();

  const WIDTH = 300;
  const HEIGHT = 60;

  const graph = $derived(evalGraph(tree));
  const x = (at: number) => at * WIDTH;
  const y = (share: number) => (1 - share) * HEIGHT;
  const line = $derived(graph.map((p) => `${x(p.at)},${y(p.share)}`).join(' '));
  const area = $derived(
    graph.length ? `${x(graph[0].at)},${HEIGHT} ${line} ${x(graph[graph.length - 1].at)},${HEIGHT}` : '',
  );
  const here = $derived(graph.find((p) => p.id === cursor));

  function click(e: MouseEvent) {
    const box = (e.currentTarget as Element).getBoundingClientRect();
    const at = (e.clientX - box.left) / box.width;
    const nearest = graph.reduce((a, b) => (Math.abs(b.at - at) < Math.abs(a.at - at) ? b : a));
    onGoto(nearest.id);
  }
</script>

{#if graph.length > 1}
  <!-- A pointer shortcut: the move list reaches the same moves by keyboard. -->
  <!-- svelte-ignore a11y_click_events_have_key_events, a11y_no_static_element_interactions -->
  <svg
    viewBox="0 0 {WIDTH} {HEIGHT}"
    preserveAspectRatio="none"
    aria-label="Evaluation over the game, from gold's side"
    onclick={click}
  >
    <polygon class="area" points={area} />
    <line class="even" x1="0" x2={WIDTH} y1={HEIGHT / 2} y2={HEIGHT / 2} />
    <polyline class="line" points={line} />
    {#if here}<line class="here" x1={x(here.at)} x2={x(here.at)} y1="0" y2={HEIGHT} />{/if}
    {#if here}<title>{here.label} {formatEval(here.eval)}</title>{/if}
  </svg>
{/if}

<style>
  svg {
    display: block;
    width: 100%;
    height: 60px;
    border: 1px solid var(--border);
    border-radius: 3px;
    background: #5d6168;
    cursor: pointer;
  }
  .area {
    fill: #e8c35a;
  }
  .even {
    stroke: rgba(0, 0, 0, 0.35);
    vector-effect: non-scaling-stroke;
  }
  .line {
    fill: none;
    stroke: rgba(0, 0, 0, 0.6);
    stroke-width: 1;
    vector-effect: non-scaling-stroke;
  }
  .here {
    stroke: var(--accent);
    stroke-width: 2;
    vector-effect: non-scaling-stroke;
  }
</style>
