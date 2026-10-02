// Clock arithmetic for the running side, counted on locally from a view.
import type { ClockView } from './bindings/ClockView';

/** Time the running side has left this turn (move time plus usable reserve),
 * `sinceViewMs` after the view carrying `clock` arrived. */
export function turnTimeLeft(clock: ClockView, sinceViewMs: number): number {
  return Math.max(0, clock.turnAllowanceMs - clock.turnElapsedMs - sinceViewMs);
}

/** Time left (ms) at which the low-time tick plays, largest first: at 30, 20
 * and 10 s, then each second, half second and quarter second as time runs
 * out (4steps' schedule). */
export const TICK_TIMES_MS: readonly number[] = [
  30_000, 20_000, 10_000, 9000, 8000, 7000, 6000, 5000, 4500, 4000, 3500, 3000, 2500, 2250, 2000, 1750,
  1500, 1250, 1000, 750, 500, 250,
];

/** The next tick to play once time left falls to it: the largest tick time
 * below both `left` and the last tick played. */
export function nextTick(left: number, lastTick = Infinity): number | null {
  return TICK_TIMES_MS.find((t) => t < left && t < lastTick) ?? null;
}
