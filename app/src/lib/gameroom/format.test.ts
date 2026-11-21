import { describe, expect, it } from 'vitest';
import type { LiveGameView } from '../bindings/LiveGameView';
import { liveMoveLabel } from './format';

const game = (moveNumber: number | null, turn: LiveGameView['turn']): LiveGameView => ({
  gid: '1',
  gold: 'a',
  silver: 'b',
  goldRating: null,
  silverRating: null,
  goldId: null,
  silverId: null,
  timeControl: '2m/2m',
  rated: true,
  postal: false,
  turn,
  moveNumber,
});

describe('liveMoveLabel', () => {
  it("reads the list's move number and side", () => {
    expect(liveMoveLabel(game(12, 'silver'))).toBe('12s');
    expect(liveMoveLabel(game(1, 'gold'))).toBe('1g');
    expect(liveMoveLabel(game(null, 'gold'))).toBeNull();
    expect(liveMoveLabel(game(5, null))).toBeNull();
  });

  it('counts from the turns played when the game is followed', () => {
    expect(liveMoveLabel(game(28, 'silver'), 0)).toBe('1g');
    expect(liveMoveLabel(game(28, 'silver'), 1)).toBe('1s');
    expect(liveMoveLabel(game(28, 'silver'), 2)).toBe('2g');
    expect(liveMoveLabel(game(28, 'silver'), 57)).toBe('29s');
  });
});
