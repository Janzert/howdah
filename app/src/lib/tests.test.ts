import { describe, expect, it } from 'vitest';
import type { PoolView } from './bindings/PoolView';
import type { ProjectionView } from './bindings/ProjectionView';
import type { TestView } from './bindings/TestView';
import {
  briefText,
  cutoffPercent,
  decideText,
  detailsText,
  eloText,
  endingText,
  endsRows,
  endsText,
  endsTimeText,
  gamesPerHour,
  nextGameAfter,
  llrPosition,
  matchupFor,
  oddsText,
  pairCount,
  percentCutoff,
  poolEnginesText,
  poolLeaderText,
  scoreText,
  sprtText,
  testProgress,
  testState,
  zShade,
} from './tests';

function view(over: Partial<TestView> = {}): TestView {
  return {
    id: '0001-t',
    name: 't',
    engines: ['a', 'b'],
    timeControl: null,
    status: 'active',
    outcome: null,
    halted: null,
    priority: 0,
    maxPairs: null,
    concurrency: 1,
    sprt: { elo0: 0, elo1: 10, undecidedBelow: null },
    running: [],
    games: 21,
    pairs: 10,
    wins: [12, 9],
    pairCounts: [2, 4, 4],
    score: 0.6,
    elo: '+70.4 [-20.0, +170.0]',
    los: 0.9,
    llr: 0,
    llrBounds: [-2.94, 2.94],
    decideChance: null,
    projection: null,
    repeated: 0,
    spsa: null,
    pool: null,
    path: '/tmp/t',
    ...over,
  };
}

function pool(): PoolView {
  const engine = (name: string, elo: number | null, error: number | null, retired = false) => ({
    name,
    retired,
    games: elo == null ? 0 : 20,
    wins: 10,
    elo,
    error,
  });
  return {
    engines: [engine('v1', 0, 0), engine('v2', 24.14, 30), engine('v3', -10, 31, true), engine('v4', null, null)],
    ranking: [1, 0, 2, 3],
    anchor: 0,
    matchups: [{ a: 0, b: 1, wins: [3, 9], pairs: 6, expected: 0.47, z: -2.1, outlier: false }],
    minPairs: 2,
    outlierZ: 3,
  };
}

describe('tests', () => {
  it('words a test state', () => {
    expect(testState(view())).toBe('active');
    expect(testState(view({ status: 'finished', outcome: 'passed' }))).toBe('passed');
  });

  it('places the LLR between its bounds', () => {
    expect(llrPosition(view())).toBe(0.5);
    expect(llrPosition(view({ llr: 5 }))).toBe(1);
    expect(llrPosition(view({ llr: null, llrBounds: null }))).toBeNull();
  });

  it('describes the score and the SPRT', () => {
    expect(scoreText(view())).toBe('12–9 in 21 games (10 pairs)');
    expect(scoreText(view(), true)).toBe('12–9 in 21');
    expect(sprtText(view({ llr: 1.234 }))).toBe('LLR 1.23 (-2.94, 2.94) for Elo [0, 10]');
  });

  it('words the chance of deciding in time', () => {
    expect(decideText(view())).toBeNull();
    expect(decideText(view({ maxPairs: 3000, decideChance: 0.623 }))).toBe('62% chance to decide within 3000 pairs');
    expect(decideText(view({ maxPairs: 3000, decideChance: 0.0412 }))).toBe('4.1% chance to decide within 3000 pairs');
    expect(decideText(view({ maxPairs: 3000, decideChance: 0.00001 }))).toBe(
      '<0.1% chance to decide within 3000 pairs',
    );
    expect(decideText(view({ maxPairs: 3000, decideChance: 0.5, status: 'finished' }))).toBeNull();
    const cut = view({ maxPairs: 3000, decideChance: 0.2, sprt: { elo0: 0, elo1: 10, undecidedBelow: 0.05 } });
    expect(decideText(cut)).toBe('20% chance to decide within 3000 pairs (stops under 5%)');
  });

  it('reads and fills a cutoff field in percent', () => {
    expect(percentCutoff('')).toBeNull();
    expect(percentCutoff(0)).toBeNull();
    expect(percentCutoff(5)).toBe(0.05);
    expect(percentCutoff('2.5')).toBe(0.025);
    expect(cutoffPercent(view())).toBeNull();
    expect(cutoffPercent(view({ sprt: { elo0: 0, elo1: 10, undecidedBelow: 0.07 } }))).toBe(7);
  });

  it('reads a pair limit field', () => {
    expect(pairCount('')).toBeNull();
    expect(pairCount(null)).toBeNull();
    expect(pairCount(12)).toBe(12);
  });
});

describe('endingText', () => {
  it('words the reasons that need it', () => {
    expect(endingText('goal')).toBe('goal');
    expect(endingText('illegalMove')).toBe('illegal move');
    expect(endingText('resourceLimit')).toBe('a memory-limit forfeit');
  });
});

describe('testProgress', () => {
  it('counts games against the pair limit', () => {
    expect(testProgress(view())).toBeNull();
    expect(testProgress(view({ maxPairs: 100, games: 21 }))).toBeCloseTo(0.105);
    expect(testProgress(view({ maxPairs: 10, games: 25 }))).toBe(1);
  });
});

describe('briefText', () => {
  it('gives the score, Elo, LLR and games left', () => {
    expect(briefText(view({ llr: 0.4412 }))).toBe('12–9 +70.4 0.44');
    expect(briefText(view({ elo: '+inf', llr: null }))).toBe('12–9 +inf');
    expect(briefText(view({ elo: null, llr: null }))).toBe('12–9');
    const projection = {
      pass: 0.5,
      fail: 0.5,
      undecided: 0,
      done: 0,
      gamesLeft: [925, 1843, 3612] as [number, number, number],
      gamesIfDecided: null,
      gamesIfNot: null,
      openEnded: false,
    };
    expect(briefText(view({ llr: 0.4412, maxPairs: 3000, decideChance: 0.623 }))).toBe('12–9 +70.4 0.44');
    expect(briefText(view({ llr: 0.4412, projection }))).toBe('12–9 +70.4 0.44 1800');
    expect(briefText(view({ llr: 0.4412, projection: { ...projection, gamesLeft: [0, 0, 0] } }))).toBe(
      '12–9 +70.4 0.44',
    );
    expect(briefText(view({ llr: 3, projection, status: 'finished' }))).toBe('12–9 +70.4 3.00');
  });
});

describe('projections', () => {
  const projection: ProjectionView = {
    pass: 0.2234,
    fail: 0.36,
    undecided: 0.4166,
    done: 0,
    gamesLeft: [925, 1843, 3612],
    gamesIfDecided: [310, 1024, 2650],
    gamesIfNot: [3800, 4410, 5200],
    openEnded: false,
  };

  it('words when a test is likely to end', () => {
    expect(endsText(view())).toBeNull();
    expect(endsText(view({ projection }))).toBe('in about 1800 games (930–3600)');
    expect(endsText(view({ projection: { ...projection, gamesLeft: [12, 64, 230] } }))).toBe(
      'in about 64 games (12–230)',
    );
    expect(endsText(view({ projection: { ...projection, openEnded: true } }))).toBe('in about 1800 games (930–3600+)');
    expect(endsText(view({ projection: { ...projection, gamesLeft: [0, 0, 0] } }))).toBe(
      'once the running games finish',
    );
    expect(endsText(view({ projection, status: 'finished' }))).toBeNull();
    expect(endsText(view({ projection }), [310, 1024, 2650])).toBe('in about 1000 games (310–2700)');
  });

  it('splits the ends when a test may well decide and may well not', () => {
    const one = (p: ProjectionView, over: Partial<TestView> = {}) => endsRows(view({ projection: p, ...over }));
    expect(one(projection, { maxPairs: 3000 })).toEqual([
      { label: 'if it decides', gamesLeft: [310, 1024, 2650] },
      { label: "if it doesn't", gamesLeft: [3800, 4410, 5200] },
    ]);
    const all = [{ label: null, gamesLeft: [925, 1843, 3612] }];
    // No pair limit: not deciding is just going on.
    expect(one(projection)).toEqual(all);
    // Likely to decide, or likely not to.
    expect(one({ ...projection, pass: 0.5, fail: 0.4, undecided: 0.1 }, { maxPairs: 3000 })).toEqual(all);
    expect(one({ ...projection, pass: 0.05, fail: 0.05, undecided: 0.9 }, { maxPairs: 3000 })).toEqual(all);
    expect(one({ ...projection, gamesIfNot: null }, { maxPairs: 3000 })).toEqual(all);
    expect(one(projection, { maxPairs: 3000, status: 'finished' })).toBeNull();
    expect(endsRows(view())).toBeNull();
  });

  it('words how it is likely to end', () => {
    expect(oddsText(view())).toBeNull();
    expect(oddsText(view({ projection, maxPairs: 3000 }))).toBe('pass 22%, fail 36%, undecided 42%');
    const done = { ...projection, undecided: 0, done: 0.4166 };
    expect(oddsText(view({ projection: done, maxPairs: 3000 }))).toBe('pass 22%, fail 36%, all pairs played 42%');
    expect(oddsText(view({ projection: done }))).toBe('pass 22%, fail 36%, still going 42%');
  });
});

describe('detailsText', () => {
  it('gives a line for each detail', () => {
    expect(detailsText(view({ elo: null, sprt: null, llr: null, llrBounds: null }))).toBe(
      '12–9 in 21 games (10 pairs)',
    );
    expect(detailsText(view({ maxPairs: 3000, decideChance: 0.623 })).split('\n')).toEqual([
      '12–9 in 21 games (10 pairs)',
      'Elo +70.4 [-20.0, +170.0]',
      'LLR 0.00 (-2.94, 2.94) for Elo [0, 10]',
      '62% chance to decide within 3000 pairs',
    ]);
    const projection = {
      pass: 0.5,
      fail: 0.5,
      undecided: 0,
      done: 0,
      gamesLeft: [10, 20, 30] as [number, number, number],
      gamesIfDecided: [10, 20, 30] as [number, number, number],
      gamesIfNot: null,
      openEnded: false,
    };
    expect(detailsText(view({ projection })).split('\n').at(-1)).toBe(
      'Ends in about 20 games (10–30): pass 50%, fail 50%',
    );
    expect(
      detailsText(view({ projection, maxPairs: 3000, decideChance: 0.623 }))
        .split('\n')
        .slice(-2),
    ).toEqual(['Ends in about 20 games (10–30): pass 50%, fail 50%', '62% chance to decide within 3000 pairs']);
  });
});

describe('time left', () => {
  it('finds the pace since the last pause', () => {
    // A game a minute for ten games, after a day's pause.
    const recent = Array.from({ length: 10 }, (_, i) => 100_000 - 60 * i);
    expect(gamesPerHour(recent)).toBeCloseTo(60);
    expect(gamesPerHour([...recent, 10_000, 9_000])).toBeCloseTo(60);
    expect(gamesPerHour([300, 200, 100])).toBeNull();
    expect(gamesPerHour([])).toBeNull();
  });

  it('turns the games left into hours', () => {
    const projection = {
      pass: 0.2,
      fail: 0.3,
      undecided: 0.5,
      done: 0,
      gamesIfDecided: null,
      gamesIfNot: null,
      openEnded: false,
    };
    const t = (gamesLeft: [number, number, number], over: Partial<TestView> = {}) =>
      view({ projection: { ...projection, gamesLeft }, ...over });
    expect(endsTimeText(t([60, 360, 720]), 60)).toBe('about 6 h (1–12 h)');
    expect(endsTimeText(t([20, 60, 4800]), 60)).toBe('about 1 h (20 min–3 days)');
    expect(endsTimeText(t([20, 60, 4800]), null)).toBeNull();
    expect(endsTimeText(t([20, 60, 4800], { status: 'paused' }), 60)).toBeNull();
    expect(endsTimeText(t([0, 0, 0]), 60)).toBeNull();
    expect(endsTimeText(t([0, 0, 0]), 60, [600, 1200, 2400])).toBe('about 20 h (10–40 h)');
  });

  it('words a pool', () => {
    const p = pool();
    expect(eloText(p.engines[0])).toBe('0');
    expect(eloText(p.engines[1])).toBe('+24.1 ±30.0');
    expect(eloText(p.engines[2])).toBe('-10.0 ±31.0');
    expect(eloText(p.engines[3])).toBe('–');
    expect(poolEnginesText(p)).toBe('3 engines playing, 1 retired');
    expect(poolLeaderText(p)).toBe('v2 leads by 24.1');
    expect(poolLeaderText({ ...p, anchor: null })).toBeNull();
    const t = view({ pool: p, games: 40 });
    expect(briefText(t)).toBe('4 engines, v2 leads');
    expect(detailsText(t)).toBe('Pool of 3 engines playing, 1 retired, 40 games\nv2 leads by 24.1');
  });

  it('reads a matchup from either side', () => {
    const p = pool();
    expect(matchupFor(p, 0, 1)).toEqual({ wins: 3, losses: 9, pairs: 6, expected: 0.47, z: -2.1, outlier: false });
    const back = matchupFor(p, 1, 0)!;
    expect([back.wins, back.losses, back.z]).toEqual([9, 3, 2.1]);
    expect(back.expected).toBeCloseTo(0.53);
    expect(matchupFor(p, 0, 3)).toBeNull();
  });

  it('shades a result by how far out of line it is', () => {
    expect(zShade(0.3, 3)).toBeNull();
    expect(zShade(1.5, 3)).toBe('color-mix(in srgb, var(--accent) 28%, transparent)');
    expect(zShade(-9, 3)).toBe('color-mix(in srgb, var(--warn) 55%, transparent)');
  });
});

describe('nextGameAfter', () => {
  const g = (pair: number, game: number) => ({ pair, game });
  it('takes the first game started after the one given', () => {
    expect(nextGameAfter(g(3, 1), [g(5, 1), g(1, 2), g(3, 2), g(4, 1), g(3, 1)])).toEqual(g(3, 2));
    expect(nextGameAfter(g(3, 2), [g(5, 1), g(4, 2), g(2, 1)])).toEqual(g(4, 2));
  });
  it('is null when none is later', () => {
    expect(nextGameAfter(g(3, 2), [g(1, 1), g(3, 1), g(3, 2)])).toBeNull();
    expect(nextGameAfter(g(1, 1), [])).toBeNull();
  });
});

describe('a test that plays every pair', () => {
  const fixed = (over: Partial<TestView> = {}) =>
    view({ sprt: null, llr: null, llrBounds: null, maxPairs: 100, ...over });

  it('ends after its exact games left', () => {
    expect(endsText(fixed())).toBe('in 179 games');
    expect(endsText(fixed({ games: 199 }))).toBe('in 1 game');
    expect(endsText(fixed({ games: 200 }))).toBe('once the running games finish');
    expect(endsText(fixed({ status: 'finished' }))).toBeNull();
    expect(endsText(fixed({ maxPairs: null }))).toBeNull();
    expect(endsRows(fixed())).toEqual([{ label: null, gamesLeft: [179, 179, 179] }]);
  });

  it('ends in time at the pace', () => {
    expect(endsTimeText(fixed(), 30)).toBe('about 6 h');
    expect(endsTimeText(fixed(), null)).toBeNull();
    expect(endsTimeText(fixed({ status: 'paused' }), 30)).toBeNull();
    expect(endsTimeText(fixed({ games: 200 }), 30)).toBeNull();
  });

  it('shows the games left in brief and in its details', () => {
    expect(briefText(fixed())).toBe('12–9 +70.4 179');
    expect(detailsText(fixed()).split('\n').at(-1)).toBe('Ends in 179 games');
  });
});
