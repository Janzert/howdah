// Puzzles from arimaa.com's puzzle pages: which ones the user has solved
// (kept in this browser's storage, per viewer), and what comes next in
// the list.
import type { PuzzleGroupView } from './bindings/PuzzleGroupView';
import type { PuzzleView } from './bindings/PuzzleView';

const SOLVED_KEY = 'puzzles.solved';

export function solvedPuzzles(): Set<string> {
  try {
    const ids = JSON.parse(localStorage.getItem(SOLVED_KEY) ?? '[]');
    return new Set(Array.isArray(ids) ? ids.filter((i) => typeof i === 'string') : []);
  } catch {
    return new Set();
  }
}

export function markSolved(id: string) {
  const ids = solvedPuzzles();
  ids.add(id);
  try {
    localStorage.setItem(SOLVED_KEY, JSON.stringify([...ids]));
  } catch {
    /* not kept */
  }
}

/** The puzzle after `id` in the list, across headings; null at the end. */
export function nextPuzzle(groups: PuzzleGroupView[], id: string): string | null {
  const ids = groups.flatMap((g) => g.puzzles.map((p) => p.id));
  const i = ids.indexOf(id);
  return i >= 0 && i + 1 < ids.length ? ids[i + 1] : null;
}

/** What the puzzle panel says about how the solving goes. */
export function puzzleStatus(p: PuzzleView): string {
  const side = p.solver === 'gold' ? 'Gold' : 'Silver';
  switch (p.status) {
    case 'solving':
      if (!p.atFrontier) return 'Exploring away from the puzzle';
      return p.movesLeft > 1 ? `${side} to play: ${p.movesLeft} moves to go` : `${side} to play`;
    case 'wrong':
      return `Not the solution${p.wrongMove ? `: ${p.wrongMove}` : ''}`;
    case 'solved':
      return 'Solved!';
    case 'shown':
      return 'The answer is in the move list: step through it with →';
    case 'open':
      return 'No answer came with this puzzle: explore freely';
  }
}
