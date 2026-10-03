import { describe, expect, it } from 'vitest';
import { analysisEngine, formatEval, formatRabbits, goldShare, nodesPerSecond, pvMoves } from './analysis';
import type { AnalysisLine } from './bindings/AnalysisLine';
import type { EngineSpec } from './bindings/EngineSpec';

const line = (over: Partial<AnalysisLine> = {}): AnalysisLine => ({
  node: 3,
  depth: '12',
  eval: { kind: 'centiRabbits', value: 35 },
  pv: [
    { label: '3g', notation: 'Ee2n Ee3n', steps: null },
    { label: '3s', notation: 'ee7s', steps: null },
  ],
  nodes: 2_400_000,
  timeMs: 2000,
  ...over,
});

describe('analysis display', () => {
  it('reads evals from gold’s side', () => {
    expect(formatEval({ kind: 'centiRabbits', value: 35 })).toBe('+0.35');
    expect(formatEval({ kind: 'centiRabbits', value: -120 })).toBe('−1.20');
    expect(formatEval({ kind: 'centiRabbits', value: 0 })).toBe('0.00');
    expect(formatEval({ kind: 'decided', winner: 'silver' })).toBe('Silver wins');
    expect(formatEval(null)).toBe('');
    expect(formatRabbits(-5)).toBe('−0.05');
  });

  it('fills the eval bar from gold’s end', () => {
    expect(goldShare(null)).toBe(0.5);
    expect(goldShare({ kind: 'centiRabbits', value: 0 })).toBe(0.5);
    expect(goldShare({ kind: 'centiRabbits', value: 100 })).toBeCloseTo(0.58, 2);
    expect(goldShare({ kind: 'centiRabbits', value: -100 })).toBeCloseTo(0.42, 2);
    expect(goldShare({ kind: 'decided', winner: 'gold' })).toBe(1);
    expect(goldShare({ kind: 'decided', winner: 'silver' })).toBe(0);
  });

  it('sends the PV up to the clicked turn', () => {
    expect(pvMoves(line(), 0)).toEqual(['Ee2n Ee3n']);
    expect(pvMoves(line(), 1)).toEqual(['Ee2n Ee3n', 'ee7s']);
  });

  it('shortens nodes per second', () => {
    expect(nodesPerSecond(line())).toBe('1.2M');
    expect(nodesPerSecond(line({ nodes: 5000, timeMs: 1000 }))).toBe('5k');
    expect(nodesPerSecond(line({ timeMs: 0 }))).toBeNull();
    expect(nodesPerSecond(line({ nodes: null }))).toBeNull();
  });

  it('picks the engine used last, or the first', () => {
    const e = (id: string): EngineSpec => ({ id, name: id, program: id, args: [], workingDir: null, options: [] });
    expect(analysisEngine([e('a'), e('b')], 'b')?.id).toBe('b');
    expect(analysisEngine([e('a'), e('b')], 'gone')?.id).toBe('a');
    expect(analysisEngine([], 'a')).toBeNull();
  });
});
