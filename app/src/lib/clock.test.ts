import { describe, expect, it } from 'vitest';
import type { ClockView } from './bindings/ClockView';
import { nextTick, TICK_TIMES_MS, turnTimeLeft } from './clock';

describe('turnTimeLeft', () => {
  const clock: ClockView = {
    gold: null,
    silver: null,
    running: 'gold',
    turnElapsedMs: 4000,
    turnAllowanceMs: 15_000,
    gameRemainingMs: null,
  };

  it('counts on from the view and stops at zero', () => {
    expect(turnTimeLeft(clock, 0)).toBe(11_000);
    expect(turnTimeLeft(clock, 1000)).toBe(10_000);
    expect(turnTimeLeft(clock, 60_000)).toBe(0);
  });
});

describe('nextTick', () => {
  it('waits for 30 s, then speeds up', () => {
    expect(nextTick(60_000)).toBe(30_000);
    expect(nextTick(30_000)).toBe(20_000);
    expect(nextTick(9500)).toBe(9000);
    expect(nextTick(4800)).toBe(4500);
    expect(nextTick(2400)).toBe(2250);
  });

  it('never repeats a tick when the timer fires early', () => {
    expect(nextTick(9001, 9000)).toBe(8000);
  });

  it('ends after the last tick', () => {
    expect(nextTick(250)).toBeNull();
    expect(nextTick(5000, 250)).toBeNull();
  });

  it('plays every tick in order, largest first', () => {
    const played: number[] = [];
    let left = 40_000;
    let last: number | undefined;
    for (let tick = nextTick(left); tick != null; tick = nextTick(left, last)) {
      played.push(tick);
      last = tick;
      left = tick - 3; // the timer fires a little late
    }
    expect(played).toEqual(TICK_TIMES_MS);
  });
});
