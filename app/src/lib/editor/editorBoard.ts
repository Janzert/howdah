// The position editor's board: which piece stands on each square, plus a
// stable id per piece so the board animates pieces that move rather than
// redrawing them. Only layout lives here; whether a position can start a
// game is the backend's call (`check_position`).
import type { Color } from '../bindings/Color';
import type { Piece } from '../bindings/Piece';
import type { PieceKind } from '../bindings/PieceKind';
import type { PieceView } from '../bindings/PieceView';
import type { PositionSpec } from '../bindings/PositionSpec';
import type { Square } from '../bindings/Square';
import { fileOf, rankOf } from '../geometry';

const KINDS: Record<string, PieceKind> = {
  r: 'rabbit',
  c: 'cat',
  d: 'dog',
  h: 'horse',
  m: 'camel',
  e: 'elephant',
};

/** Piece letter as in the short format: upper case gold, lower case silver. */
export function letterOf(piece: Piece): string {
  const l = Object.keys(KINDS).find((k) => KINDS[k] === piece.kind)!;
  return piece.color === 'gold' ? l.toUpperCase() : l;
}

export function pieceOf(letter: string): Piece | null {
  const kind = KINDS[letter.toLowerCase()];
  if (!kind) return null;
  return { color: letter === letter.toUpperCase() ? 'gold' : 'silver', kind };
}

/** The palette's pieces for a side, strongest first. */
export function palettePieces(color: Color): Piece[] {
  return (['elephant', 'camel', 'horse', 'dog', 'cat', 'rabbit'] as PieceKind[]).map((kind) => ({ color, kind }));
}

export class EditorBoard {
  /** Index = square (0 = a1). */
  squares: (Piece | null)[];
  ids: (number | null)[];
  private nextId = 0;

  constructor(squares: (Piece | null)[] = Array(64).fill(null)) {
    this.squares = [...squares];
    this.ids = this.squares.map((p) => (p ? this.nextId++ : null));
  }

  /** Reads the short format, `[` + 64 squares from a8 to h1 + `]`. */
  static fromShort(short: string): EditorBoard {
    const inner = short.trim().replace(/^\[/, '').replace(/\]$/, '');
    const squares: (Piece | null)[] = Array(64).fill(null);
    for (let i = 0; i < 64 && i < inner.length; i++) {
      squares[(7 - Math.floor(i / 8)) * 8 + (i % 8)] = pieceOf(inner[i]);
    }
    return new EditorBoard(squares);
  }

  toShort(): string {
    let s = '[';
    for (let rank = 7; rank >= 0; rank--) {
      for (let file = 0; file < 8; file++) {
        const p = this.squares[rank * 8 + file];
        s += p ? letterOf(p) : ' ';
      }
    }
    return s + ']';
  }

  /** Puts a piece on a square, replacing what was there. */
  put(sq: Square, piece: Piece) {
    this.squares[sq] = piece;
    this.ids[sq] = this.nextId++;
  }

  remove(sq: Square) {
    this.squares[sq] = null;
    this.ids[sq] = null;
  }

  /** Moves a piece (keeping its id), replacing what stood on `to`. */
  move(from: Square, to: Square) {
    if (from === to || !this.squares[from]) return;
    this.squares[to] = this.squares[from];
    this.ids[to] = this.ids[from];
    this.remove(from);
  }

  clear() {
    this.squares.fill(null);
    this.ids.fill(null);
  }

  /** The same position seen in a mirror: files a and h swap. Arimaa is
   * symmetric left to right. */
  mirror() {
    this.remap((sq) => rankOf(sq) * 8 + (7 - fileOf(sq)), false);
  }

  /** The colors swapped and the board turned over (ranks 1 and 8 swap), so
   * the same position plays with the other side, which Arimaa's symmetry
   * makes equivalent. The side to move changes too (the caller's). */
  swapColors() {
    this.remap((sq) => (7 - rankOf(sq)) * 8 + fileOf(sq), true);
  }

  private remap(to: (sq: Square) => Square, swap: boolean) {
    const squares: (Piece | null)[] = Array(64).fill(null);
    const ids: (number | null)[] = Array(64).fill(null);
    this.squares.forEach((p, sq) => {
      if (!p) return;
      squares[to(sq)] = swap ? { kind: p.kind, color: p.color === 'gold' ? 'silver' : 'gold' } : p;
      ids[to(sq)] = this.ids[sq];
    });
    this.squares = squares;
    this.ids = ids;
  }

  /** The pieces for the board component. */
  pieceViews(): PieceView[] {
    const out: PieceView[] = [];
    this.squares.forEach((piece, square) => {
      if (piece) out.push({ id: this.ids[square]!, piece, square, frozen: false });
    });
    return out;
  }

  spec(sideToMove: Color, moveNumber: number): PositionSpec {
    return { short: this.toShort(), sideToMove, moveNumber };
  }
}
