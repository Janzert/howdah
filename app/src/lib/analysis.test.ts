import { describe, expect, it } from 'vitest';
import {
  analysisEngine,
  evalGraph,
  formatEval,
  formatRabbits,
  goldShare,
  nodesPerSecond,
  pvMoves,
  shortEval,
} from './analysis';
import type { AnalysisLine } from './bindings/AnalysisLine';
import type { EngineSpec } from './bindings/EngineSpec';
import type { Eval } from './bindings/Eval';
import type { MoveNodeView } from './bindings/MoveNodeView';

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

describe('evalGraph', () => {
  const move = (id: number, ply: number, value: number | null, depth = 0) =>
    ({
      id,
      ply,
      depth,
      label: `${ply}g`,
      eval: value == null ? null : { kind: 'centiRabbits', value },
    }) as unknown as MoveNodeView;

  it('is empty without evals', () => {
    expect(evalGraph([move(1, 1, null)])).toEqual([]);
  });

  it('keeps the main line and spaces the moves by ply', () => {
    const tree = [move(1, 1, null), move(2, 3, 0), move(9, 4, 500, 1), move(3, 4, 100), move(4, 7, -100)];
    const graph = evalGraph(tree);
    expect(graph.map((p) => p.id)).toEqual([2, 3, 4]);
    expect(graph.map((p) => p.at)).toEqual([0, 0.25, 1]);
    expect(graph[0].share).toBe(0.5);
  });

  it('survives a single move', () => {
    expect(evalGraph([move(2, 3, 0)])[0].at).toBe(0);
  });
});

describe('shortEval', () => {
  it('shows a win as on the eval bar', () => {
    expect(shortEval({ kind: 'decided', winner: 'silver' } as Eval)).toBe('L');
    expect(shortEval({ kind: 'centiRabbits', value: 35 })).toBe('+0.35');
  });
});
