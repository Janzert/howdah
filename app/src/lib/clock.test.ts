import { describe, expect, it } from 'vitest';
import type { ClockView } from './bindings/ClockView';
import { clockLevel, formatClock, formatShort, nextTick, sideTimes, TICK_TIMES_MS, turnTimeLeft } from './clock';

describe('turnTimeLeft', () => {
  const clock: ClockView = {
    gold: null,
    silver: null,
    running: 'gold',
    turnElapsedMs: 4000,
    turnAllowanceMs: 15_000,
    gameRemainingMs: null,
    past: false,
  };

  it('counts on from the view and stops at zero', () => {
    expect(turnTimeLeft(clock, 0)).toBe(11_000);
    expect(turnTimeLeft(clock, 1000)).toBe(10_000);
    expect(turnTimeLeft(clock, 60_000)).toBe(0);
  });
});

describe('sideTimes', () => {
  const side = { timeControl: '30s/2m', moveTimeMs: 30_000, reserveMs: 120_000, turnAllowanceMs: 150_000 };

  it('counts the running side down, move time first', () => {
    const clock: ClockView = {
      gold: { ...side, lastUsedMs: null },
      silver: null,
      running: 'gold',
      turnElapsedMs: 40_000,
      turnAllowanceMs: 150_000,
      gameRemainingMs: null,
      past: false,
    };
    expect(sideTimes(clock, 'gold', 0)).toEqual({
      move: 0,
      reserve: 110_000,
      running: true,
      used: false,
      level: 'ok',
    });
    expect(sideTimes(clock, 'silver', 0)).toBeNull();
  });

  it("shows the time a past move took on the side that made it", () => {
    const clock: ClockView = {
      gold: { ...side, lastUsedMs: 12_000 },
      silver: { ...side, reserveMs: 5000, turnAllowanceMs: 35_000, lastUsedMs: null },
      running: null,
      turnElapsedMs: 0,
      turnAllowanceMs: 0,
      gameRemainingMs: null,
      past: true,
    };
    expect(sideTimes(clock, 'gold', 9999)).toEqual({
      move: 12_000,
      reserve: 120_000,
      running: false,
      used: true,
      level: 'ok',
    });
    expect(sideTimes(clock, 'silver', 0)).toMatchObject({ move: 30_000, used: false, level: 'ok' });
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

describe('formatShort', () => {
  it('keeps the two largest units, rounded down', () => {
    expect(formatShort(0)).toBe('0:00');
    expect(formatShort(7_900)).toBe('0:07');
    expect(formatShort(59 * 60_000 + 59_999)).toBe('59:59');
    expect(formatShort(3_600_000 + 5 * 60_000 + 59_000)).toBe('1h05');
    expect(formatShort(23 * 3_600_000 + 59 * 60_000)).toBe('23h59');
    expect(formatShort(2 * 86_400_000 + 3 * 3_600_000 + 50 * 60_000)).toBe('2d03');
    expect(formatShort(-5)).toBe('0:00');
  });
});
