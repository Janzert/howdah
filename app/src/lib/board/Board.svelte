<script lang="ts">
  import { onMount, untrack } from 'svelte';
  import { fade } from 'svelte/transition';
  import { SvelteMap } from 'svelte/reactivity';
  import type { Square } from '../bindings/Square';
  import type { StepTarget } from '../bindings/StepTarget';
  import { registerBoard } from '../devHooks';
  import { SQ, squareAt, squareName, squareXY } from '../geometry';
  import type { LastMoveView } from '../bindings/LastMoveView';
  import type { Coordinates, HoverInput } from '../settings.svelte';
  import { LAST_MOVE_COLORS, PV_MOVE_COLORS, viewBox, type Theme } from '../theme';
  import AnnotationLayer from './AnnotationLayer.svelte';
  import { Annotations, colorFor, type AnnotationColor } from './annotations.svelte';
  import BoardSurface from './BoardSurface.svelte';
  import FrozenFilter from './FrozenFilter.svelte';
  import LastMoveLayer from './LastMoveLayer.svelte';
  import type { BoardModel, DisplayPiece } from './boardModel.svelte';
  import { DragPath } from './dragPath';
  import { stepChoice, type StepArrow } from './hoverInput';
  import PieceGlyph from './PieceGlyph.svelte';

  interface Props {
    model: BoardModel;
    theme: Theme;
    flipped: boolean;
    /** Whether pieces can be dragged right now. */
    interactive: boolean;
    pushPending: Square | null;
    /** The move that produced the shown position, drawn when the board is at rest. */
    lastMove: LastMoveView | null;
    /** The analysis engine's next move, drawn over the last move when at rest. */
    pvMove?: LastMoveView | null;
    coordinates: Coordinates;
    /** Input from pointer movement: arrows for a hovered piece's legal steps,
     * or step mode (the step toward the pointer, taken with a click). */
    hoverInput: HoverInput;
    /** Changes whenever legal steps may have changed, to refresh the hover arrows. */
    positionKey: string;
    /** Takes a single step (a click on a hover arrow); resolves whether it was accepted. */
    onStep: (from: Square, to: Square) => Promise<boolean>;
    /** Called with a dropped piece's move and the squares it was dragged
     * across; resolve false to slide it back. */
    onDrop: (from: Square, to: Square, path: Square[]) => Promise<boolean>;
    legalTargets: (from: Square) => Promise<StepTarget[]>;
    /** Squares a drop on `to` would walk the piece through, or null. */
    planRoute: (from: Square, to: Square, path: Square[]) => Promise<Square[] | null>;
  }

  let {
    model,
    theme,
    flipped,
    interactive,
    pushPending,
    lastMove,
    pvMove = null,
    coordinates,
    hoverInput,
    positionKey,
    onStep,
    onDrop,
    legalTargets,
    planRoute,
  }: Props = $props();

  const annotations = new Annotations();
  const uid = $props.id();
  const frozenFilter = `frozen-${uid}`;
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
  /** Step mode: the step a click on a piece takes if it's released on the piece's square. */
  let stepAtDown: { from: Square; to: Square } | null = null;

  const dragged = $derived(drag ? model.find(drag.id) : undefined);

  // Input from pointer movement alone: hover arrows or step mode. Both read
  // each square's legal single steps from `stepsCache`, fetched on demand and
  // cleared when the position changes.
  const stepsCache = new SvelteMap<Square, StepArrow[]>();
  const requested = new Set<Square>();
  let cacheKey = '';
  /** Legal-step requests in flight (for the dev hook's `idle`). */
  let pendingRequests = 0;
  /** The pointer over the board, in board units (mouse only). */
  let pointer = $state<{ x: number; y: number } | null>(null);
  /** Hover arrows: the piece whose arrows are shown. It stays while the
   * pointer is over one of its arrows. */
  let hoverFrom = $state<Square | null>(null);
  const hoverActive = $derived(hoverInput !== 'off' && interactive && !model.animating && drag == null);

  const hasPiece = (sq: Square) => model.pieces.some((p) => p.square === sq && p.fading === null);

  /** A square's legal single steps, or undefined while they're fetched. */
  function stepsFrom(sq: Square): StepArrow[] | undefined {
    if (!hasPiece(sq)) return [];
    const cached = stepsCache.get(sq);
    if (cached || requested.has(sq)) return cached;
    requested.add(sq);
    const key = cacheKey;
    pendingRequests++;
    legalTargets(sq)
      .then((targets) => {
        if (key !== cacheKey) return;
        stepsCache.set(
          sq,
          targets
            .filter((t) => t.steps === 1 && t.kind != null)
            .map((t) => ({ to: t.to, enemy: t.kind === 'pushStart' || t.kind === 'pullFinish' })),
        );
      })
      .finally(() => pendingRequests--);
    return undefined;
  }

  const hover = $derived(
    hoverInput === 'arrows' && hoverActive && hoverFrom != null
      ? { from: hoverFrom, arrows: stepsFrom(hoverFrom) ?? [] }
      : null,
  );
  const step = $derived(
    hoverInput === 'step' && hoverActive && pointer ? stepChoice(pointer, flipped, stepsFrom) : null,
  );
  const pointerSquare = $derived(pointer ? squareAt(pointer.x, pointer.y, flipped) : null);
  const overStep = $derived(
    (hover?.arrows.some((a) => a.to === pointerSquare) ?? false) || (step != null && step.to != null),
  );

  function updatePointer(p: { x: number; y: number } | null) {
    pointer = p;
    const sq = p ? squareAt(p.x, p.y, flipped) : null;
    if (sq == null) {
      hoverFrom = null;
      return;
    }
    const keep =
      hoverFrom != null && (sq === hoverFrom || stepsCache.get(hoverFrom)?.some((a) => a.to === sq));
    if (!keep) hoverFrom = hasPiece(sq) ? sq : null;
  }

  // Legal steps change with the position: start over for the square under the pointer.
  $effect(() => {
    cacheKey = positionKey;
    untrack(() => {
      stepsCache.clear();
      requested.clear();
      hoverFrom = null;
      updatePointer(pointer);
    });
  });

  /** The step a click on `sq` takes, if any: a hover arrow on it, or step
   * mode's offer for the pointer there. */
  function clickStep(sq: Square): { from: Square; to: Square } | null {
    if (hover?.arrows.some((a) => a.to === sq)) return { from: hover.from, to: sq };
    if (step?.to != null) return { from: step.from, to: step.to };
    return null;
  }

  /** Triangle in the half of the target square next to the hovered piece,
   * pointing away from it, its tip short of the square's center (in a frame
   * centered on the target and pointing away from the piece). */
  const ARROW_POINTS = '-40,-17 -8,0 -40,17';
  function hoverArrowTransform(from: Square, to: Square): string {
    const a = squareXY(from, flipped);
    const b = squareXY(to, flipped);
    const angle = (Math.atan2(b.y - a.y, b.x - a.x) * 180) / Math.PI;
    return `translate(${b.x + SQ / 2}, ${b.y + SQ / 2}) rotate(${angle})`;
  }

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
    const clicked = sq == null ? null : clickStep(sq);
    // A step from another square (an arrow, or step mode over an empty
    // square) is taken now; a piece's own step waits for the button to come
    // up on its square, so the piece can still be dragged.
    if (clicked && clicked.from !== sq) {
      onStep(clicked.from, clicked.to);
      return;
    }
    stepAtDown = clicked;
    if (!interactive || model.animating || sq == null) return;
    const piece = model.pieces.find((p) => p.square === sq && p.fading === null);
    if (!piece) return;
    // Keeps WebKit from starting a native drag of the piece image, which
    // stops pointer events mid-drag and swallows the pointerup.
    e.preventDefault();
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
    if (!drag && !rightStart && e.pointerType === 'mouse') updatePointer(toBoard(e));
    if (drag && e.pointerId === drag.pointerId) {
      // The button came up without a pointerup reaching us: drop the drag
      // rather than leave the piece stuck to the pointer.
      if (e.pointerType === 'mouse' && (e.buttons & 1) === 0) {
        onpointercancel();
        return;
      }
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
    const clicked = stepAtDown;
    stepAtDown = null;
    if (sq === d.from && clicked?.from === d.from) {
      onStep(clicked.from, clicked.to);
      return;
    }
    if (sq == null || sq === d.from) return;
    model.dropAt(d.id, sq);
    if (!(await onDrop(d.from, sq, hint))) model.revert(d.id, d.from);
  }

  function onpointercancel() {
    endDrag();
    rightStart = null;
    annotations.preview = null;
  }

  /** Accessible name, e.g. "gold elephant e2, frozen". */
  function pieceLabel(p: DisplayPiece): string {
    return `${p.piece.color} ${p.piece.kind} ${squareName(p.square)}${p.frozen ? ', frozen' : ''}`;
  }

  if (import.meta.env.DEV) {
    onMount(() => {
      registerBoard({
        svg,
        busy: () => pendingRequests > 0,
        clientPoint: (sq) => {
          const p = squareXY(sq, flipped);
          const ctm = svg.getScreenCTM();
          if (!ctm) throw new Error('board not laid out');
          const c = new DOMPoint(p.x + SQ / 2, p.y + SQ / 2).matrixTransform(ctm);
          return { x: c.x, y: c.y };
        },
      });
      return () => registerBoard(null);
    });
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
  style:--last-move={theme.ui.lastMove ?? LAST_MOVE_COLORS.lastMove}
  style:--last-move-displaced={theme.ui.lastMoveDisplaced ?? LAST_MOVE_COLORS.lastMoveDisplaced}
  style:--pv-move={PV_MOVE_COLORS.move}
  style:--pv-move-displaced={PV_MOVE_COLORS.displaced}
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
    ondragstart={(e) => e.preventDefault()}
    onpointerleave={() => updatePointer(null)}
    class:over-arrow={overStep}
    oncontextmenu={(e) => e.preventDefault()}
  >
    <defs>
      <FrozenFilter id={frozenFilter} spec={theme.ui.frozen} />
    </defs>

    <BoardSurface {theme} {flipped} {coordinates} />

    {#if lastMove && !model.animating}
      <LastMoveLayer {lastMove} {theme} {flipped} />
    {/if}
    {#if pvMove && !model.animating}
      <LastMoveLayer lastMove={pvMove} {theme} {flipped} pv />
    {/if}

    <g class="hints">
      {#if pushPending != null}
        {@const p = squareXY(pushPending, flipped)}
        <rect class="push" x={p.x + 5} y={p.y + 5} width={SQ - 10} height={SQ - 10} rx="10" />
      {/if}
      {#each targets as t (t.to)}
        {@const p = squareXY(t.to, flipped)}
        <circle
          data-target={squareName(t.to)}
          data-steps={t.steps}
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
          role="img"
          aria-label={pieceLabel(p)}
          aria-hidden={p.fading === 'out'}
          data-square={squareName(p.square)}
          data-piece="{p.piece.color} {p.piece.kind}"
          class:instant={p.instant}
          class:hidden={drag?.id === p.id}
          style={translate(p.square)}
        >
          <g class="glyph" class:fade-out={p.fading === 'out'} class:fade-in={p.fading === 'in'}>
            <PieceGlyph piece={p.piece} {theme} />
            {#if p.frozen && !model.animating}
              <!-- A frosted copy over the piece. -->
              <g class="frozen" filter="url(#{frozenFilter})" transition:fade={{ duration: model.fadeMs }}>
                <PieceGlyph piece={p.piece} {theme} />
              </g>
            {/if}
          </g>
        </g>
      {/each}
    </g>

    {#if hover || step}
      <g class="hover-arrows">
        {#if hover}
          {#each hover.arrows as a (a.to)}
            <polygon
              class="hover-arrow"
              class:enemy={a.enemy}
              data-hover-target={squareName(a.to)}
              transform={hoverArrowTransform(hover.from, a.to)}
              points={ARROW_POINTS}
            />
          {/each}
        {/if}
        {#if step}
          {@const o = squareXY(step.from, flipped)}
          <rect class="step-origin" x={o.x + 4} y={o.y + 4} width={SQ - 8} height={SQ - 8} rx="10" />
          {#if step.to != null}
            <polygon
              class="hover-arrow"
              class:enemy={step.enemy}
              data-hover-target={squareName(step.to)}
              transform={hoverArrowTransform(step.from, step.to)}
              points={ARROW_POINTS}
            />
          {/if}
        {/if}
      </g>
    {/if}

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
    -webkit-user-drag: none;
    display: block;
    width: 100%;
    height: 100%;
    touch-action: none;
    user-select: none;
  }
  .piece :global(image) {
    -webkit-user-drag: none;
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
    pointer-events: none;
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
  svg.over-arrow {
    cursor: pointer;
  }
  .hover-arrows {
    pointer-events: none;
  }
  .hover-arrow {
    fill: rgba(20, 140, 110, 0.8);
    stroke: rgba(255, 255, 255, 0.7);
    stroke-width: 2;
  }
  .hover-arrow.enemy {
    fill: var(--push);
  }
  .step-origin {
    fill: none;
    stroke: rgba(20, 140, 110, 0.8);
    stroke-width: 5;
  }
  .push {
    fill: none;
    stroke: var(--push);
    stroke-width: 6;
    stroke-dasharray: 14 8;
  }
</style>
