// Small checks on the gameroom's lists and a followed game's state, shared
// by the toolbar's alerts and the arimaa.com dialog.
import type { Color } from './bindings/Color';
import type { LiveGameView } from './bindings/LiveGameView';
import type { WatchView } from './bindings/WatchView';

const same = (a: string | null, b: string | null | undefined) =>
  a != null && b != null && a.toLowerCase() === b.toLowerCase();

/** The user's side in one of their games. */
export function mySide(g: LiveGameView, user: string | null | undefined): Color | null {
  if (same(g.gold, user)) return 'gold';
  if (same(g.silver, user)) return 'silver';
  return null;
}

/** Whether one of the user's games waits on their move. The list gives no
 * turn before the first move, which is gold's setup. */
export function myTurn(g: LiveGameView, user: string | null | undefined): boolean {
  const side = mySide(g, user);
  return side != null && (g.turn ?? 'gold') === side && g.gold != null && g.silver != null;
}

/** The opponent's chat lines that `next` adds to `prev` (the same game,
 * at the user's seat). */
export function newOpponentChat(prev: WatchView | null, next: WatchView): WatchView['chat'] {
  if (next.side == null || prev?.gid !== next.gid) return [];
  return next.chat.slice(prev.chat.length).filter((l) => l.side != null && l.side !== next.side);
}
