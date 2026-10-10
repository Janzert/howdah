import { describe, expect, it } from 'vitest';
import { EditorBoard, letterOf, palettePieces, pieceOf } from './editorBoard';

const sq = (name: string) => (Number(name[1]) - 1) * 8 + 'abcdefgh'.indexOf(name[0]);
const at = (b: EditorBoard, name: string) => {
  const p = b.squares[sq(name)];
  return p ? letterOf(p) : null;
};

/** The short format with pieces on the given squares (`Ra2 eh8`). */
function short(spec: string): string {
  const b = new EditorBoard();
  for (const w of spec.split(' ').filter(Boolean)) b.put(sq(w.slice(1)), pieceOf(w[0])!);
  return b.toShort();
}

describe('EditorBoard', () => {
  it('reads and writes the short format', () => {
    const s = short('Ra2 Ed4 eh8 rh7');
    expect(s).toHaveLength(66);
    expect(s.startsWith('[       e       r')).toBe(true);
    const b = EditorBoard.fromShort(s);
    expect(b.toShort()).toBe(s);
    expect([at(b, 'a2'), at(b, 'd4'), at(b, 'h8'), at(b, 'a1')]).toEqual(['R', 'E', 'e', null]);
  });

  it('moves pieces with their ids and replaces what stood there', () => {
    const b = EditorBoard.fromShort(short('Ra2 Ed4'));
    const id = b.ids[sq('d4')];
    b.move(sq('d4'), sq('a2'));
    expect(at(b, 'a2')).toBe('E');
    expect(b.ids[sq('a2')]).toBe(id);
    expect(b.pieceViews()).toHaveLength(1);
    b.put(sq('c3'), pieceOf('c')!);
    expect(new Set(b.pieceViews().map((p) => p.id)).size).toBe(2);
    b.remove(sq('c3'));
    b.clear();
    expect(b.pieceViews()).toEqual([]);
  });

  it('mirrors and swaps colors', () => {
    const b = EditorBoard.fromShort(short('Ra2 Dc3 eh8'));
    b.mirror();
    expect([at(b, 'h2'), at(b, 'f3'), at(b, 'a8')]).toEqual(['R', 'D', 'e']);
    b.swapColors();
    expect([at(b, 'h7'), at(b, 'f6'), at(b, 'a1')]).toEqual(['r', 'd', 'E']);
  });

  it('lists a side’s pieces strongest first', () => {
    expect(palettePieces('silver').map(letterOf).join('')).toBe('emhdcr');
  });
});
