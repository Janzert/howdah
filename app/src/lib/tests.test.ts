import { describe, expect, it } from 'vitest';
import type { TestView } from './bindings/TestView';
import { llrPosition, pairCount, scoreText, sprtText, testState } from './tests';

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
    sprt: { elo0: 0, elo1: 10 },
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
    expect(sprtText(view({ llr: 1.234 }))).toBe('LLR 1.23 (-2.94, 2.94) for Elo [0, 10]');
  });

  it('reads a pair limit field', () => {
    expect(pairCount('')).toBeNull();
    expect(pairCount(null)).toBeNull();
    expect(pairCount(12)).toBe(12);
  });
});
