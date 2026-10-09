import { describe, expect, it } from 'vitest';
import type { PlayerKind } from './bindings/PlayerKind';
import type { SessionView } from './bindings/SessionView';
import { gameTitle, waitsOnUser } from './windowList';

function match(gold: PlayerKind, silver: PlayerKind, livePly: number, extra: Partial<SessionView> = {}): SessionView {
  return {
    players: { gold: { kind: gold, name: 'Al' }, silver: { kind: silver, name: 'Bob' } },
    live: 5,
    cursor: 5,
    livePly,
    result: null,
    clock: null,
    tree: [],
    tagNames: [null, null],
    ...extra,
  } as unknown as SessionView;
}

describe('waitsOnUser', () => {
  it('is the human side to move against an engine or a remote player', () => {
    expect(waitsOnUser(match('human', 'engine', 2))).toBe(true);
    expect(waitsOnUser(match('human', 'engine', 3))).toBe(false);
    expect(waitsOnUser(match('remote', 'human', 3))).toBe(true);
  });

  it('follows the running clock over the shown line', () => {
    const clock = { running: 'silver' } as SessionView['clock'];
    expect(waitsOnUser(match('engine', 'human', 2, { clock, livePly: null }))).toBe(true);
  });

  it("isn't for people at one board, spectators or finished games", () => {
    expect(waitsOnUser(match('human', 'human', 2))).toBe(false);
    expect(waitsOnUser(match('remote', 'remote', 2))).toBe(false);
    expect(waitsOnUser(match('human', 'engine', 2, { result: { winner: 'gold', reason: 'goal' } }))).toBe(false);
    expect(waitsOnUser({ ...match('human', 'engine', 2), players: null })).toBe(false);
  });
});

describe('gameTitle', () => {
  it('names the players, from the match or the record', () => {
    expect(gameTitle(match('human', 'engine', 2))).toBe('Al - Bob');
    expect(gameTitle({ ...match('human', 'engine', 2), players: null, tagNames: ['Cy', null] })).toBe('Cy - ?');
    expect(gameTitle({ ...match('human', 'engine', 2), players: null })).toBeNull();
  });
});
