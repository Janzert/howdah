// The SPRT funnel: the observed Elo at which a test passes or fails after
// each number of pairs, the band where it stops undecided, and the test's
// own path through them. The LLR follows howdah-tourney's `stats.rs`
// (fishtest's normal approximation on pairs, the variance padded with one
// pair of each outcome), as does the parent repo's SPRT cutoff chart this
// is drawn after.
import type { TestGameView } from './bindings/TestGameView';
import type { TestView } from './bindings/TestView';

export interface Funnel {
  elo0: number;
  elo1: number;
  /** The scores elo0 and elo1 predict, and halfway between them. */
  s0: number;
  s1: number;
  mid: number;
  /** The LLR's fail and pass bounds. */
  lower: number;
  upper: number;
}

/** A point of a curve: pairs played and the Elo there. */
export type CurvePoint = [pairs: number, elo: number];

/** The test's results after each pair it completed. */
export interface PathPoint {
  pairs: number;
  /** The observed Elo, infinite after all wins or all losses. */
  elo: number;
  /** Its 95% interval; null while every pair has scored the same. */
  interval: [number, number] | null;
}

export const scoreFromElo = (e: number) => 1 / (1 + 10 ** (-e / 400));
export const eloFromScore = (s: number) => (s <= 0 ? -Infinity : s >= 1 ? Infinity : -400 * Math.log10(1 / s - 1));

/** A test's SPRT, or null without one. */
export function funnelOf(t: TestView): Funnel | null {
  if (!t.sprt || !t.llrBounds) return null;
  const { elo0, elo1 } = t.sprt;
  const s0 = scoreFromElo(elo0);
  const s1 = scoreFromElo(elo1);
  return { elo0, elo1, s0, s1, mid: (s0 + s1) / 2, lower: t.llrBounds[0], upper: t.llrBounds[1] };
}

/** Variance of pair scores 0, ½ and 1 with these counts. */
function pairVar(c0: number, c1: number, c2: number): number {
  const t = c0 + c1 + c2;
  const m = (0.5 * c1 + c2) / t;
  return (c0 * m * m + c1 * (0.5 - m) ** 2 + c2 * (1 - m) ** 2) / t;
}

/** The padded pair-score variance after n pairs whose games are won with
 * chance m, as `stats.rs` pads the real counts. */
function modelVar(n: number, m: number): number {
  const q = 1 - m;
  return pairVar(n * q * q + 1, 2 * n * m * q + 1, n * m * m + 1);
}

/** The LLR after n pairs whose games are won with chance m. */
export function llrAt(f: Funnel, n: number, m: number): number {
  return (n * (f.s1 - f.s0) * (2 * m - f.s0 - f.s1)) / (2 * modelVar(n, m));
}

/** The score that puts the LLR on `bound` after n pairs, on the bound's
 * side of the midpoint; null if even a perfect score doesn't reach it. */
function cutoff(f: Funnel, n: number, bound: number): number | null {
  const pass = bound > 0;
  const eps = 1e-12;
  let lo = pass ? f.mid : eps;
  let hi = pass ? 1 - eps : f.mid;
  const edge = pass ? llrAt(f, n, hi) : llrAt(f, n, lo);
  if (pass ? edge < bound : edge > bound) return null;
  for (let i = 0; i < 60; i++) {
    const m = (lo + hi) / 2;
    const v = llrAt(f, n, m);
    if (pass ? v >= bound : v > bound) hi = m;
    else lo = m;
  }
  return pass ? hi : lo;
}

/** The observed Elo at or below which the test fails after n pairs, and
 * at or above which it passes; null where a bound can't be reached yet. */
export function cutoffsAt(f: Funnel, n: number): { fail: number | null; pass: number | null } {
  const lo = cutoff(f, n, f.lower);
  const hi = cutoff(f, n, f.upper);
  return { fail: lo === null ? null : eloFromScore(lo), pass: hi === null ? null : eloFromScore(hi) };
}

/** The fail and pass curves out to `maxPairs`: every pair count for the
 * first 60, where they bend most, then about `points` evenly. A curve
 * starts at the first pair count a bound can be reached at. */
export function funnelCurves(f: Funnel, maxPairs: number, points = 400): { fail: CurvePoint[]; pass: CurvePoint[] } {
  const ns: number[] = [];
  for (let n = 1; n <= Math.min(maxPairs, 60); n++) ns.push(n);
  const step = Math.max(1, Math.floor(maxPairs / points));
  for (let n = 61; n <= maxPairs; n += step) ns.push(n);
  if (maxPairs > 60 && ns[ns.length - 1] !== maxPairs) ns.push(maxPairs);
  const fail: CurvePoint[] = [];
  const pass: CurvePoint[] = [];
  for (const n of ns) {
    const c = cutoffsAt(f, n);
    if (c.fail !== null) fail.push([n, c.fail]);
    if (c.pass !== null) pass.push([n, c.pass]);
  }
  return { fail, pass };
}

/** ln Φ(x), accurate far into the lower tail (as `stats.rs`). */
function lnPhi(x: number): number {
  const z = Math.abs(x) / Math.SQRT2;
  const t = 1 / (1 + 0.5 * z);
  const poly =
    -1.26551223 +
    t *
      (1.00002368 +
        t *
          (0.37409196 +
            t *
              (0.09678418 +
                t *
                  (-0.18628806 +
                    t * (0.27886807 + t * (-1.13520398 + t * (1.48851587 + t * (-0.82215223 + t * 0.17087277))))))));
  const lnErfc = Math.log(t) - z * z + poly;
  return x < 0 ? Math.log(0.5) + lnErfc : Math.log1p(-0.5 * Math.exp(lnErfc));
}

/** The chance a Brownian motion with drift mu and variance s2 a pair rises
 * by h within t pairs. */
function cross(h: number, mu: number, s2: number, t: number): number {
  const sd = Math.sqrt(s2 * t);
  return Math.exp(lnPhi((mu * t - h) / sd)) + Math.exp((2 * mu * h) / s2 + lnPhi((-h - mu * t) / sd));
}

const POST_Z = Array.from({ length: 25 }, (_, i) => -4 + i / 3);
const POST_W = POST_Z.map((z) => Math.exp((-z * z) / 2));
const POST_SUM = POST_W.reduce((a, b) => a + b);

/** The chance a test at observed score m after n pairs still reaches a
 * bound by `maxPairs`, as `stats.rs`'s `decide_chance` works it out:
 * averaged over a normal posterior for the true score. */
export function reachChance(f: Funnel, n: number, m: number, maxPairs: number): number {
  const left = maxPairs - n;
  const l = llrAt(f, n, m);
  if (l >= f.upper || l <= f.lower) return 1;
  if (left <= 0) return 0;
  const d = f.s1 - f.s0;
  const hit = (q: number) => {
    const v = (q * (1 - q)) / 2;
    const mu = (d * (2 * q - f.s0 - f.s1)) / (2 * v);
    const s2 = (d * d) / v;
    return Math.min(1, cross(f.upper - l, mu, s2, left) + cross(l - f.lower, -mu, s2, left));
  };
  const sd = Math.sqrt(modelVar(n, m) / n);
  let sum = 0;
  for (let i = 0; i < POST_Z.length; i++)
    sum += POST_W[i] * hit(Math.min(1 - 1e-6, Math.max(1e-6, m + sd * POST_Z[i])));
  return sum / POST_SUM;
}

/** The observed Elo range [lo, hi] at which the test stops undecided after
 * n pairs, or null if none. */
function undecidedAt(f: Funnel, n: number, maxPairs: number, below: number): [number, number] | null {
  const lo = cutoff(f, n, f.lower) ?? 1e-9;
  const hi = cutoff(f, n, f.upper) ?? 1 - 1e-9;
  const p = (m: number) => reachChance(f, n, m, maxPairs);
  // Golden-section search for the score least likely to decide.
  const g = (Math.sqrt(5) - 1) / 2;
  let a = lo;
  let b = hi;
  let c = b - g * (b - a);
  let d = a + g * (b - a);
  let fc = p(c);
  let fd = p(d);
  for (let i = 0; i < 40; i++) {
    if (fc < fd) {
      b = d;
      d = c;
      fd = fc;
      c = b - g * (b - a);
      fc = p(c);
    } else {
      a = c;
      c = d;
      fc = fd;
      d = a + g * (b - a);
      fd = p(d);
    }
  }
  const best = (a + b) / 2;
  if (p(best) >= below) return null;
  const edge = (out: number, inside: number) => {
    for (let i = 0; i < 40; i++) {
      const m = (out + inside) / 2;
      if (p(m) < below) inside = m;
      else out = m;
    }
    return inside;
  };
  return [eloFromScore(edge(lo, best)), eloFromScore(edge(hi, best))];
}

/** The undecided band at about `points` pair counts, from the pair limit
 * back to where it closes: rows of pairs and the Elo range, in order of
 * pairs. Kept for the last few tests, since it takes a moment. */
export function undecidedBand(
  f: Funnel,
  maxPairs: number,
  below: number,
  points = 200,
): [pairs: number, lo: number, hi: number][] {
  const key = `${f.elo0} ${f.elo1} ${f.lower} ${f.upper} ${maxPairs} ${below} ${points}`;
  const kept = bandCache.get(key);
  if (kept) return kept;
  const rows: [number, number, number][] = [];
  const step = Math.max(1, Math.round(maxPairs / points));
  for (let n = maxPairs; n >= 1; n -= step) {
    const band = undecidedAt(f, n, maxPairs, below);
    if (!band) break;
    rows.push([n, ...band]);
  }
  rows.reverse();
  if (bandCache.size >= 8) bandCache.delete(bandCache.keys().next().value!);
  bandCache.set(key, rows);
  return rows;
}
const bandCache = new Map<string, [number, number, number][]>();

/** The test's path: its results after each pair as it completed, taken
 * from its finished games (newest first, as the panel keeps them) in the
 * order they finished, as `stats.rs` counts them. Engine 1 plays gold in
 * a pair's game 1 and silver in game 2. */
export function funnelPath(games: TestGameView[]): PathPoint[] {
  const open = new Map<number, number>();
  const seen = new Set<string>();
  const counts = [0, 0, 0];
  const out: PathPoint[] = [];
  for (let i = games.length - 1; i >= 0; i--) {
    const g = games[i];
    const key = `${g.pair}.${g.game}`;
    if (seen.has(key)) continue;
    seen.add(key);
    const won = (g.winner === 'gold') === (g.game === 1) ? 1 : 0;
    const other = open.get(g.pair);
    if (other === undefined) {
      open.set(g.pair, won);
      continue;
    }
    open.delete(g.pair);
    counts[other + won]++;
    const n = counts[0] + counts[1] + counts[2];
    const m = (0.5 * counts[1] + counts[2]) / n;
    const se = Math.sqrt(pairVar(counts[0], counts[1], counts[2]) / n);
    out.push({
      pairs: n,
      elo: eloFromScore(m),
      interval: se > 0 ? [eloFromScore(m - 1.96 * se), eloFromScore(m + 1.96 * se)] : null,
    });
  }
  return out;
}

/** Every k-th point, keeping the last, so a long path stays light. */
export function thinPath<T>(points: T[], max: number): T[] {
  if (points.length <= max) return points;
  const k = Math.ceil(points.length / max);
  const out = points.filter((_, i) => i % k === 0);
  if ((points.length - 1) % k !== 0) out.push(points[points.length - 1]);
  return out;
}

/** The Elo shown either side of the funnel's middle: as far out as the
 * curves are a sixteenth of the way down (the funnel narrows as 1/pairs,
 * so above that it's a thin slice), and wider if the path strays further
 * once past the first twentieth of the pairs shown (before that it swings
 * by hundreds). A multiple of 10, from 20 to 600. */
export function eloSpan(f: Funnel, path: PathPoint[], pairsShown: number): number {
  const centre = (f.elo0 + f.elo1) / 2;
  const c = cutoffsAt(f, Math.max(10, Math.round(pairsShown / 16)));
  let half = Math.max(
    20,
    c.pass == null ? 9 * Math.abs(f.elo1 - f.elo0) : c.pass - centre,
    c.fail == null ? 0 : centre - c.fail,
  );
  const settled = Math.max(10, pairsShown / 20);
  for (const p of path) {
    if (p.pairs >= settled && Number.isFinite(p.elo)) half = Math.max(half, Math.abs(p.elo - centre) * 1.15);
  }
  return Math.ceil(Math.min(half, 600) / 10) * 10;
}

/** How many pairs the chart runs to: the pair limit, or without one, the
 * slow end of the projection (or what's been played), with room to spare. */
export function pairsShown(t: TestView): number {
  if (t.maxPairs != null && t.maxPairs > 0) return Math.max(t.maxPairs, t.pairs);
  const ahead = t.projection ? Math.ceil(t.projection.gamesLeft[2] / 2) : 0;
  return Math.max(50, Math.ceil((t.pairs + ahead) * 1.15));
}
