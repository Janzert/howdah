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

export const STEP_MS = 220;
export const FADE_MS = 260;
/** More plies than this waiting to be shown: skip straight to the latest. */
export const MAX_BEHIND = 6;

const sleep = (ms: number) => new Promise((r) => setTimeout(r, ms));
const frame = () => new Promise((r) => requestAnimationFrame(() => r(undefined)));

interface QueuedUpdate {
  final: PieceView[];
  anim: AnimStep[];
  hooks: AnimHooks;
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
  /** Current slide and fade durations; shortened when moves queue up. */
  stepMs = $state(STEP_MS);
  fadeMs = $state(FADE_MS);
  private generation = 0;
  /** Final state of the latest update; what the board settles on. */
  private target: PieceView[] = [];
  private queue: QueuedUpdate[] = [];

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
   */
  apply(final: PieceView[], anim: AnimStep[], hooks: AnimHooks = {}) {
    if (anim.length === 0) {
      if (samePosition(final, this.target)) {
        this.target = final;
        if (!this.animating) this.refreshFlags(final);
        return;
      }
      this.generation++;
      this.queue = [];
      this.animating = false;
      this.stepMs = STEP_MS;
      this.fadeMs = FADE_MS;
      this.snap(final);
      return;
    }
    this.target = final;
    this.queue.push({ final, anim, hooks });
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
      if (behind > MAX_BEHIND) {
        // Too far behind: show the latest position and carry on from there.
        const latest = this.queue[this.queue.length - 1];
        this.queue = [];
        this.snap(latest.final);
        break;
      }
      const item = this.queue.shift()!;
      await this.play(item, gen);
      if (gen !== this.generation) return;
      this.settle(item.final);
    }
    if (gen !== this.generation) return;
    this.stepMs = STEP_MS;
    this.fadeMs = FADE_MS;
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

  /** Normal speed when only the current move is showing; each move waiting
   * behind it makes the animation faster. */
  private updateSpeed() {
    const behind = this.queue.length + 1;
    const speed = 1 / (1 + 0.6 * (behind - 1));
    this.stepMs = Math.round(STEP_MS * speed);
    this.fadeMs = Math.round(FADE_MS * speed);
  }

  private async play({ anim, hooks }: QueuedUpdate, gen: number) {
    for (const p of this.pieces) p.instant = false;
    for (const a of anim) {
      if (gen !== this.generation) return;
      this.updateSpeed();
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

  find(id: number): DisplayPiece | undefined {
    return this.pieces.find((p) => p.id === id);
  }

  /** Places a dropped piece on `to` without a transition (before the backend confirms). */
  dropAt(id: number, to: Square) {
    const p = this.find(id);
    if (!p) return;
    p.instant = true;
    p.square = to;
  }

  /** Slides a rejected drop back to where it came from. */
  async revert(id: number, from: Square) {
    const p = this.find(id);
    if (!p) return;
    await frame();
    p.instant = false;
    await frame();
    p.square = from;
  }
}
