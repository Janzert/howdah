// What the lobby shows about each game window, from its session's view.
import type { Color } from './bindings/Color';
import type { SessionView } from './bindings/SessionView';

/** The side to move at ply `ply` (0 is gold's setup). */
const sideAt = (ply: number): Color => (ply % 2 === 0 ? 'gold' : 'silver');

/** The match's result at its live node; undefined outside a match. */
function liveResult(v: SessionView) {
  if (v.live == null) return undefined;
  const live = v.live;
  return live === v.cursor ? v.result : (v.tree.find((n) => n.id === live)?.result ?? null);
}

/** Whether the game waits on the user's move: a match against an engine
 * or a remote player (on arimaa.com), with the human side to move. */
export function waitsOnUser(v: SessionView): boolean {
  if (!v.players || liveResult(v) != null) return false;
  const side = v.clock?.running ?? (v.livePly != null ? sideAt(v.livePly) : null);
  if (!side) return false;
  const other = side === 'gold' ? 'silver' : 'gold';
  return v.players[side].kind === 'human' && v.players[other].kind !== 'human';
}

/** The players, as "Gold - Silver", or null when the game names none. */
export function gameTitle(v: SessionView): string | null {
  if (v.players) return `${v.players.gold.name} - ${v.players.silver.name}`;
  const [gold, silver] = v.tagNames;
  if (gold || silver) return `${gold ?? '?'} - ${silver ?? '?'}`;
  return null;
}
