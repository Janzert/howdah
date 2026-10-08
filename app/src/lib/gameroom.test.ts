import { describe, expect, it } from 'vitest';
import type { LiveGameView } from './bindings/LiveGameView';
import type { WatchView } from './bindings/WatchView';
import { mySide, myTurn, newOpponentChat, watchSounds } from './gameroom';

function game(gold: string | null, silver: string | null, turn: LiveGameView['turn']): LiveGameView {
  return { gid: '1', gold, silver, timeControl: '1d/30d/100/0/0', rated: false, postal: true, turn };
}

function watch(gid: string, side: WatchView['side'], chat: WatchView['chat']): WatchView {
  return { gid, side, chat } as WatchView;
}

const line = (side: WatchView['side'], text: string) => ({ side, label: '3g', text });

describe('myTurn', () => {
  it('finds the user by name, ignoring case', () => {
    expect(mySide(game('Alice', 'bob', 'silver'), 'Bob')).toBe('silver');
    expect(myTurn(game('Alice', 'bob', 'silver'), 'Bob')).toBe(true);
    expect(myTurn(game('Alice', 'bob', 'gold'), 'Bob')).toBe(false);
  });

  it('waits for an opponent and a known user', () => {
    expect(myTurn(game('Alice', null, 'gold'), 'Alice')).toBe(false);
    expect(myTurn(game('Alice', 'Bob', 'gold'), null)).toBe(false);
  });

  it("takes a game with no turn yet as gold's setup", () => {
    expect(myTurn(game('Alice', 'Bob', null), 'Alice')).toBe(true);
    expect(myTurn(game('Alice', 'Bob', null), 'Bob')).toBe(false);
  });
});

describe('newOpponentChat', () => {
  it("gives the opponent's new lines at the user's seat", () => {
    const before = watch('7', 'gold', [line('gold', 'hi')]);
    const after = watch('7', 'gold', [line('gold', 'hi'), line('gold', 'gl'), line('silver', 'you too')]);
    expect(newOpponentChat(before, after).map((l) => l.text)).toEqual(['you too']);
  });

  it('ignores a new game, a spectator and lines without a side', () => {
    const chat = [line('silver', 'hi'), line(null, 'server')];
    expect(newOpponentChat(watch('7', 'gold', []), watch('8', 'gold', chat))).toEqual([]);
    expect(newOpponentChat(null, watch('7', 'gold', chat))).toEqual([]);
    expect(newOpponentChat(watch('7', null, []), watch('7', null, chat))).toEqual([]);
    expect(newOpponentChat(watch('7', 'gold', []), watch('7', 'gold', [line(null, 'server')]))).toEqual([]);
  });
});

describe('watchSounds', () => {
  const seat = (over: Partial<WatchView> = {}): WatchView =>
    ({
      gid: '7',
      side: 'gold',
      state: 'following',
      waiting: false,
      away: [false, false],
      refused: null,
      chat: [],
      ...over,
    }) as WatchView;

  it('plays join and leave for the opponent only', () => {
    expect(watchSounds(seat({ waiting: true }), seat())).toEqual(['join']);
    expect(watchSounds(seat(), seat({ away: [false, true] }))).toEqual(['leave']);
    expect(watchSounds(seat({ away: [false, true] }), seat())).toEqual(['join']);
    expect(watchSounds(seat(), seat({ away: [true, false] }))).toEqual([]);
    expect(watchSounds(seat(), seat({ state: 'ended', away: [false, true] }))).toEqual([]);
  });

  it('plays leave and join at the table after the game, while its chat is open', () => {
    const ended = (over: Partial<WatchView> = {}) => seat({ state: 'ended', chatOpen: true, ...over });
    expect(watchSounds(ended(), ended({ away: [false, true] }))).toEqual(['leave']);
    expect(watchSounds(ended({ away: [false, true] }), ended())).toEqual(['join']);
    expect(watchSounds(ended(), ended({ chatOpen: false, away: [false, true] }))).toEqual([]);
  });

  it('plays a refused move once, and chat', () => {
    expect(watchSounds(seat(), seat({ refused: 'Illegal move' }))).toEqual(['illegal']);
    expect(watchSounds(seat({ refused: 'Illegal move' }), seat({ refused: 'Illegal move' }))).toEqual([]);
    expect(watchSounds(seat(), seat({ chat: [line('silver', 'hi')] }))).toEqual(['chat']);
  });

  it('is quiet for a new game and a spectator', () => {
    expect(watchSounds(null, seat({ chat: [line('silver', 'hi')] }))).toEqual([]);
    expect(watchSounds(seat({ gid: '6', waiting: true }), seat())).toEqual([]);
    expect(watchSounds(seat({ side: null }), seat({ side: null, away: [false, true] }))).toEqual([]);
  });
});
