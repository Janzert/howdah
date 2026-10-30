// Clock arithmetic for the running side, counted on locally from a view.
import type { ClockView, Color } from './types';

/** Time the running side has left this turn (move time plus usable reserve),
 * `sinceViewMs` after the view carrying `clock` arrived. */
export function turnTimeLeft(clock: ClockView, sinceViewMs: number): number {
  return Math.max(0, clock.turnAllowanceMs - clock.turnElapsedMs - sinceViewMs);
}

/** A side's clock as shown: move time and reserve, counted down while
 * it runs (`sinceViewMs` after the view arrived), and how urgent it is by
 * the time left for the turn (otherwise what its next turn will have).
 * In a past clock, the side that made the shown move shows the time it
 * took in place of the move time (`used`), as arimaa.com's viewer does.
 * Null for an untimed side. */
export function sideTimes(
  clock: ClockView,
  side: Color,
  sinceViewMs: number,
): { move: number; reserve: number; running: boolean; used: boolean; level: ClockLevel } | null {
  const mine = clock[side];
  if (!mine) return null;
  const { moveTimeMs, reserveMs, turnAllowanceMs, lastUsedMs } = mine;
  if (clock.running !== side) {
    const used = lastUsedMs != null;
    const move = lastUsedMs ?? moveTimeMs;
    return { move, reserve: reserveMs, running: false, used, level: clockLevel(turnAllowanceMs) };
  }
  const elapsed = clock.turnElapsedMs + sinceViewMs;
  const left = Math.max(0, clock.turnAllowanceMs - elapsed);
  const move = Math.min(left, Math.max(0, moveTimeMs - elapsed));
  const reserve = Math.min(left, Math.max(0, reserveMs - Math.max(0, elapsed - moveTimeMs)));
  return { move, reserve, running: true, used: false, level: clockLevel(left) };
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
  30_000, 20_000, 10_000, 9000, 8000, 7000, 6000, 5000, 4500, 4000, 3500, 3000, 2500, 2250, 2000, 1750, 1500, 1250,
  1000, 750, 500, 250,
];

/** The next tick to play once time left falls to it: the largest tick time
 * below both `left` and the last tick played. */
export function nextTick(left: number, lastTick = Infinity): number | null {
  return TICK_TIMES_MS.find((t) => t < left && t < lastTick) ?? null;
}

/** A time in its two largest units, to keep it short: `m:ss` under an
 * hour, then `1h05` (hours and minutes) and `2d03` (days and hours), each
 * rounded down. With `fractional`, a time under 10 s is its seconds to the
 * millisecond instead, the same width: `0.123`. */
export function formatShort(ms: number, fractional = false): string {
  if (fractional && ms < 10_000) return (Math.max(0, ms) / 1000).toFixed(3);
  const total = Math.max(0, Math.floor(ms / 1000));
  const pad = (n: number) => String(n).padStart(2, '0');
  if (total >= 86400) return `${Math.floor(total / 86400)}d${pad(Math.floor((total % 86400) / 3600))}`;
  if (total >= 3600) return `${Math.floor(total / 3600)}h${pad(Math.floor((total % 3600) / 60))}`;
  return `${Math.floor(total / 60)}:${pad(total % 60)}`;
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

/** A clock's time in a time control with fractions of a second: under 10 s
 * in tenths while it runs and to the millisecond when it's stopped, otherwise
 * as `formatClock`. `spare` is what's left out of the longest form; the bar
 * keeps its width, so the time doesn't shift when the form changes. */
export function formatFractional(ms: number, running: boolean): { text: string; spare: string } {
  const digits = running ? 1 : 3;
  const unit = 10 ** (3 - digits);
  const shown = Math.ceil(Math.max(0, ms) / unit) * unit;
  if (shown >= 10_000) return { text: formatClock(ms), spare: '.000' };
  const fraction = String(shown % 1000)
    .padStart(3, '0')
    .slice(0, digits);
  return { text: `0:0${Math.floor(shown / 1000)}.${fraction}`, spare: '0'.repeat(3 - digits) };
}
