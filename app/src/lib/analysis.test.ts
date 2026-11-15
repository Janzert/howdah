import { describe, expect, it } from 'vitest';
import {
  analysisEngine,
  barEval,
  earlierEval,
  evalGraph,
  evalTitle,
  formatChance,
  formatEval,
  formatRabbits,
  goldShare,
  markTitle,
  nodesPerSecond,
  pvMoves,
  shortEval,
  sweepText,
} from './analysis';
import type { AnalysisLine } from './bindings/AnalysisLine';
import type { EngineSpec } from './bindings/EngineSpec';
import type { Eval } from './bindings/Eval';
import type { MoveNodeView } from './bindings/MoveNodeView';

const line = (over: Partial<AnalysisLine> = {}): AnalysisLine => ({
  node: 3,
  depth: '12',
  eval: { kind: 'centiRabbits', value: 35, chance: null },
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
    expect(formatEval({ kind: 'centiRabbits', value: 35, chance: null })).toBe('+0.35');
    expect(formatEval({ kind: 'centiRabbits', value: -120, chance: null })).toBe('−1.20');
    expect(formatEval({ kind: 'centiRabbits', value: 0, chance: null })).toBe('0.00');
    expect(formatEval({ kind: 'decided', winner: 'silver' })).toBe('Silver wins');
    expect(formatEval(null)).toBe('');
    expect(formatRabbits(-5)).toBe('−0.05');
  });

  it('reads a win chance as gold’s', () => {
    const e: Eval = { kind: 'centiRabbits', value: 166, chance: 637 };
    expect(formatEval(e)).toBe('Gold 64%');
    expect(shortEval(e)).toBe('64%');
    expect(goldShare(e)).toBe(0.637);
    expect(evalTitle(e)).toBe("Gold's chance of winning 64%; +1.66 in rabbits");
    expect(evalTitle({ kind: 'centiRabbits', value: -35, chance: null })).toBe("−0.35 rabbits, from gold's side");
    expect(formatChance(996)).toBe('99.6%');
    expect(formatChance(4)).toBe('0.4%');
    expect(formatChance(10)).toBe('1%');
    expect(formatChance(1000)).toBe('100.0%');
  });

  it('fills the eval bar from gold’s end', () => {
    expect(goldShare(null)).toBe(0.5);
    expect(goldShare({ kind: 'centiRabbits', value: 0, chance: null })).toBe(0.5);
    expect(goldShare({ kind: 'centiRabbits', value: 100, chance: null })).toBeCloseTo(0.58, 2);
    expect(goldShare({ kind: 'centiRabbits', value: -100, chance: null })).toBeCloseTo(0.42, 2);
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
      eval: value == null ? null : { kind: 'centiRabbits', value, chance: null },
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
    expect(shortEval({ kind: 'centiRabbits', value: 35, chance: null })).toBe('+0.35');
  });
});

describe('markTitle', () => {
  it('names the mark and the better move', () => {
    expect(markTitle({ glyph: '??', best: 'Ee2n', provisional: false })).toBe('Blunder, by the engine. Best was Ee2n');
    expect(markTitle({ glyph: '?!', best: null, provisional: true })).toBe(
      'Dubious, by the engine (provisional: more search may change it)',
    );
  });
});

describe('sweepText', () => {
  const view = { done: 40, total: 88, queued: 0, remainingMs: 190_400, spentMs: 750_000, settled: false, live: false };
  it('words each phase', () => {
    expect(sweepText({ ...view, phase: 'quick' })).toBe('Quick pass 40/88');
    expect(sweepText({ ...view, phase: 'refining' })).toBe('Refining, 3:10 left');
    expect(sweepText({ ...view, phase: 'following', live: true })).toBe('Following the game, 3:10 left');
  });
  it('counts up in a run until stopped', () => {
    const open = { ...view, remainingMs: null };
    expect(sweepText({ ...open, phase: 'refining' })).toBe('Refining, 12:30 so far');
    expect(sweepText({ ...open, phase: 'refining', settled: true })).toBe('Settled, deepening, 12:30 so far');
    expect(sweepText({ ...open, phase: 'following', live: true })).toBe('Following the game, 12:30 so far');
  });
});

describe('barEval', () => {
  const searched: Eval = { kind: 'centiRabbits', value: 0, chance: null };
  it('gives the winner the bar when the position ends the game', () => {
    for (const reason of ['goal', 'elimination', 'immobilization'] as const) {
      expect(barEval({ winner: 'silver', reason }, searched)).toEqual({ kind: 'decided', winner: 'silver' });
    }
    expect(barEval({ winner: 'gold', reason: 'goal' }, null)).toEqual({ kind: 'decided', winner: 'gold' });
  });
  it("keeps the search's eval for a result decided off the board", () => {
    for (const reason of ['timeout', 'resignation', 'illegalMove', 'score', 'forfeit'] as const) {
      expect(barEval({ winner: 'gold', reason }, searched)).toBe(searched);
    }
    expect(barEval(null, searched)).toBe(searched);
    expect(barEval(null, null)).toBeNull();
  });
});

describe('earlierEval', () => {
  const node = (id: number, parent: number, value: number | null) =>
    ({
      id,
      parent,
      eval: value == null ? null : { kind: 'centiRabbits', value, chance: null },
    }) as unknown as MoveNodeView;
  // 1 (+50) - 2 (none) - 3 (none); a variation 4 (+300) - 5 (none) from 2.
  const tree = [node(1, 0, 50), node(2, 1, null), node(3, 2, null), node(4, 2, 300), node(5, 4, null)];

  it('walks back along the line to a move with an eval', () => {
    expect(earlierEval(tree, 3)).toEqual({ kind: 'centiRabbits', value: 50, chance: null });
    expect(earlierEval(tree, 5)).toEqual({ kind: 'centiRabbits', value: 300, chance: null });
  });

  it("leaves out the node's own eval, and finds none before the first", () => {
    expect(earlierEval(tree, 4)).toMatchObject({ value: 50 });
    expect(earlierEval(tree, 1)).toBeNull();
    expect(earlierEval(tree, 0)).toBeNull();
  });
});
