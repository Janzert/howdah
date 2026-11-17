// A player's finished games seen from their side: whether they won,
// how their games ended, and filtering by those. Works on the games
// already fetched, so it asks the server for nothing.
import type { PastGameView } from './bindings/PastGameView';
import type { WinReason } from './bindings/WinReason';

export type Outcome = 'win' | 'loss';

/** Whether `username` won or lost `g`; null if they didn't play it or it has no result. */
export function outcome(g: PastGameView, username: string): Outcome | null {
  if (!g.result) return null;
  const name = username.toLowerCase();
  const side = g.gold.toLowerCase() === name ? 'gold' : g.silver.toLowerCase() === name ? 'silver' : null;
  if (!side) return null;
  return g.result.winner === side ? 'win' : 'loss';
}

/** The endings in order: the position's first, then the rest. */
export const REASONS: WinReason[] = [
  'goal',
  'elimination',
  'immobilization',
  'timeout',
  'resignation',
  'illegalMove',
  'score',
  'forfeit',
];

export interface EndingLine {
  reason: WinReason;
  wins: number;
  losses: number;
}

/** How the player's games ended, won and lost, for the endings that
 * happened, in `REASONS`' order. */
export function endingStats(games: PastGameView[], username: string): EndingLine[] {
  const by = new Map<WinReason, EndingLine>();
  for (const g of games) {
    const o = outcome(g, username);
    if (!o || !g.result) continue;
    let line = by.get(g.result.reason);
    if (!line) by.set(g.result.reason, (line = { reason: g.result.reason, wins: 0, losses: 0 }));
    if (o === 'win') line.wins++;
    else line.losses++;
  }
  return REASONS.flatMap((r) => by.get(r) ?? []);
}

/** Which games to show: `null` for any. */
export interface GamesFilter {
  outcome: Outcome | null;
  reason: WinReason | null;
  rated: boolean | null;
}

export const NO_FILTER: GamesFilter = { outcome: null, reason: null, rated: null };

export const isFiltered = (f: GamesFilter) => f.outcome != null || f.reason != null || f.rated != null;

export function filterGames(games: PastGameView[], username: string, f: GamesFilter): PastGameView[] {
  if (!isFiltered(f)) return games;
  return games.filter(
    (g) =>
      (f.rated == null || g.rated === f.rated) &&
      (f.reason == null || g.result?.reason === f.reason) &&
      (f.outcome == null || outcome(g, username) === f.outcome),
  );
}
