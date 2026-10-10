import { beforeEach, describe, expect, it } from 'vitest';
import type { PuzzleView } from './bindings/PuzzleView';
import { markSolved, nextPuzzle, puzzleStatus, solvedPuzzles } from './puzzles';

const groups = [
  { name: '1 Move Puzzles', puzzles: [{ id: 'p4', title: 'a' }, { id: 'p5', title: 'b' }] },
  { name: '2 Move Puzzles', puzzles: [{ id: 'p7', title: 'c' }] },
];

const view = (over: Partial<PuzzleView>): PuzzleView => ({
  id: 'p4',
  title: null,
  question: null,
  solver: 'gold',
  status: 'solving',
  hasSolution: true,
  movesLeft: 2,
  atFrontier: true,
  hint: null,
  hintAvailable: true,
  author: null,
  wrongMove: null,
  ...over,
});

/** The tests run in Node: a plain stand-in for the browser's storage. */
function stubStorage() {
  const items = new Map<string, string>();
  Object.defineProperty(globalThis, 'localStorage', {
    configurable: true,
    value: {
      getItem: (k: string) => items.get(k) ?? null,
      setItem: (k: string, v: string) => void items.set(k, String(v)),
      removeItem: (k: string) => void items.delete(k),
      clear: () => items.clear(),
    },
  });
}

describe('puzzles', () => {
  beforeEach(stubStorage);

  it('goes on to the next puzzle across headings', () => {
    expect(nextPuzzle(groups, 'p5')).toBe('p7');
    expect(nextPuzzle(groups, 'p7')).toBeNull();
    expect(nextPuzzle(groups, 'p99')).toBeNull();
  });

  it('remembers solved puzzles', () => {
    expect(solvedPuzzles().size).toBe(0);
    markSolved('p4');
    markSolved('p4');
    expect([...solvedPuzzles()]).toEqual(['p4']);
    localStorage.setItem('puzzles.solved', '{bad');
    expect(solvedPuzzles().size).toBe(0);
  });

  it('says how the solving goes', () => {
    expect(puzzleStatus(view({}))).toBe('Gold to play: 2 moves to go');
    expect(puzzleStatus(view({ movesLeft: 1, solver: 'silver' }))).toBe('Silver to play');
    expect(puzzleStatus(view({ atFrontier: false }))).toBe('Exploring away from the puzzle');
    expect(puzzleStatus(view({ status: 'wrong', wrongMove: '2g Ra5e' }))).toBe('Not the solution: 2g Ra5e');
    expect(puzzleStatus(view({ status: 'solved' }))).toBe('Solved!');
  });
});
