import { describe, expect, it } from 'vitest';
import type { PlayerKind } from './bindings/PlayerKind';
import type { PlayersView } from './bindings/PlayersView';
import { ticksFor } from './clockTicks';

describe('ticksFor', () => {
  const players = (gold: PlayerKind, silver: PlayerKind): PlayersView => {
    const p = (kind: PlayerKind) => ({ kind, name: kind, engineId: null, options: [] });
    return { gold: p(gold), silver: p(silver) };
  };

  it("ticks for a human's clock only, unless no one plays here and the setting is on", () => {
    expect(ticksFor(players('human', 'remote'), 'gold', false)).toBe(true);
    expect(ticksFor(players('human', 'remote'), 'silver', true)).toBe(false);
    expect(ticksFor(players('human', 'engine'), 'silver', true)).toBe(false);
    expect(ticksFor(players('human', 'human'), 'silver', false)).toBe(true);
    expect(ticksFor(players('engine', 'remote'), 'gold', true)).toBe(true);
    expect(ticksFor(players('engine', 'engine'), 'silver', true)).toBe(true);
    expect(ticksFor(players('engine', 'engine'), 'gold', false)).toBe(false);
    expect(ticksFor(players('remote', 'remote'), 'gold', false)).toBe(false);
    expect(ticksFor(players('remote', 'remote'), 'gold', true)).toBe(true);
    expect(ticksFor(players('remote', 'remote'), 'silver', true)).toBe(true);
  });
});
