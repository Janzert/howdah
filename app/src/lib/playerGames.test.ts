import { describe, expect, it } from 'vitest';
import type { PastGameView } from './bindings/PastGameView';
import type { WinReason } from './bindings/WinReason';
import { endingStats, filterGames, NO_FILTER, outcome, tally } from './playerGames';

const game = (
  gid: string,
  gold: string,
  silver: string,
  winner: 'gold' | 'silver' | null,
  reason: WinReason = 'goal',
  rated = true,
): PastGameView => ({
  gid,
  gold,
  silver,
  goldRating: null,
  silverRating: null,
  timeControl: null,
  rated,
  result: winner ? { winner, reason } : null,
  moves: 40,
  finished: null,
  comments: 0,
});

const games = [
  game('1', 'Fritzlein', 'bot_Bomb', 'gold', 'goal'),
  game('2', 'bot_Bomb', 'fritzlein', 'gold', 'elimination'),
  game('3', 'bot_Clueless', 'Fritzlein', 'silver', 'timeout', false),
  game('4', 'Fritzlein', 'bot_Sharp', 'silver', 'goal'),
  game('5', 'Fritzlein', 'bot_Sharp', null),
];

describe('outcome', () => {
  it('reads the player side case-insensitively', () => {
    expect(games.map((g) => outcome(g, 'Fritzlein'))).toEqual(['win', 'loss', 'win', 'loss', null]);
    expect(outcome(games[0], 'someone')).toBeNull();
  });
});

describe('endingStats', () => {
  it('counts wins and losses by ending, in rule order', () => {
    expect(endingStats(games, 'Fritzlein')).toEqual([
      { reason: 'goal', wins: 1, losses: 1 },
      { reason: 'elimination', wins: 0, losses: 1 },
      { reason: 'timeout', wins: 1, losses: 0 },
    ]);
  });
});

describe('filterGames', () => {
  const ids = (f: Partial<typeof NO_FILTER>) =>
    filterGames(games, 'Fritzlein', { ...NO_FILTER, ...f }).map((g) => g.gid);
  it('filters by result, ending and rated together', () => {
    expect(ids({})).toEqual(['1', '2', '3', '4', '5']);
    expect(ids({ outcome: 'win' })).toEqual(['1', '3']);
    expect(ids({ outcome: 'loss', reason: 'goal' })).toEqual(['4']);
    expect(ids({ rated: false })).toEqual(['3']);
    expect(ids({ rated: true, outcome: 'win' })).toEqual(['1']);
  });
});

describe('tally', () => {
  it('counts wins and losses, skipping games without a result', () => {
    expect(tally(games, 'Fritzlein')).toEqual({ wins: 2, losses: 2 });
  });
});
