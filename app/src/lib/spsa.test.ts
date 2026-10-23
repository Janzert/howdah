import { describe, expect, it } from 'vitest';
import type { SpsaParamView } from './bindings/SpsaParamView';
import type { SpsaStepView } from './bindings/SpsaStepView';
import {
  durationText,
  iterationsPerHour,
  meanMovement,
  moved,
  movedPastNoise,
  optionsText,
  orderParams,
  shareText,
  thin,
  valueText,
} from './spsa';

function param(over: Partial<SpsaParamView> = {}): SpsaParamView {
  return { name: 'w', value: 1, start: 1, min: 0, max: 2, int: false, c: 0.1, ...over };
}

function step(finished: number, theta: number[] = []): SpsaStepView {
  return { iteration: 1, wins: 8, losses: 8, theta, finished };
}

describe('spsa', () => {
  it('measures a move as a share of the range, and against c', () => {
    const p = param({ value: 1.5 });
    expect(moved(p)).toBe(0.25);
    expect(movedPastNoise(p)).toBe(true);
    expect(movedPastNoise(param({ value: 1.05 }))).toBe(false);
  });

  it('words values and shares', () => {
    expect(valueText(12.3456)).toBe('12.35');
    expect(valueText(-0.012345)).toBe('−0.01235');
    expect(valueText(3.456, true)).toBe('3.46');
    expect(valueText(0)).toBe('0');
    expect(shareText(0.123)).toBe('+12.3%');
    expect(shareText(-0.004)).toBe('−0.4%');
    expect(shareText(0.0001)).toBe('0.0%');
  });

  it('writes θ as option lines, integers rounded', () => {
    const text = optionsText([param({ name: 'a', value: 1.23456789 }), param({ name: 'b', value: 6.6, int: true })]);
    expect(text).toBe('a = 1.23457\nb = 7');
  });

  it('averages the movement after each step', () => {
    const params = [param({ start: 1 }), param({ name: 'v', start: 0, min: -1, max: 1 })];
    const [first, second] = meanMovement(params, [step(0, [1.2, 0.2]), step(0, [1, 0])]);
    expect(first).toBeCloseTo(0.1);
    expect(second).toBe(0);
  });

  it('thins a series, keeping both ends', () => {
    const xs = Array.from({ length: 101 }, (_, i) => i);
    const t = thin(xs, 11);
    expect(t).toHaveLength(11);
    expect(t[0]).toEqual([0, 0]);
    expect(t[10]).toEqual([100, 100]);
    expect(thin([1, 2], 5)).toEqual([
      [0, 1],
      [1, 2],
    ]);
  });

  it('orders and filters the parameters', () => {
    const params = [
      param({ name: 'b', value: 1.1 }),
      param({ name: 'a', value: 0.5 }),
      param({ name: 'c', value: 1.3 }),
    ];
    expect(orderParams(params, 'spec', '')).toEqual([0, 1, 2]);
    expect(orderParams(params, 'moved', '')).toEqual([1, 2, 0]);
    expect(orderParams(params, 'name', '')).toEqual([1, 0, 2]);
    expect(orderParams(params, 'spec', 'C')).toEqual([2]);
  });

  it('paces iterations since the last long gap', () => {
    // A pause, then an iteration every 60 s.
    const steps = [step(0), ...[10000, 10060, 10120, 10180, 10240].map((t) => step(t))];
    expect(iterationsPerHour(steps)).toBe(60);
    expect(iterationsPerHour(steps.slice(0, 3))).toBeNull();
  });

  it('words a stretch of time', () => {
    expect(durationText(0.5)).toBe('30 min');
    expect(durationText(2.25)).toBe('2.3 h');
    expect(durationText(30)).toBe('30 h');
    expect(durationText(100)).toBe('4.2 days');
  });
});
