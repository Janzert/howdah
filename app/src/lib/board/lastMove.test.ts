import { describe, expect, it } from 'vitest';
import type { LastStepView } from '../bindings/LastStepView';
import type { Piece } from '../bindings/Piece';
import { lastMoveTrails } from './lastMove';

const E: Piece = { color: 'gold', kind: 'elephant' };
const R: Piece = { color: 'gold', kind: 'rabbit' };
const h: Piece = { color: 'silver', kind: 'horse' };
const step = (piece: Piece, from: number, to: number, captured: LastStepView['captured'] = null): LastStepView => ({
  piece,
  from,
  to,
  captured,
});

describe('lastMoveTrails', () => {
  it('joins one piece walking several squares', () => {
    const { trails } = lastMoveTrails({ color: 'gold', steps: [step(E, 12, 20), step(E, 20, 28), step(E, 28, 29)] });
    expect(trails).toEqual([{ piece: E, squares: [12, 20, 28, 29], displaced: false }]);
  });

  it('separates a push into the enemy trail and the pusher trail', () => {
    // E on g4 (30) pushes the horse on g3 (22) to f3 (21), where it's captured.
    const cap = { piece: h, square: 21 };
    const { trails, captured } = lastMoveTrails({
      color: 'gold',
      steps: [step(h, 22, 21, cap), step(E, 30, 22), step(E, 22, 30)],
    });
    expect(trails).toEqual([
      { piece: h, squares: [22, 21], displaced: true },
      { piece: E, squares: [30, 22, 30], displaced: false },
    ]);
    expect(captured).toEqual([cap]);
  });

  it('starts a new trail for a different piece arriving on a vacated square', () => {
    const { trails } = lastMoveTrails({ color: 'gold', steps: [step(E, 12, 20), step(R, 4, 12)] });
    expect(trails.map((t) => t.squares)).toEqual([
      [12, 20],
      [4, 12],
    ]);
  });
});
