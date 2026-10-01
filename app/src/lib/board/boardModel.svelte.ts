// What the board currently displays, and the animation player that moves it
// from one backend state to the next. Pieces are keyed by stable ids from the
// backend, so a slide is just a change of square with a CSS transition.
import type { AnimStep } from '../bindings/AnimStep';
import type { Piece } from '../bindings/Piece';
import type { PieceView } from '../bindings/PieceView';
import type { Square } from '../bindings/Square';

export interface DisplayPiece {
  id: number;
  piece: Piece;
  square: Square;
  frozen: boolean;
  /** 'out' shrinks and fades a captured piece; 'in' grows a restored one. */
  fading: 'in' | 'out' | null;
  /** Suppress the slide transition (snaps and drag drops). */
  instant: boolean;
}

export interface AnimHooks {
  onSlide?: () => void;
  onCapture?: () => void;
}

/** Default slide duration per step; see `BoardModel.setBaseSpeed`. */
export const STEP_MS = 220;
/** Capture/restore fades last this much longer than a slide. */
const FADE_RATIO = 260 / 220;
/** More moves than this waiting: show each one instantly, `INSTANT_GAP_MS` apart. */
export const MAX_BEHIND = 6;
export const INSTANT_GAP_MS = 150;
/** Slide duration per step when replaying the route of a dropped piece. */
export const ROUTE_STEP_MS = 110;

const sleep = (ms: number) => new Promise((r) => setTimeout(r, ms));
const frame = () => new Promise((r) => requestAnimationFrame(() => r(undefined)));

interface QueuedUpdate {
  final: PieceView[];
  anim: AnimStep[];
  hooks: AnimHooks;
  /** Longest the animation may take (the move's time off the clock). */
  budgetMs: number | null;
}

/** Same pieces on the same squares (ignoring frozen flags). */
function samePosition(a: PieceView[], b: PieceView[]): boolean {
  if (a.length !== b.length) return false;
  const squares = new Map(a.map((p) => [p.id, p.square]));
  return b.every((p) => squares.get(p.id) === p.square);
}

export class BoardModel {
  pieces = $state<DisplayPiece[]>([]);
  animating = $state(false);
  /** Slide duration per step at normal speed. */
  baseStepMs = $state(STEP_MS);
  /** Current slide and fade durations: the base speed, shortened when moves
   * queue up or a move's time budget is short. */
  stepMs = $state(STEP_MS);
  fadeMs = $state(Math.round(STEP_MS * FADE_RATIO));
  private generation = 0;
  /** Final state of the latest update; what the board settles on. */
  private target: PieceView[] = [];
  private queue: QueuedUpdate[] = [];
  /** A piece the user just dropped on its destination. If the next update
   * walks it there in one step, it's already in place; if it took a route of
   * several steps, the piece jumps back and replays them quickly. */
  private dropped: number | null = null;

  /** Sets the normal animation speed (slide duration per step, in ms). */
  setBaseSpeed(stepMs: number) {
    this.baseStepMs = Math.max(0, stepMs);
    if (!this.animating) this.resetSpeed();
  }

  private resetSpeed() {
    this.stepMs = this.baseStepMs;
    this.fadeMs = Math.round(this.baseStepMs * FADE_RATIO);
  }

  /** Shows `pieces` immediately, without transitions. */
  snap(pieces: PieceView[]) {
    this.target = pieces;
    this.pieces = pieces.map((p) => ({ ...p, fading: null, instant: true }));
    const gen = this.generation;
    // Re-enable transitions once the new positions have been painted.
    frame().then(frame).then(() => {
      if (gen !== this.generation) return;
      for (const p of this.pieces) p.instant = false;
    });
  }

  /**
   * Takes a backend update. Animated updates are queued and played in order,
   * faster the further behind the display is. An update without animation
   * either leaves the board alone (same position: e.g. a clock or "thinking"
   * change) or cancels everything and jumps (a real jump in the game).
   * `budgetMs` caps how long this move's animation may take.
   */
  apply(final: PieceView[], anim: AnimStep[], hooks: AnimHooks = {}, budgetMs: number | null = null) {
    if (anim.length === 0) {
      if (samePosition(final, this.target)) {
        this.target = final;
        if (!this.animating) this.refreshFlags(final);
        return;
      }
      this.generation++;
      this.dropped = null;
      this.queue = [];
      this.animating = false;
      this.resetSpeed();
      this.snap(final);
      return;
    }
    this.target = final;
    this.queue.push({ final, anim, hooks, budgetMs });
    if (!this.animating) this.drain();
  }

  /** Updates frozen flags without touching positions. */
  private refreshFlags(final: PieceView[]) {
    const frozen = new Map(final.map((p) => [p.id, p.frozen]));
    for (const p of this.pieces) p.frozen = frozen.get(p.id) ?? p.frozen;
  }

  private async drain() {
    const gen = this.generation;
    this.animating = true;
    while (this.queue.length > 0 && gen === this.generation) {
      const behind = this.queue.length;
      const item = this.queue.shift()!;
      if (behind > MAX_BEHIND) {
        // Far behind: show each move instantly, briefly, until caught up.
        this.snap(item.final);
        item.hooks.onSlide?.();
        await sleep(INSTANT_GAP_MS);
        continue;
      }
      await this.play(item, gen);
      this.dropped = null;
      if (gen !== this.generation) return;
      this.settle(item.final);
    }
    if (gen !== this.generation) return;
    this.resetSpeed();
    this.animating = false;
    this.refreshFlags(this.target);
  }

  /** After an animation, match the display to its final state exactly. */
  private settle(final: PieceView[]) {
    const byId = new Map(this.pieces.map((p) => [p.id, p]));
    const consistent =
      final.length === this.pieces.length && final.every((f) => byId.get(f.id)?.square === f.square);
    if (consistent) this.refreshFlags(final);
    else this.snap(final);
  }

  /** Speed for the next step: the base speed, faster for each move waiting
   * behind the current one, and scaled by `budgetScale`. */
  private updateSpeed(budgetScale: number) {
    const behind = this.queue.length + 1;
    const backlog = 1 / (1 + 0.6 * (behind - 1));
    this.stepMs = Math.round(this.baseStepMs * backlog * budgetScale);
    this.fadeMs = Math.round(this.baseStepMs * FADE_RATIO * backlog * budgetScale);
  }

  /** Factor that fits the move's animation into its time budget (≤ 1). */
  private budgetScale({ anim, budgetMs }: QueuedUpdate): number {
    if (budgetMs == null) return 1;
    this.updateSpeed(1);
    const fades = anim.filter((a) => a.captured || a.restored).length;
    const planned = anim.length * this.stepMs + fades * this.fadeMs;
    return planned > budgetMs ? budgetMs / planned : 1;
  }

  private async play(item: QueuedUpdate, gen: number) {
    const { anim, hooks } = item;
    const scale = this.budgetScale(item);
    const dropped = this.dropped;
    const route = anim.filter((a) => a.id === dropped);
    for (const p of this.pieces) p.instant = false;
    if (route.length > 1) await this.rewind(dropped!, route[0].from);
    for (const a of anim) {
      if (gen !== this.generation) return;
      this.updateSpeed(scale);
      if (route.length > 1) this.stepMs = Math.min(this.stepMs, ROUTE_STEP_MS);
      if (a.restored) {
        this.pieces.push({ ...a.restored, frozen: false, fading: 'in', instant: true });
        await sleep(this.fadeMs);
        const r = this.find(a.restored.id);
        if (r) {
          r.fading = null;
          r.instant = false;
        }
      }
      const p = this.find(a.id);
      hooks.onSlide?.();
      if (p && p.square !== a.to) {
        p.square = a.to;
        await sleep(this.stepMs);
      }
      if (a.captured) {
        const c = this.find(a.captured.id);
        if (c) {
          c.fading = 'out';
          hooks.onCapture?.();
          await sleep(this.fadeMs);
          this.pieces = this.pieces.filter((x) => x.id !== c.id);
        }
      }
    }
  }

  /** Puts a piece back on `square` without a transition, ready to slide again. */
  private async rewind(id: number, square: Square) {
    const p = this.find(id);
    if (!p) return;
    p.instant = true;
    p.square = square;
    await frame();
    await frame();
    p.instant = false;
  }

  find(id: number): DisplayPiece | undefined {
    return this.pieces.find((p) => p.id === id);
  }

  /** Places a dropped piece on `to` without a transition (before the backend confirms). */
  dropAt(id: number, to: Square) {
    const p = this.find(id);
    if (!p) return;
    p.instant = true;
    p.square = to;
    this.dropped = id;
  }

  /** Slides a rejected drop back to where it came from. */
  async revert(id: number, from: Square) {
    const p = this.find(id);
    if (!p) return;
    this.dropped = null;
    await frame();
    p.instant = false;
    await frame();
    p.square = from;
  }
}
