// Engine tests (the howdah-tourney daemon): how the lobby words a test's
// state and results.
import type { PoolEngineView } from './bindings/PoolEngineView';
import type { PoolView } from './bindings/PoolView';
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

/** When a running SPRT test is likely to end, in games, to follow "Ends":
 * `in about 1800 games (920–3600)`, the median and the middle 80% of
 * `gamesLeft` (by default every ending's); null without a projection. */
export function endsText(t: TestView, gamesLeft = t.projection?.gamesLeft): string | null {
  const p = t.projection;
  if (p == null || gamesLeft == null || t.status === 'finished') return null;
  const [lo, mid, hi] = gamesLeft.map(roughly);
  if (hi === 0) return 'once the running games finish';
  return `in about ${mid} games (${lo}–${hi}${p.openEnded ? '+' : ''})`;
}

/** Below this chance, passing or failing (or not) gets no row of its own
 * in [`endsRows`]. */
const LIKELY = 0.15;

/** One of the panel's Ends rows: what it's for (null for the only one) and
 * the games left at the 10th, 50th and 90th percentiles. */
export interface EndsRow {
  label: string | null;
  gamesLeft: [number, number, number];
}

/** The panel's Ends rows for a running SPRT test: one for every ending,
 * or, when it may well decide and may well not (each at least 15%) before
 * its pair limit, one for each, since the overall median then falls
 * between a decision along the way and running on to near the end. Null
 * without a projection. */
export function endsRows(t: TestView): EndsRow[] | null {
  const p = t.projection;
  if (p == null || t.status === 'finished') return null;
  const decides = p.pass + p.fail;
  if (t.maxPairs != null && p.gamesIfDecided && p.gamesIfNot && decides >= LIKELY && 1 - decides >= LIKELY) {
    return [
      { label: 'if it decides', gamesLeft: p.gamesIfDecided },
      { label: "if it doesn't", gamesLeft: p.gamesIfNot },
    ];
  }
  return [{ label: null, gamesLeft: p.gamesLeft }];
}

/** Games looked back over for the pace. */
const PACE_GAMES = 30;

/** How fast a test is finishing games, in games an hour, from its newest
 * games' finish times (seconds, newest first): the games since the last
 * long gap (a pause, or the daemon not running), a gap over ten times
 * the median and over a minute. Null with fewer than four such games. */
export function gamesPerHour(finished: number[]): number | null {
  const times = finished.slice(0, PACE_GAMES);
  const gaps = times.slice(1).map((t, i) => times[i] - t);
  const median = [...gaps].sort((a, b) => a - b)[Math.floor(gaps.length / 2)] ?? 0;
  const pause = gaps.findIndex((g) => g > 60 && g > 10 * median);
  const n = pause < 0 ? times.length : pause + 1;
  const span = times[0] - times[n - 1];
  return n < 4 || span <= 0 ? null : ((n - 1) / span) * 3600;
}

/** A stretch of time in brief, as precise as a projection is: `40 min`,
 * `6 h`, `3 days`. */
function roughTime(hours: number): string {
  const minutes = Math.round((hours * 60) / 5) * 5;
  if (minutes < 60) return `${Math.max(1, minutes)} min`;
  if (hours < 48) return `${Math.round(hours)} h`;
  return `${Math.round(hours / 24)} days`;
}

/** When an active SPRT test is likely to end, in time, at `perHour`
 * games an hour: `about 6 h (3–12 h)`, the median and the middle 80% of
 * `gamesLeft` (by default every ending's); null without a projection or a
 * pace. */
export function endsTimeText(t: TestView, perHour: number | null, gamesLeft = t.projection?.gamesLeft): string | null {
  const p = t.projection;
  if (p == null || gamesLeft == null || perHour == null || t.status !== 'active') return null;
  if (gamesLeft[2] === 0) return null;
  const [lo, mid, hi] = gamesLeft.map((n) => roughTime(n / perHour));
  const unit = (s: string) => s.split(' ')[1];
  const from = unit(lo) === unit(hi) ? lo.split(' ')[0] : lo;
  return `about ${mid} (${from}–${hi}${p.openEnded ? '+' : ''})`;
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

/** Games, pairs and the score: `12-8 in 20 games (10 pairs)`; `brief`,
 * for a card's narrow line, `12-8 in 20`. */
export function scoreText(t: TestView, brief = false): string {
  if (brief) return `${t.wins[0]}–${t.wins[1]} in ${t.games}`;
  const pairs = t.pairs === 1 ? '1 pair' : `${t.pairs} pairs`;
  return `${t.wins[0]}–${t.wins[1]} in ${t.games} games (${pairs})`;
}

/** A pool engine's Elo with its 95% interval: `+12.3 ±8.0`; the
 * anchor's `0`; `–` before its first game. */
export function eloText(e: PoolEngineView): string {
  if (e.elo == null) return '–';
  if (e.error === 0) return '0';
  const elo = `${e.elo >= 0 ? '+' : ''}${e.elo.toFixed(1)}`;
  return e.error == null ? elo : `${elo} ±${e.error.toFixed(1)}`;
}

/** A pool's engines in brief: `5 engines`, or `4 engines playing, 1
 * retired`. */
export function poolEnginesText(p: PoolView): string {
  const retired = p.engines.filter((e) => e.retired).length;
  const playing = p.engines.length - retired;
  const n = (k: number) => (k === 1 ? '1 engine' : `${k} engines`);
  return retired === 0 ? n(playing) : `${n(playing)} playing, ${retired} retired`;
}

/** Who leads a pool: `v3 leads by 24.1` (over the next), or just `v3
 * leads`; null before any games. */
export function poolLeaderText(p: PoolView): string | null {
  if (p.anchor == null) return null;
  const [first, second] = p.ranking.map((i) => p.engines[i]).filter((e) => e.elo != null);
  if (!first) return null;
  if (!second) return `${first.name} leads`;
  return `${first.name} leads by ${((first.elo ?? 0) - (second.elo ?? 0)).toFixed(1)}`;
}

/** Engine `i` against engine `j` in a pool, from `i`'s side: its wins
 * and losses, the score the ratings predict for it, and how far out of
 * line (z) the result is; null if they haven't played. */
export function matchupFor(
  p: PoolView,
  i: number,
  j: number,
): { wins: number; losses: number; pairs: number; expected: number; z: number; outlier: boolean } | null {
  const m = p.matchups.find((m) => m.a === Math.min(i, j) && m.b === Math.max(i, j));
  if (!m) return null;
  const mine = m.a === i;
  return {
    wins: mine ? m.wins[0] : m.wins[1],
    losses: mine ? m.wins[1] : m.wins[0],
    pairs: m.pairs,
    expected: mine ? m.expected : 1 - m.expected,
    z: mine ? m.z : -m.z,
    outlier: m.outlier,
  };
}

/** A crosstable cell's shade for how far its result is from the
 * ratings' prediction: the accent (better) or warn (worse) color, mixed
 * in up to 55% by `|z|` up to the outlier threshold; null within half a
 * standard deviation. */
export function zShade(z: number, outlierZ: number): string | null {
  if (Math.abs(z) < 0.5) return null;
  const share = Math.round(Math.min(1, Math.abs(z) / outlierZ) * 55);
  return `color-mix(in srgb, var(${z > 0 ? '--accent' : '--warn'}) ${share}%, transparent)`;
}

/** The score, Elo, LLR and games left in brief, for the sidebar:
 * `118–99 +29.0 0.44 1800` (each left out when the test has none); an
 * SPSA run's iterations: `iteration 358/2000`; a pool's engines and
 * leader. */
export function briefText(t: TestView): string {
  if (t.spsa) return `iteration ${t.spsa.done}/${t.spsa.iterations}`;
  if (t.pool) {
    const leader = poolLeaderText(t.pool);
    return `${t.pool.engines.length} engines${leader ? `, ${leader.split(' by ')[0]}` : ''}`;
  }
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
  if (t.spsa) {
    const s = t.spsa;
    return [
      `SPSA: iteration ${s.done} of ${s.iterations}, ${s.pairsPerIteration} pairs each`,
      `${s.params.length} parameters`,
      `θ + cΔ against θ − cΔ: ${scoreText(t)}`,
    ].join('\n');
  }
  if (t.pool) {
    const outliers = t.pool.matchups.filter((m) => m.outlier).length;
    return [
      `Pool of ${poolEnginesText(t.pool)}, ${t.games} games`,
      poolLeaderText(t.pool),
      outliers > 0 ? `${outliers} ${outliers === 1 ? 'matchup' : 'matchups'} out of line with the ratings` : null,
    ]
      .filter(Boolean)
      .join('\n');
  }
  const ends = endsText(t) && `Ends ${endsText(t)}: ${oddsText(t)}`;
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

/** A test's game by number: its pair, and 1 or 2 within it. */
export type GameNumber = { pair: number; game: number };

/** The first of `games` started after `after` (games start in number
 * order), or null if none was. */
export function nextGameAfter(after: GameNumber, games: Iterable<GameNumber>): GameNumber | null {
  const later = (a: GameNumber, b: GameNumber) => a.pair > b.pair || (a.pair === b.pair && a.game > b.game);
  let next: GameNumber | null = null;
  for (const g of games) {
    if (later(g, after) && (!next || later(next, g))) next = { pair: g.pair, game: g.game };
  }
  return next;
}
