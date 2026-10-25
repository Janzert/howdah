// Which clocks tick when time runs low.
import type { Color } from './bindings/Color';
import type { PlayersView } from './bindings/PlayersView';
/** Whether the low-time tick plays for `side`'s running clock: a human's
 * at this machine, or either side's in a game without one (watched,
 * engine against engine) if `spectating` ticks are on. */
export function ticksFor(players: PlayersView, side: Color, spectating: boolean): boolean {
  if (players[side].kind === 'human') return true;
  return spectating && players.gold.kind !== 'human' && players.silver.kind !== 'human';
}
