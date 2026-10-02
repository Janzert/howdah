import { describe, expect, it } from 'vitest';
import type { GameResult } from './bindings/GameResult';
import type { SessionView } from './bindings/SessionView';
import { describeResult, justEnded } from './result';

function view(notations: string[], ply = notations.length, result: GameResult | null = null): SessionView {
  return {
    moves: notations.map((notation, i) => ({ ply: i + 1, label: `${i}`, notation })),
    ply,
    result,
  } as SessionView;
}

const goal: GameResult = { winner: 'gold', reason: 'goal' };

describe('describeResult', () => {
  it('names the winner and the reason', () => {
    expect(describeResult(goal)).toEqual({ title: 'Gold wins', reason: 'A gold rabbit reached the goal.' });
    expect(describeResult({ winner: 'silver', reason: 'timeout' }).reason).toBe('Gold ran out of time.');
    expect(describeResult({ winner: 'gold', reason: 'elimination' }).reason).toBe(
      'All silver rabbits were captured.',
    );
  });
});

describe('justEnded', () => {
  it('announces a final move or a result without one (a timeout)', () => {
    expect(justEnded(view(['a', 'b']), view(['a', 'b', 'c'], 3, goal))).toBe(true);
    expect(justEnded(view(['a', 'b']), view(['a', 'b'], 2, goal))).toBe(true);
  });

  it('ignores reaching the end of a finished game, and loads', () => {
    expect(justEnded(view(['a', 'b', 'c'], 1), view(['a', 'b', 'c'], 3, goal))).toBe(false);
    expect(justEnded(view(['a', 'b']), view(['x', 'y', 'z'], 3, goal))).toBe(false);
    expect(justEnded(view(['a']), view(['a', 'b', 'c'], 3, goal))).toBe(false);
    expect(justEnded(null, view(['a'], 1, goal))).toBe(false);
  });

  it('announces once', () => {
    expect(justEnded(view(['a'], 1, goal), view(['a'], 1, goal))).toBe(false);
  });
});
