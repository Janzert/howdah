import type { Square } from './bindings/Square';

/** Board units per square; the playing area is 800×800. */
export const SQ = 100;

export const fileOf = (sq: Square) => sq % 8;
export const rankOf = (sq: Square) => Math.floor(sq / 8);

/** Top-left corner of a square in board units. */
export function squareXY(sq: Square, flipped: boolean): { x: number; y: number } {
  const f = fileOf(sq);
  const r = rankOf(sq);
  return { x: (flipped ? 7 - f : f) * SQ, y: (flipped ? r : 7 - r) * SQ };
}

/** Square under a point in board units, or null if off the board. */
export function squareAt(x: number, y: number, flipped: boolean): Square | null {
  const col = Math.floor(x / SQ);
  const row = Math.floor(y / SQ);
  if (col < 0 || col > 7 || row < 0 || row > 7) return null;
  const f = flipped ? 7 - col : col;
  const r = flipped ? row : 7 - row;
  return r * 8 + f;
}

export function squareName(sq: Square): string {
  return 'abcdefgh'[fileOf(sq)] + (rankOf(sq) + 1);
}

export const TRAPS: Square[] = [18, 21, 42, 45];
