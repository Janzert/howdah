import { describe, expect, it } from 'vitest';
import type { GameInfoView } from './bindings/GameInfoView';
import { gameInfoText, recordDate, timeControlsText } from './gameInfo';

const none: GameInfoView = { timeControls: [null, null], event: null, date: null, rated: null, gameId: null };

describe('gameInfoText', () => {
  it('says nothing without tags', () => {
    expect(gameInfoText(none)).toBeNull();
  });

  it('joins what the tags say', () => {
    const info = { ...none, event: 'Casual game', timeControls: ['2m/2m', '2m/2m'], rated: false, gameId: '671438' };
    expect(gameInfoText(info as GameInfoView)).toBe('Casual game · 2m/2m · unrated · #671438');
  });

  it("names each side's time control when they differ", () => {
    expect(timeControlsText(['2m/2m', '30s/5m'])).toBe('Gold 2m/2m · Silver 30s/5m');
    expect(timeControlsText([null, '30s/5m'])).toBe('Silver 30s/5m');
  });

  it('keeps a date with unknown parts as written', () => {
    expect(recordDate('2014.??.??')).toBe('2014.??.??');
    expect(recordDate('2014.01.04')).toContain('2014');
  });
});
