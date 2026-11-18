import { describe, expect, it } from 'vitest';
import { EMPTY_FORM, dayStart, resultWords, toFilter } from './library';

describe('library', () => {
  it('words results, old draws included', () => {
    expect(resultWords({ winner: 'g', termination: 'g' })).toBe('Gold won by goal');
    expect(resultWords({ winner: 's', termination: null })).toBe('Silver won');
    expect(resultWords({ winner: 'd', termination: 'n' })).toBe('Draw (no rabbits left)');
    expect(resultWords({ winner: null, termination: null })).toBe('No result');
  });

  it('reads dates as UTC days', () => {
    expect(dayStart('2018-11-01')).toBe(1541030400);
    expect(dayStart('2018-11-01', true)).toBe(1541116800);
    expect(dayStart('')).toBeNull();
  });

  it('counts the side and result only with a player', () => {
    const f = toFilter({ ...EMPTY_FORM, side: 'gold', outcome: 'won', opponent: ' Bob ', rated: 'unrated' });
    expect(f).toMatchObject({ player: null, side: null, outcome: null, opponent: 'Bob', rated: false });
    expect(toFilter({ ...EMPTY_FORM, player: 'Ann', side: 'gold' }).side).toBe('gold');
  });
});
