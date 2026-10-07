import { describe, expect, it } from 'vitest';
import type { ClockView } from './bindings/ClockView';
import { clockLevel, formatClock, nextTick, TICK_TIMES_MS, turnTimeLeft } from './clock';

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

describe('formatClock', () => {
  it('writes minutes, hours and days', () => {
    expect(formatClock(65_000)).toBe('1:05');
    expect(formatClock(3_600_000 + 5_000)).toBe('1:00:05');
    // 60 days, as postal reserves run.
    expect(formatClock(60 * 86_400_000)).toBe('60d 0:00');
    expect(formatClock(86_400_000 + 3 * 3_600_000 + 7 * 60_000 + 30_000)).toBe('1d 3:08');
    expect(formatClock(-5)).toBe('0:00');
  });
});

describe('clockLevel', () => {
  it('turns yellow under 30 s and red under 10 s', () => {
    expect(clockLevel(60_000)).toBe('ok');
    expect(clockLevel(30_000)).toBe('ok');
    expect(clockLevel(29_999)).toBe('warn');
    expect(clockLevel(10_000)).toBe('warn');
    expect(clockLevel(9999)).toBe('low');
    expect(clockLevel(0)).toBe('low');
  });
});
