import { describe, expect, it } from 'vitest';
import type { TestView } from './bindings/TestView';
import {
  briefText,
  cutoffPercent,
  decideText,
  detailsText,
  endingText,
  endsText,
  llrPosition,
  oddsText,
  pairCount,
  percentCutoff,
  scoreText,
  sprtText,
  testProgress,
  testState,
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
    path: '/tmp/t',
    ...over,
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
    expect(scoreText(view(), false)).toBe('12–9 in 21 games');
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
  const projection = {
    pass: 0.2234,
    fail: 0.36,
    undecided: 0.4166,
    done: 0,
    gamesLeft: [925, 1843, 3612] as [number, number, number],
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
