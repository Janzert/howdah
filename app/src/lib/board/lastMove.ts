// Turns the last move's steps into one trail per piece, for drawing.
import type { LastMoveView } from '../bindings/LastMoveView';
import type { Piece } from '../bindings/Piece';
import type { PieceAt } from '../bindings/PieceAt';
import type { Square } from '../bindings/Square';

export interface Trail {
  piece: Piece;
  /** Squares the piece passed through, start first. */
  squares: Square[];
  /** An enemy of the mover, pushed or pulled. */
  displaced: boolean;
}

/** Joins each piece's consecutive steps into a single trail, so a piece that
 * walked three squares gets one path, and collects the captured pieces. */
export function lastMoveTrails(last: LastMoveView): { trails: Trail[]; captured: PieceAt[] } {
  const trails: Trail[] = [];
  const captured: PieceAt[] = [];
  // The trail whose piece now stands on each square.
  const at = new Map<Square, Trail>();
  for (const s of last.steps) {
    let t = at.get(s.from);
    at.delete(s.from);
    if (t) {
      t.squares.push(s.to);
    } else {
      t = { piece: s.piece, squares: [s.from, s.to], displaced: s.piece.color !== last.color };
      trails.push(t);
    }
    at.set(s.to, t);
    if (s.captured) {
      captured.push(s.captured);
      at.delete(s.captured.square);
    }
  }
  return { trails, captured };
}
