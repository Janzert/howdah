import { describe, expect, it } from 'vitest';

import { SQ, squareAt, squareName } from '../geometry';
import { DragPath, type Point } from './dragPath';

const locate = (p: Point) => squareAt(p.x, p.y, false);

/** Center of a named square, unflipped. */
function at(name: string): Point {
  const f = name.charCodeAt(0) - 97;
  const r = Number(name[1]) - 1;
  return { x: f * SQ + SQ / 2, y: (7 - r) * SQ + SQ / 2 };
}

function drag(from: string, ...points: Point[]): string {
  const path = new DragPath(locate(at(from))!, at(from), locate);
  for (const p of points) path.moveTo(p);
  return path.squares.map(squareName).join(' ');
}

describe('DragPath', () => {
  it('records squares as the pointer enters them', () => {
    expect(drag('d4', at('d5'), at('d6'), at('e6'))).toBe('d4 d5 d6 e6');
  });

  it('cuts back to a square it revisits', () => {
    expect(drag('d4', at('d5'), at('d6'), at('d5'), at('e5'))).toBe('d4 d5 e5');
    expect(drag('d4', at('d5'), at('d4'))).toBe('d4');
  });

  it('fills in squares skipped by a fast move', () => {
    expect(drag('d4', at('d7'))).toBe('d4 d5 d6 d7');
  });

  it('resolves a cut corner to the square the pointer crossed', () => {
    // From the middle of d4, up and to the right: crosses d4's top edge
    // first (into d5) before reaching e5.
    const nearTop = { x: at('d4').x + 30, y: at('d4').y - 45 };
    const intoE5 = { x: at('e5').x - 40, y: at('e5').y + 20 };
    expect(drag('d4', nearTop, intoE5)).toBe('d4 d5 e5');
    // Mostly rightward first: crosses into e4.
    const nearRight = { x: at('d4').x + 45, y: at('d4').y - 30 };
    const intoE5b = { x: at('e5').x - 20, y: at('e5').y + 40 };
    expect(drag('d4', nearRight, intoE5b)).toBe('d4 e4 e5');
  });

  it('ignores moves off the board', () => {
    expect(drag('a1', { x: -50, y: 750 }, at('a2'))).toBe('a1 a2');
  });
});
