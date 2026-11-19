import { describe, expect, it } from 'vitest';
import { EMPTY_FORM, dayStart, percent, resultWords, setupPieces, setupWords, toFilter } from './library';

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

  it('reads setup keys for either side', () => {
    const key = 'RRRCCRRRRHDMEDHR';
    expect(setupWords(key, 'gold')).toBe('elephant e2, camel d2, horses b2 g2, dogs c2 f2, cats d1 e1');
    expect(setupWords(key, 'silver')).toBe('elephant e7, camel d7, horses b7 g7, dogs c7 f7, cats d8 e8');
    expect(setupPieces(key, 'gold')).toHaveLength(16);
    expect(setupPieces(key, 'silver')[0]).toEqual({ file: 0, back: true, rank: 8, kind: 'rabbit' });
    expect(percent(1, 3)).toBe('33%');
    expect(percent(0, 0)).toBe('–');
  });

  it('passes the setups on', () => {
    const f = toFilter({ ...EMPTY_FORM, goldSetup: 'RRRCCRRRRHDMEDHR' });
    expect(f).toMatchObject({ goldSetup: 'RRRCCRRRRHDMEDHR', silverSetup: null });
  });

  it('passes a position on only when there is one', () => {
    expect(toFilter(EMPTY_FORM)).toMatchObject({ position: null, positionMatch: null });
    const short = `[${' '.repeat(35)}EM${' '.repeat(27)}]`;
    const f = toFilter({ ...EMPTY_FORM, position: short, positionSide: 'silver', positionMatch: 'pieces' });
    expect(f).toMatchObject({ position: { short, sideToMove: 'silver' }, positionMatch: 'pieces' });
  });
});
