// Next game (docs/WINDOWS.md, after lichess's auto-switch): from a game
// window, the next game waiting on the user's move. Another window first,
// in the order they were opened, after this one; otherwise a postal game
// from the lobby's lists that has no window yet.
import type { Color } from './bindings/Color';
import type { GameroomGames } from './bindings/GameroomGames';
import type { SessionId } from './bindings/SessionId';
import { mySide, myTurn } from './gameroom';

export type NextGame = { kind: 'window'; session: SessionId } | { kind: 'postal'; gid: string; side: Color } | null;

/** What Next game goes to from session `current`: `waiting` are the
 * sessions waiting on the user, in the order they were opened; `games`
 * the lobby's last lists; `open` the arimaa.com games open in a window. */
export function pickNext(
  current: SessionId,
  waiting: SessionId[],
  games: GameroomGames | null,
  open: Set<string>,
): NextGame {
  const others = waiting.filter((id) => id !== current);
  if (others.length > 0) {
    const after = others.find((id) => id > current) ?? others[0];
    return { kind: 'window', session: after };
  }
  for (const g of games?.mine ?? []) {
    const side = mySide(g, games!.user);
    if (g.postal && side && myTurn(g, games!.user) && !open.has(g.gid)) return { kind: 'postal', gid: g.gid, side };
  }
  return null;
}

/** How many games Next game could go to. */
export function countNext(
  current: SessionId,
  waiting: SessionId[],
  games: GameroomGames | null,
  open: Set<string>,
): number {
  const windows = waiting.filter((id) => id !== current).length;
  const postal = (games?.mine ?? []).filter((g) => g.postal && myTurn(g, games!.user) && !open.has(g.gid)).length;
  return windows + postal;
}
