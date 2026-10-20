// Engine tests (the howdah-tourney daemon): how the lobby words a test's
// state and results.
import type { TestView } from './bindings/TestView';

/** A test's state in a word: active, paused, or how it ended. */
export function testState(t: TestView): string {
  if (t.status === 'finished') return t.outcome ?? 'finished';
  return t.status;
}

/** Where the LLR stands between the SPRT's bounds, from 0 (the lower) to
 * 1 (the upper); null without an SPRT. */
export function llrPosition(t: TestView): number | null {
  if (t.llr == null || t.llrBounds == null) return null;
  const [lo, hi] = t.llrBounds;
  return Math.min(1, Math.max(0, (t.llr - lo) / (hi - lo)));
}

/** How far a test is through its pair limit, from 0 to 1, by games
 * played; null without a limit. */
export function testProgress(t: TestView): number | null {
  if (t.maxPairs == null || t.maxPairs <= 0) return null;
  return Math.min(1, t.games / (2 * t.maxPairs));
}

/** The SPRT in brief: `LLR 1.23 (-2.94, 2.94) for [0, 10]`. */
export function sprtText(t: TestView): string | null {
  if (t.llr == null || t.llrBounds == null || t.sprt == null) return null;
  const [lo, hi] = t.llrBounds;
  return `LLR ${t.llr.toFixed(2)} (${lo.toFixed(2)}, ${hi.toFixed(2)}) for Elo [${t.sprt.elo0}, ${t.sprt.elo1}]`;
}

/** The chance below which new tests with a pair limit stop undecided, as
 * a percentage. */
export const UNDECIDED_PERCENT = 5;

/** A cutoff from a percentage field (empty or 0: off). */
export function percentCutoff(value: number | string | null | undefined): number | null {
  if (value == null || value === '') return null;
  const n = Number(value);
  return Number.isFinite(n) && n > 0 ? n / 100 : null;
}

/** A test's cutoff as a percentage for its field (null: off). */
export function cutoffPercent(t: TestView): number | null {
  const c = t.sprt?.undecidedBelow;
  return c == null ? null : Math.round(c * 1e6) / 1e4;
}

/** A chance as a percentage: `62%`, `4.1%`, `<0.1%`. */
function pct(c: number): string {
  const p = c * 100;
  return p > 0 && p < 0.1 ? '<0.1%' : p > 99.9 && p < 100 ? '>99.9%' : `${+p.toFixed(p < 10 ? 1 : 0)}%`;
}

/** The chance the SPRT still decides before the pair limit, while it's
 * running: `62% chance to decide within 3000 pairs`, and where the test
 * stops undecided if it does: `(stops under 5%)`. */
export function decideText(t: TestView): string | null {
  if (t.decideChance == null || t.maxPairs == null || t.status === 'finished') return null;
  const cutoff = t.sprt?.undecidedBelow;
  return `${pct(t.decideChance)} chance to decide within ${t.maxPairs} pairs${cutoff != null ? ` (stops under ${pct(cutoff)})` : ''}`;
}

/** A count of games to two significant figures (`1843` → `1800`), as
 * precise as a projection is. */
function roughly(n: number): number {
  if (n < 100) return n;
  const unit = 10 ** (Math.floor(Math.log10(n)) - 1);
  return Math.round(n / unit) * unit;
}

/** When a running SPRT test is likely to end, in games: `about 1800 games
 * (920–3600)`, the median and the middle 80%; null without a projection. */
export function endsText(t: TestView): string | null {
  const p = t.projection;
  if (p == null || t.status === 'finished') return null;
  const [lo, mid, hi] = p.gamesLeft.map(roughly);
  if (hi === 0) return 'after the games running';
  return `about ${mid} games (${lo}–${hi}${p.openEnded ? '+' : ''})`;
}

/** How a running SPRT test is likely to end: `pass 22%, fail 36%,
 * undecided 42%`, with every pair played (or, without a pair limit, still
 * going) when that can happen; null without a projection. */
export function oddsText(t: TestView): string | null {
  const p = t.projection;
  if (p == null || t.status === 'finished') return null;
  const parts = [`pass ${pct(p.pass)}`, `fail ${pct(p.fail)}`];
  if (p.undecided > 0) parts.push(`undecided ${pct(p.undecided)}`);
  if (p.done > 0) parts.push(`${t.maxPairs == null ? 'still going' : 'all pairs played'} ${pct(p.done)}`);
  return parts.join(', ');
}

/** Games, pairs and the score: `12-8 in 20 games (10 pairs)`. */
export function scoreText(t: TestView): string {
  const pairs = t.pairs === 1 ? '1 pair' : `${t.pairs} pairs`;
  return `${t.wins[0]}–${t.wins[1]} in ${t.games} games (${pairs})`;
}

/** The score, Elo, LLR and games left in brief, for the sidebar:
 * `118–99 +29.0 0.44 1800` (each left out when the test has none). */
export function briefText(t: TestView): string {
  const parts = [`${t.wins[0]}–${t.wins[1]}`];
  if (t.elo) parts.push(t.elo.split(' [')[0]);
  if (t.llr != null) parts.push(t.llr.toFixed(2));
  const left = t.projection?.gamesLeft[1];
  if (left && t.status !== 'finished') parts.push(`${roughly(left)}`);
  return parts.join(' ');
}

/** A test's details, one to a line, for a tooltip: the score, Elo, SPRT,
 * when and how it's likely to end, and the chance to decide in time. */
export function detailsText(t: TestView): string {
  const ends = endsText(t) && `Ends in ${endsText(t)}: ${oddsText(t)}`;
  return [scoreText(t), t.elo && `Elo ${t.elo}`, sprtText(t), ends, decideText(t)].filter(Boolean).join('\n');
}

/** Fast time controls for engine tests, with a turn limit. */
export const TEST_TIME_CONTROLS: [string, string][] = [
  ['1s/10s/100/0/100t', '1 s a move'],
  ['2s/30s/100/0/100t', '2 s a move'],
  ['5s/1m/100/0/100t', '5 s a move'],
  ['15s/2m/100/0/100t', '15 s a move'],
];

/** A pair limit from a number field (empty: none). */
export function pairCount(value: number | string | null | undefined): number | null {
  if (value == null || value === '') return null;
  const n = Number(value);
  return Number.isFinite(n) ? n : null;
}

/** How a test's game ended, in words: the log's reason names, with
 * `resourceLimit` (a forfeit for going over a memory limit) spelled out. */
export function endingText(reason: string): string {
  const words: Record<string, string> = {
    illegalMove: 'illegal move',
    resourceLimit: 'a memory-limit forfeit',
  };
  return words[reason] ?? reason;
}
