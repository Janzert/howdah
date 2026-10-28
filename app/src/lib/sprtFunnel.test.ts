import { describe, expect, it } from 'vitest';
import type { TestGameView } from './bindings/TestGameView';
import type { TestView } from './bindings/TestView';
import {
  eloFromScore,
  eloSpan,
  funnelCurves,
  funnelOf,
  funnelPath,
  llrAt,
  pairsShown,
  reachChance,
  scoreFromElo,
  thinPath,
  undecidedBand,
  type Funnel,
} from './sprtFunnel';

const BOUND = Math.log(0.95 / 0.05);

function funnel(elo0 = 0, elo1 = 10): Funnel {
  return funnelOf({ sprt: { elo0, elo1, undecidedBelow: null }, llrBounds: [-BOUND, BOUND] } as TestView)!;
}

/** `stats.rs`'s LLR from pair counts, the variance padded with a pair of
 * each outcome. */
function rustLlr(f: Funnel, counts: [number, number, number]): number {
  const meanVar = (c: number[]) => {
    const t = c[0] + c[1] + c[2];
    const m = (0.5 * c[1] + c[2]) / t;
    return [m, (c[0] * m * m + c[1] * (0.5 - m) ** 2 + c[2] * (1 - m) ** 2) / t];
  };
  const n = counts[0] + counts[1] + counts[2];
  const [m] = meanVar(counts);
  const [, v] = meanVar(counts.map((c) => c + 1));
  return (n * (f.s1 - f.s0) * (2 * m - f.s0 - f.s1)) / (2 * v);
}

function game(pair: number, g: number, engine1Won: boolean): TestGameView {
  const gold = g === 1 ? 'a' : 'b';
  const silver = g === 1 ? 'b' : 'a';
  return {
    test: 't',
    pair,
    game: g,
    gold,
    silver,
    winner: engine1Won === (g === 1) ? 'gold' : 'silver',
    reason: 'g',
    plies: 40,
    detail: null,
    finished: 0,
  };
}

describe('funnel', () => {
  it('reads the bounds from the test', () => {
    const f = funnel(0, 10);
    expect(f.s0).toBe(0.5);
    expect(f.s1).toBeCloseTo(scoreFromElo(10));
    expect(f.lower).toBe(-BOUND);
    expect(funnelOf({ sprt: null, llrBounds: null } as TestView)).toBeNull();
  });

  it('pads the variance as stats.rs does', () => {
    const f = funnel();
    // Counts that are exactly what games won with chance m give.
    expect(llrAt(f, 100, 0.5)).toBeCloseTo(rustLlr(f, [25, 50, 25]), 12);
    expect(llrAt(f, 100, 0.6)).toBeCloseTo(rustLlr(f, [16, 48, 36]), 12);
  });

  it('puts the LLR on a bound along each curve', () => {
    const f = funnel();
    const { fail, pass } = funnelCurves(f, 3000);
    expect(pass.length).toBeGreaterThan(100);
    for (const [n, e] of [pass[0], pass[50], pass[pass.length - 1]]) {
      expect(llrAt(f, n, scoreFromElo(e))).toBeCloseTo(BOUND, 6);
    }
    for (const [n, e] of [fail[0], fail[fail.length - 1]]) {
      expect(llrAt(f, n, scoreFromElo(e))).toBeCloseTo(-BOUND, 6);
    }
    expect(pass[pass.length - 1][0]).toBe(3000);
  });

  it('narrows like a funnel toward the middle', () => {
    const f = funnel();
    const { fail, pass } = funnelCurves(f, 3000);
    const at = (c: [number, number][], n: number) => c.find(([p]) => p >= n)![1];
    expect(at(pass, 100)).toBeGreaterThan(at(pass, 1000));
    expect(at(pass, 1000)).toBeGreaterThan(5);
    expect(at(fail, 100)).toBeLessThan(at(fail, 1000));
    expect(at(fail, 1000)).toBeLessThan(5);
    // About 18000/n Elo from the middle once the padding doesn't matter.
    expect(at(pass, 1000) - 5).toBeGreaterThan(14);
    expect(at(pass, 1000) - 5).toBeLessThan(23);
  });

  it('marks where a test stops undecided', () => {
    const f = funnel();
    const band = undecidedBand(f, 3000, 0.05);
    expect(band.length).toBeGreaterThan(5);
    const [n, lo, hi] = band[0];
    expect(band[band.length - 1][0]).toBe(3000);
    expect(lo).toBeLessThan(5);
    expect(hi).toBeGreaterThan(5);
    // On its edges the chance to decide is the cutoff.
    expect(reachChance(f, n, scoreFromElo(lo), 3000)).toBeCloseTo(0.05, 2);
    expect(reachChance(f, n, scoreFromElo(hi), 3000)).toBeCloseTo(0.05, 2);
    expect(reachChance(f, n, scoreFromElo(5), 3000)).toBeLessThan(0.05);
    // Kept for the next call.
    expect(undecidedBand(f, 3000, 0.05)).toBe(band);
  });

  it('follows the pairs as they complete', () => {
    // Newest first, as the panel keeps them: pair 2 completes before 1.
    const games = [game(1, 2, true), game(2, 2, false), game(2, 1, true), game(1, 1, true)].map((g, i) => ({
      ...g,
      finished: 10 - i,
    }));
    const path = funnelPath(games);
    expect(path.map((p) => p.pairs)).toEqual([1, 2]);
    // Pair 2 split, then pair 1 won both.
    expect(path[0].elo).toBeCloseTo(0);
    expect(path[0].interval).toBeNull();
    expect(path[1].elo).toBeCloseTo(eloFromScore(0.75));
    expect(path[1].interval![0]).toBeLessThan(path[1].elo);
    // A game listed twice counts once.
    expect(funnelPath([games[3], ...games]).map((p) => p.pairs)).toEqual([1, 2]);
  });

  it('thins long paths, keeping the last point', () => {
    const xs = Array.from({ length: 1001 }, (_, i) => i);
    const thin = thinPath(xs, 100);
    expect(thin.length).toBeLessThanOrEqual(102);
    expect(thin[thin.length - 1]).toBe(1000);
    expect(thinPath(xs.slice(0, 50), 100)).toHaveLength(50);
  });

  it('scales the Elo shown with the funnel and the path', () => {
    const f = funnel(0, 10);
    // The curves a sixteenth of the way down: about 18000/n from the middle.
    const span = eloSpan(f, [], 3000);
    expect(span).toBeGreaterThanOrEqual(80);
    expect(span).toBeLessThanOrEqual(120);
    expect(eloSpan(funnel(0, 5), [], 3000)).toBeGreaterThan(span);
    // An early swing doesn't widen it; a settled stray does.
    expect(eloSpan(f, [{ pairs: 20, elo: 400, interval: null }], 3000)).toBe(span);
    expect(eloSpan(f, [{ pairs: 1000, elo: 150, interval: null }], 3000)).toBe(170);
  });

  it('runs to the pair limit, or past the projection', () => {
    const t = { maxPairs: 3000, pairs: 10, projection: null } as TestView;
    expect(pairsShown(t)).toBe(3000);
    expect(pairsShown({ ...t, maxPairs: null, pairs: 400 })).toBe(460);
    const projection = { gamesLeft: [100, 400, 1200] } as TestView['projection'];
    expect(pairsShown({ ...t, maxPairs: null, pairs: 400, projection })).toBe(Math.ceil(1000 * 1.15));
  });
});
