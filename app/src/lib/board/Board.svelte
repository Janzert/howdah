<script lang="ts">
  import type { Square } from '../bindings/Square';
  import type { StepTarget } from '../bindings/StepTarget';
  import { SQ, squareAt, squareXY } from '../geometry';
  import { viewBox, type Theme } from '../theme';
  import AnnotationLayer from './AnnotationLayer.svelte';
  import { Annotations, colorFor, type AnnotationColor } from './annotations.svelte';
  import BoardSurface from './BoardSurface.svelte';
  import type { BoardModel } from './boardModel.svelte';
  import { DragPath } from './dragPath';
  import PieceGlyph from './PieceGlyph.svelte';

  interface Props {
    model: BoardModel;
    theme: Theme;
    flipped: boolean;
    /** Whether pieces can be dragged right now. */
    interactive: boolean;
    pushPending: Square | null;
    /** Called with a dropped piece's move and the squares it was dragged
     * across; resolve false to slide it back. */
    onDrop: (from: Square, to: Square, path: Square[]) => Promise<boolean>;
    legalTargets: (from: Square) => Promise<StepTarget[]>;
    /** Squares a drop on `to` would walk the piece through, or null. */
    planRoute: (from: Square, to: Square, path: Square[]) => Promise<Square[] | null>;
  }

  let { model, theme, flipped, interactive, pushPending, onDrop, legalTargets, planRoute }: Props = $props();

  const annotations = new Annotations();
  let svg: SVGSVGElement;

  interface Drag {
    id: number;
    from: Square;
    x: number;
    y: number;
    pointerId: number;
  }
  let drag = $state<Drag | null>(null);
  let targets = $state<StepTarget[]>([]);
  /** Where a drop on the hovered square would walk the dragged piece. */
  let route = $state<{ to: Square; squares: Square[] } | null>(null);
  let path: DragPath | null = null;
  let planned: { to: Square; hint: string } | null = null;
  let rightStart: { square: Square; color: AnnotationColor } | null = null;

  const dragged = $derived(drag ? model.find(drag.id) : undefined);

  /** Pointer position in board units (the playing grid spans 0..800). */
  function toBoard(e: PointerEvent): { x: number; y: number } {
    const ctm = svg.getScreenCTM();
    if (!ctm) return { x: -1, y: -1 };
    const p = new DOMPoint(e.clientX, e.clientY).matrixTransform(ctm.inverse());
    return { x: p.x, y: p.y };
  }

  function squareFor(e: PointerEvent): Square | null {
    const p = toBoard(e);
    return squareAt(p.x, p.y, flipped);
  }

  async function onpointerdown(e: PointerEvent) {
    const sq = squareFor(e);
    if (e.button === 2) {
      if (sq == null) return;
      rightStart = { square: sq, color: colorFor(e) };
      svg.setPointerCapture(e.pointerId);
      return;
    }
    if (e.button !== 0) return;
    annotations.clear();
    if (!interactive || model.animating || sq == null) return;
    const piece = model.pieces.find((p) => p.square === sq && p.fading === null);
    if (!piece) return;
    svg.setPointerCapture(e.pointerId);
    const p = toBoard(e);
    drag = { id: piece.id, from: sq, x: p.x, y: p.y, pointerId: e.pointerId };
    path = new DragPath(sq, p, (q) => squareAt(q.x, q.y, flipped));
    planned = null;
    route = null;
    const t = await legalTargets(sq);
    if (drag?.id === piece.id) targets = t;
  }

  function onpointermove(e: PointerEvent) {
    if (drag && e.pointerId === drag.pointerId) {
      const p = toBoard(e);
      drag.x = p.x;
      drag.y = p.y;
      path?.moveTo(p);
      updateRoute(drag.from, squareFor(e));
    }
    if (rightStart) {
      const sq = squareFor(e);
      annotations.preview =
        sq != null && sq !== rightStart.square
          ? { from: rightStart.square, to: sq, color: rightStart.color }
          : null;
    }
  }

  /** Asks for the route to `to` when the target or the dragged path changes. */
  function updateRoute(from: Square, to: Square | null) {
    if (to == null || to === from || !path) {
      planned = null;
      route = null;
      return;
    }
    const hint = path.hint;
    const key = { to, hint: hint.join() };
    if (planned && planned.to === key.to && planned.hint === key.hint) return;
    planned = key;
    planRoute(from, to, hint).then((squares) => {
      if (planned !== key) return;
      route = squares ? { to, squares } : null;
    });
  }

  function endDrag() {
    drag = null;
    targets = [];
    path = null;
    planned = null;
    route = null;
  }

  async function onpointerup(e: PointerEvent) {
    const sq = squareFor(e);
    if (e.button === 2 && rightStart) {
      if (sq === rightStart.square) annotations.toggleHighlight(sq, rightStart.color);
      else if (sq != null) annotations.toggleArrow(rightStart.square, sq, rightStart.color);
      rightStart = null;
      annotations.preview = null;
      return;
    }
    if (e.button !== 0 || !drag) return;
    const d = drag;
    const hint = path?.hint ?? [];
    endDrag();
    if (sq == null || sq === d.from) return;
    model.dropAt(d.id, sq);
    if (!(await onDrop(d.from, sq, hint))) model.revert(d.id, d.from);
  }

  function onpointercancel() {
    endDrag();
    rightStart = null;
    annotations.preview = null;
  }

  function center(sq: Square): string {
    const p = squareXY(sq, flipped);
    return `${p.x + SQ / 2},${p.y + SQ / 2}`;
  }

  function translate(sq: Square) {
    const p = squareXY(sq, flipped);
    return `transform: translate(${p.x}px, ${p.y}px)`;
  }
</script>

<div
  class="board"
  style:--highlight={theme.ui.highlight}
  style:--arrow={theme.ui.arrow}
  style:--target={theme.ui.target}
  style:--push={theme.ui.pushPending}
  style:--step-ms="{model.stepMs}ms"
  style:--fade-ms="{model.fadeMs}ms"
>
  <svg
    bind:this={svg}
    viewBox={viewBox(theme)}
    preserveAspectRatio="xMidYMid meet"
    role="application"
    aria-label="Arimaa board"
    {onpointerdown}
    {onpointermove}
    {onpointerup}
    {onpointercancel}
    oncontextmenu={(e) => e.preventDefault()}
  >
    <BoardSurface {theme} {flipped} />

    <g class="hints">
      {#if pushPending != null}
        {@const p = squareXY(pushPending, flipped)}
        <rect class="push" x={p.x + 5} y={p.y + 5} width={SQ - 10} height={SQ - 10} rx="10" />
      {/if}
      {#each targets as t (t.to)}
        {@const p = squareXY(t.to, flipped)}
        <circle
          class="target"
          class:enemy={t.kind === 'pushStart' || t.kind === 'pullFinish'}
          class:far={t.steps > 1}
          cx={p.x + SQ / 2}
          cy={p.y + SQ / 2}
          r={t.steps > 1 ? 8 : 14}
        />
      {/each}
      {#if drag && route}
        <polyline class="route" points={[drag.from, ...route.squares].map(center).join(' ')} />
      {/if}
    </g>

    <g class="pieces">
      {#each model.pieces as p (p.id)}
        <g
          class="piece"
          class:instant={p.instant}
          class:hidden={drag?.id === p.id}
          style={translate(p.square)}
        >
          <g class="glyph" class:fade-out={p.fading === 'out'} class:fade-in={p.fading === 'in'}>
            <PieceGlyph piece={p.piece} {theme} />
            {#if p.frozen && !model.animating}
              <circle class="frozen" cx={SQ - 14} cy="14" r="7" />
            {/if}
          </g>
        </g>
      {/each}
    </g>

    <AnnotationLayer {annotations} {flipped} />

    {#if drag && dragged}
      <g class="ghost" transform="translate({drag.x - SQ / 2}, {drag.y - SQ / 2}) scale(1.08)">
        <PieceGlyph piece={dragged.piece} {theme} />
      </g>
      {#if route && route.squares.length > 1}
        <!-- Steps the drop would take, beside the dragged piece. -->
        <g class="route-count" transform="translate({drag.x + SQ / 2}, {drag.y - SQ / 2})">
          <circle r="16" />
          <text dy="0.35em">{route.squares.length}</text>
        </g>
      {/if}
    {/if}
  </svg>
</div>

<style>
  .board {
    width: 100%;
    height: 100%;
  }
  svg {
    display: block;
    width: 100%;
    height: 100%;
    touch-action: none;
    user-select: none;
  }
  .piece {
    transition: transform var(--step-ms, 220ms) ease-in-out;
    cursor: grab;
  }
  .piece.instant {
    transition: none;
  }
  .piece.hidden {
    visibility: hidden;
  }
  .glyph {
    transform-box: fill-box;
    transform-origin: center;
    transition:
      opacity var(--fade-ms, 260ms) ease-in,
      transform var(--fade-ms, 260ms) ease-in;
  }
  .glyph.fade-out {
    opacity: 0;
    transform: scale(0.2);
  }
  .glyph.fade-in {
    animation: appear var(--fade-ms, 260ms) ease-out;
  }
  @keyframes appear {
    from {
      opacity: 0;
      transform: scale(0.2);
    }
  }
  .frozen {
    fill: rgba(70, 140, 255, 0.9);
    stroke: white;
    stroke-width: 2;
  }
  .ghost {
    pointer-events: none;
    filter: drop-shadow(0 6px 6px rgba(0, 0, 0, 0.35));
  }
  .hints {
    pointer-events: none;
  }
  .target {
    fill: var(--target);
  }
  .target.enemy {
    fill: var(--push);
  }
  .target.far {
    opacity: 0.6;
  }
  .route {
    fill: none;
    stroke: var(--target);
    stroke-width: 8;
    stroke-linecap: round;
    stroke-linejoin: round;
    stroke-dasharray: 2 14;
  }
  .route-count {
    pointer-events: none;
  }
  .route-count circle {
    fill: rgba(20, 20, 20, 0.75);
    stroke: white;
    stroke-width: 2;
  }
  .route-count text {
    fill: white;
    font: bold 18px sans-serif;
    text-anchor: middle;
  }
  .push {
    fill: none;
    stroke: var(--push);
    stroke-width: 6;
    stroke-dasharray: 14 8;
  }
</style>
