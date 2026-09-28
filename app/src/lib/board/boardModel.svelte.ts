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

const sleep = (ms: number) => new Promise((r) => setTimeout(r, ms));
const frame = () => new Promise((r) => requestAnimationFrame(() => r(undefined)));

export class BoardModel {
  pieces = $state<DisplayPiece[]>([]);
  animating = $state(false);
  private generation = 0;
  /** Final state of the latest update, to jump to if an animation is interrupted. */
  private target: PieceView[] = [];

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

  /** Plays `anim` from the current display, then settles on `final`. */
  async apply(final: PieceView[], anim: AnimStep[], hooks: AnimHooks = {}) {
    const gen = ++this.generation;
    if (anim.length === 0) {
      this.animating = false;
      this.snap(final);
      return;
    }
    if (this.animating) {
      // Interrupted: finish the previous update instantly, then animate this one.
      this.snap(this.target);
      await frame();
      await frame();
      if (gen !== this.generation) return;
    }
    this.target = final;
    this.animating = true;
    for (const p of this.pieces) p.instant = false;

    for (const a of anim) {
      if (gen !== this.generation) return;
      if (a.restored) {
        this.pieces.push({ ...a.restored, frozen: false, fading: 'in', instant: true });
        await sleep(FADE_MS);
        const r = this.find(a.restored.id);
        if (r) {
          r.fading = null;
          r.instant = false;
        }
      }
      const p = this.find(a.id);
      if (p && p.square !== a.to) {
        p.square = a.to;
        hooks.onSlide?.();
        await sleep(STEP_MS);
      } else {
        hooks.onSlide?.();
      }
      if (a.captured) {
        const c = this.find(a.captured.id);
        if (c) {
          c.fading = 'out';
          hooks.onCapture?.();
          await sleep(FADE_MS);
          this.pieces = this.pieces.filter((x) => x.id !== c.id);
        }
      }
    }
    if (gen !== this.generation) return;
    this.animating = false;
    this.snap(final);
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
