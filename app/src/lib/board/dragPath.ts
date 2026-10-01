// The squares a dragged piece has been carried across, used as a hint for
// which of the shortest routes to take when it's dropped.
import type { Square } from '../bindings/Square';
import { fileOf, rankOf } from '../geometry';

export interface Point {
  x: number;
  y: number;
}

/** Below this segment length (board units) a corner crossing isn't resolved further. */
const MIN_SEGMENT = 0.5;

const adjacent = (a: Square, b: Square) =>
  Math.abs(fileOf(a) - fileOf(b)) + Math.abs(rankOf(a) - rankOf(b)) === 1;

/**
 * Records the path as the pointer moves. Moving back onto a square already on
 * the path cuts the path back to it, so jitter and deliberate backtracking
 * both clean themselves up. Pointer moves are subdivided until consecutive
 * squares are neighbors, so a fast move or a cut corner still records the
 * squares the pointer actually crossed.
 */
export class DragPath {
  /** Squares in order, starting with the square the drag started on. */
  squares: Square[];
  private last: Point;

  constructor(
    from: Square,
    start: Point,
    private locate: (p: Point) => Square | null,
  ) {
    this.squares = [from];
    this.last = start;
  }

  /** The path after the starting square. */
  get hint(): Square[] {
    return this.squares.slice(1);
  }

  /** Follows the pointer to `p`. Returns whether the path changed. */
  moveTo(p: Point): boolean {
    const before = this.squares.join();
    this.segment(this.last, p);
    this.last = p;
    return this.squares.join() !== before;
  }

  private segment(a: Point, b: Point) {
    const sb = this.locate(b);
    const tip = this.squares[this.squares.length - 1];
    if (sb == null || sb === tip) return;
    const far = !adjacent(tip, sb) && !this.squares.includes(sb);
    if (far && Math.hypot(b.x - a.x, b.y - a.y) > MIN_SEGMENT) {
      const mid = { x: (a.x + b.x) / 2, y: (a.y + b.y) / 2 };
      this.segment(a, mid);
      this.segment(mid, b);
      return;
    }
    this.enter(sb);
  }

  private enter(sq: Square) {
    const i = this.squares.indexOf(sq);
    if (i >= 0) this.squares.length = i + 1;
    else this.squares.push(sq);
  }
}
