import { describe, expect, it } from 'vitest';
import type { GameroomGames } from './bindings/GameroomGames';
import type { LiveGameView } from './bindings/LiveGameView';
import { countNext, pickNext } from './nextGame';

function lists(mine: Partial<LiveGameView>[]): GameroomGames {
  return {
    user: 'me',
    live: [],
    recent: [],
    open: [],
    invitations: [],
    mine: mine.map((g) => ({ gold: 'me', silver: 'you', turn: 'gold', postal: true, ...g }) as LiveGameView),
  };
}

describe('pickNext', () => {
  it('goes to the next waiting window after this one, wrapping around', () => {
    expect(pickNext(3, [1, 3, 5], null, new Set())).toEqual({ kind: 'window', session: 5 });
    expect(pickNext(5, [1, 3, 5], null, new Set())).toEqual({ kind: 'window', session: 1 });
  });

  it('then a postal game on the user\'s move without a window', () => {
    const games = lists([
      { gid: '1', turn: 'silver' },
      { gid: '2' },
      { gid: '3' },
    ]);
    expect(pickNext(3, [3], games, new Set(['2']))).toEqual({ kind: 'postal', gid: '3', side: 'gold' });
    expect(countNext(3, [3], games, new Set(['2']))).toBe(1);
  });

  it('finds nothing when nothing else waits', () => {
    expect(pickNext(3, [3], lists([{ gid: '1', postal: false }]), new Set())).toBeNull();
    expect(countNext(3, [], null, new Set())).toBe(0);
  });
});
