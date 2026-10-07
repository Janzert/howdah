// Clock arithmetic for the running side, counted on locally from a view.
import type { ClockView } from './bindings/ClockView';

/** Time the running side has left this turn (move time plus usable reserve),
 * `sinceViewMs` after the view carrying `clock` arrived. */
export function turnTimeLeft(clock: ClockView, sinceViewMs: number): number {
  return Math.max(0, clock.turnAllowanceMs - clock.turnElapsedMs - sinceViewMs);
}

/** How urgent a clock looks, by the time left for the turn (move time plus
 * reserve): `ok`, `warn` under 30 s, `low` under 10 s, as arimaa.com
 * colors its clocks green, yellow and red. */
export type ClockLevel = 'ok' | 'warn' | 'low';

export function clockLevel(leftMs: number): ClockLevel {
  return leftMs < 10_000 ? 'low' : leftMs < 30_000 ? 'warn' : 'ok';
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

/** A clock's time as text: `m:ss`, `h:mm:ss` from an hour, and from a
 * day `Nd h:mm` (postal games run for days; seconds don't matter then). */
export function formatClock(ms: number): string {
  const total = Math.max(0, Math.ceil(ms / 1000));
  const d = Math.floor(total / 86400);
  const h = Math.floor((total % 86400) / 3600);
  const m = Math.floor((total % 3600) / 60);
  const s = total % 60;
  const pad = (n: number) => String(n).padStart(2, '0');
  if (d > 0) {
    // Rounded up to the minute, as the shorter forms are to the second.
    const mins = Math.ceil(total / 60);
    return `${Math.floor(mins / 1440)}d ${Math.floor((mins % 1440) / 60)}:${pad(mins % 60)}`;
  }
  return h > 0 ? `${h}:${pad(m)}:${pad(s)}` : `${m}:${pad(s)}`;
}
