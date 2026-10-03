<script lang="ts">
  // The move that produced the shown position: a faint path per moved piece
  // (dashed for enemy pieces that were pushed or pulled) and a ghost of each
  // piece captured on a trap. Drawn under the pieces. With `pv`, an engine's
  // next move instead, in the engine color and without ghosts.
  import type { LastMoveView } from '../bindings/LastMoveView';
  import type { Square } from '../bindings/Square';
  import { SQ, squareXY } from '../geometry';
  import type { Theme } from '../theme';
  import { lastMoveTrails } from './lastMove';
  import PieceGlyph from './PieceGlyph.svelte';

  let {
    lastMove,
    theme,
    flipped,
    pv = false,
  }: { lastMove: LastMoveView; theme: Theme; flipped: boolean; pv?: boolean } = $props();

  const drawn = $derived(lastMoveTrails(lastMove));

  function center(sq: Square) {
    const p = squareXY(sq, flipped);
    return { x: p.x + SQ / 2, y: p.y + SQ / 2 };
  }

  const HEAD = 24;
  /** How far the arrowhead reaches into the last square: past the edge,
   * but short of the center, where the piece standing there would hide it. */
  const TIP_INSET = 22;

  /** The path through the square centers, ending inside the last square,
   * plus the arrowhead triangle. */
  function geometry(squares: Square[]) {
    const pts = squares.map(center);
    const e = pts[pts.length - 1];
    const s = pts[pts.length - 2];
    const len = Math.hypot(e.x - s.x, e.y - s.y);
    const ux = (e.x - s.x) / len;
    const uy = (e.y - s.y) / len;
    const tip = { x: e.x - ux * (SQ / 2 - TIP_INSET), y: e.y - uy * (SQ / 2 - TIP_INSET) };
    const base = { x: tip.x - ux * HEAD, y: tip.y - uy * HEAD };
    const shaft = [...pts.slice(0, -1), base];
    const head = [
      [tip.x, tip.y],
      [base.x - uy * 16, base.y + ux * 16],
      [base.x + uy * 16, base.y - ux * 16],
    ];
    return {
      shaft: shaft.map((p) => `${p.x},${p.y}`).join(' '),
      head: head.map((p) => p.join(',')).join(' '),
    };
  }

  // Pushed and pulled pieces on top, where their path overlaps the pusher's.
  const ordered = $derived([...drawn.trails].sort((a, b) => Number(a.displaced) - Number(b.displaced)));
</script>

<g class={pv ? 'pv-move' : 'last-move'} aria-hidden="true">
  {#each pv ? [] : drawn.captured as c (c.square)}
    {@const p = squareXY(c.square, flipped)}
    <g class="ghost" transform="translate({p.x}, {p.y})">
      <PieceGlyph piece={c.piece} {theme} />
    </g>
  {/each}
  {#each ordered as t, i (i)}
    {@const g = geometry(t.squares)}
    <g class="trail" class:displaced={t.displaced}>
      <polyline points={g.shaft} />
      <polygon points={g.head} />
    </g>
  {/each}
</g>

<style>
  .last-move,
  .pv-move {
    pointer-events: none;
  }
  .ghost {
    opacity: 0.55;
    filter: grayscale(0.6);
  }
  .trail {
    --c: var(--last-move);
  }
  .trail.displaced {
    --c: var(--last-move-displaced);
  }
  .pv-move .trail {
    --c: var(--pv-move);
  }
  .pv-move .trail.displaced {
    --c: var(--pv-move-displaced);
  }
  polyline {
    fill: none;
    stroke: var(--c);
    stroke-width: 10;
    stroke-linecap: round;
    stroke-linejoin: round;
  }
  .displaced polyline {
    stroke-dasharray: 1 16;
    stroke-width: 11;
  }
  polygon {
    fill: var(--c);
  }
</style>
