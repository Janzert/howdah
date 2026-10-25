<script lang="ts">
  import { SQ, squareXY } from '../geometry';
  import type { Annotations, AnnotationColor, Arrow } from './annotations.svelte';

  let { annotations, flipped }: { annotations: Annotations; flipped: boolean } = $props();

  const colorVar: Record<AnnotationColor, string> = {
    primary: 'var(--arrow)',
    red: 'rgba(210, 40, 40, 0.8)',
    blue: 'rgba(30, 110, 210, 0.8)',
  };

  function center(sq: number) {
    const p = squareXY(sq, flipped);
    return { x: p.x + SQ / 2, y: p.y + SQ / 2 };
  }

  /** Arrow shaft ending short of the target center so the head sits on it. */
  function geometry(a: Arrow) {
    const s = center(a.from);
    const e = center(a.to);
    const len = Math.hypot(e.x - s.x, e.y - s.y);
    const ux = (e.x - s.x) / len;
    const uy = (e.y - s.y) / len;
    const head = 34;
    return {
      x1: s.x + ux * 18,
      y1: s.y + uy * 18,
      x2: e.x - ux * head,
      y2: e.y - uy * head,
      // Arrowhead triangle at the target center.
      points: [
        [e.x, e.y],
        [e.x - ux * head - uy * 22, e.y - uy * head + ux * 22],
        [e.x - ux * head + uy * 22, e.y - uy * head - ux * 22],
      ]
        .map((p) => p.join(','))
        .join(' '),
    };
  }

  const allArrows = $derived(annotations.preview ? [...annotations.arrows, annotations.preview] : annotations.arrows);
</script>

<g class="annotations">
  {#each annotations.highlights as h (h.square)}
    {@const p = squareXY(h.square, flipped)}
    <rect
      x={p.x + 4}
      y={p.y + 4}
      width={SQ - 8}
      height={SQ - 8}
      rx="8"
      fill="none"
      stroke={colorVar[h.color]}
      stroke-width="8"
    />
  {/each}
  {#each allArrows as a, i (i)}
    {@const g = geometry(a)}
    <g fill={colorVar[a.color]} stroke={colorVar[a.color]} opacity={a === annotations.preview ? 0.6 : 1}>
      <line x1={g.x1} y1={g.y1} x2={g.x2} y2={g.y2} stroke-width="16" stroke-linecap="round" />
      <polygon points={g.points} stroke="none" />
    </g>
  {/each}
</g>

<style>
  .annotations {
    pointer-events: none;
  }
</style>
