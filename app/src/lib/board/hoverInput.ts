// Pointer-only input without a button pressed: hover arrows and step mode
// (after 4steps). This is the pure part: which square the pointer leans
// toward, and which step step mode offers.
import type { Square } from '../bindings/Square';
import { SQ, squareAt, squareXY } from '../geometry';

/** A legal single step to `to`; `enemy` when it pushes or pulls an enemy piece. */
export interface StepArrow {
  to: Square;
  enemy: boolean;
}

/** The neighbour of the square under `p` (board units) across the nearest
 * edge, or null at the exact center or off the board. */
export function nearestNeighbour(p: { x: number; y: number }, flipped: boolean): Square | null {
  const sq = squareAt(p.x, p.y, flipped);
  if (sq == null) return null;
  const c = squareXY(sq, flipped);
  const dx = p.x - (c.x + SQ / 2);
  const dy = p.y - (c.y + SQ / 2);
  if (dx === 0 && dy === 0) return null;
  const [nx, ny] = Math.abs(dx) > Math.abs(dy) ? [Math.sign(dx) * SQ, 0] : [0, Math.sign(dy) * SQ];
  return squareAt(c.x + SQ / 2 + nx, c.y + SQ / 2 + ny, flipped);
}

export interface StepChoice {
  from: Square;
  /** The step's destination, or null when the piece can step but not toward the pointer. */
  to: Square | null;
  enemy: boolean;
}

/**
 * The step step mode offers with the pointer at `p`. A piece under the
 * pointer that can step offers the step toward the nearest edge (or none, if
 * that one isn't legal). Otherwise the neighbour across the nearest edge is
 * offered stepping in. `steps` returns a square's legal single steps, or
 * undefined while they're being fetched (then nothing is offered yet).
 */
export function stepChoice(
  p: { x: number; y: number },
  flipped: boolean,
  steps: (sq: Square) => StepArrow[] | undefined,
): StepChoice | null {
  const sq = squareAt(p.x, p.y, flipped);
  const near = nearestNeighbour(p, flipped);
  if (sq == null) return null;
  const own = steps(sq);
  if (own === undefined) return null;
  if (own.length > 0) {
    const a = near == null ? undefined : own.find((s) => s.to === near);
    return a ? { from: sq, to: near, enemy: a.enemy } : { from: sq, to: null, enemy: false };
  }
  if (near == null) return null;
  const other = steps(near);
  const a = other?.find((s) => s.to === sq);
  return a ? { from: near, to: sq, enemy: a.enemy } : null;
}
