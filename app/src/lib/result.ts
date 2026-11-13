// Describing a finished game, and deciding when to announce one.
import type { Color } from './bindings/Color';
import type { GameResult } from './bindings/GameResult';
import type { SessionView } from './bindings/SessionView';

function sideWord(c: Color): string {
  return c === 'gold' ? 'Gold' : 'Silver';
}

/** The result in words, e.g. "Gold wins" and "A gold rabbit reached the goal." */
export function describeResult(r: GameResult): { title: string; reason: string } {
  const winner = sideWord(r.winner);
  const loser = sideWord(r.winner === 'gold' ? 'silver' : 'gold');
  const reason = {
    goal: `A ${r.winner} rabbit reached the goal.`,
    elimination: `All ${loser.toLowerCase()} rabbits were captured.`,
    immobilization: `${loser} has no legal move.`,
    timeout: `${loser} ran out of time.`,
    resignation: `${loser} resigned.`,
    illegalMove: `${loser} played an illegal move.`,
    score: `The game time limit ran out; ${winner} wins on score.`,
    forfeit: `${loser} forfeited.`,
  }[r.reason];
  return { title: `${winner} wins`, reason };
}

/**
 * Whether `next` shows a game ending as it was played: `prev` was at the
 * live end of the same game, without a result, and `next` adds at most the
 * final move, or, in a match already under way, the moves that came with
 * the result (a server's reply can carry the last two when a bot answers
 * at once). Going to the end of a finished game or loading a record isn't
 * an ending to announce, nor is opening a server game that has just ended.
 */
export function justEnded(prev: SessionView | null, next: SessionView): boolean {
  if (!prev || prev.result || !next.result) return false;
  if (prev.ply !== prev.moves.length) return false;
  const added = next.moves.length - prev.moves.length;
  const inMatch = prev.livePly != null && next.livePly != null && prev.moves.length > 0;
  if (added < 0 || (added > 1 && !inMatch)) return false;
  return prev.moves.every((m, i) => m.notation === next.moves[i].notation);
}
