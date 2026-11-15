// Reading analysis results for display: evals as text and as an eval-bar
// share, and the moves a PV click or hover sends.
import type { AnalysisLine } from './bindings/AnalysisLine';
import type { EngineSpec } from './bindings/EngineSpec';
import type { Eval } from './bindings/Eval';
import type { GameResult } from './bindings/GameResult';
import type { MoveMark } from './bindings/MoveMark';
import type { MoveNodeView } from './bindings/MoveNodeView';
import type { NodeId } from './bindings/NodeId';
import type { SweepView } from './bindings/SweepView';

/** Centi-rabbits giving gold about 73% of the eval bar (a logistic: a
 * rabbit up in the opening, about +100, is about 58%). An engine's win
 * chance goes on the bar as it is (`EVAL_BAR_SCALE` in `dto.rs`). */
export const EVAL_BAR_SCALE = 300;

/** Gold's share of the eval bar, 0..1: its chance of winning when the
 * engine gave one. */
export function goldShare(e: Eval | null): number {
  if (!e) return 0.5;
  if (e.kind === 'decided') return e.winner === 'gold' ? 1 : 0;
  if (e.chance != null) return e.chance / 1000;
  return 1 / (1 + Math.exp(-e.value / EVAL_BAR_SCALE));
}

/** The eval of the nearest move before `node` on its line that analysis
 * looked at, or null: where the eval bar fills from when the shown
 * position's first eval arrives. */
export function earlierEval(tree: MoveNodeView[], node: NodeId): Eval | null {
  const byId = new Map(tree.map((n) => [n.id, n]));
  let at = byId.get(node);
  while (at) {
    at = byId.get(at.parent);
    if (at?.eval) return at.eval;
  }
  return null;
}

/** The eval the bar shows at a node: a win for the winner when the
 * position itself ends the game (goal, elimination, immobilization),
 * since engines don't search a finished position; otherwise the search's
 * eval. A result decided off the board (timeout, resignation, ...) keeps
 * the search's eval of the position. */
export function barEval(result: GameResult | null, searched: Eval | null): Eval | null {
  if (result && (result.reason === 'goal' || result.reason === 'elimination' || result.reason === 'immobilization')) {
    return { kind: 'decided', winner: result.winner, turns: null };
  }
  return searched;
}

/** An eval from gold's side: its chance of winning when the engine gave
 * one (`Gold 63%`), otherwise rabbits (`+0.35`), or who has a proven win
 * and in how many of its turns (`Gold wins in 2`). */
export function formatEval(e: Eval | null): string {
  if (!e) return '';
  if (e.kind === 'decided') return `${e.winner === 'gold' ? 'Gold' : 'Silver'} wins${inTurns(e.turns)}`;
  if (e.chance != null) return `Gold ${formatChance(e.chance)}`;
  return formatRabbits(e.value);
}

/** An eval short enough for a move list row: gold's chance (`63%`), its
 * rabbits, or `W`/`L` for a win or loss for gold, as on the eval bar,
 * with the winner's turns to it when known (`W2`). */
export function shortEval(e: Eval): string {
  if (e.kind === 'decided') return `${e.winner === 'gold' ? 'W' : 'L'}${e.turns ?? ''}`;
  if (e.chance != null) return formatChance(e.chance);
  return formatRabbits(e.value);
}

/** An eval in full, for a tooltip. */
export function evalTitle(e: Eval): string {
  if (e.kind === 'decided') {
    const turns = e.turns == null ? '' : ` (in ${e.turns} of ${e.winner}'s turns)`;
    return `${e.winner === 'gold' ? 'Gold' : 'Silver'} wins${turns}, proven by the engine`;
  }
  if (e.chance != null)
    return `Gold's chance of winning ${formatChance(e.chance)}; ${formatRabbits(e.value)} in rabbits`;
  return `${formatRabbits(e.value)} rabbits, from gold's side`;
}

/** ` in 2` for a proven result's turns, or nothing when not known. */
export function inTurns(turns: number | null): string {
  return turns == null ? '' : ` in ${turns}`;
}

/** A chance in tenths of a percent: whole percents, with a tenth near the
 * ends (`63%`, `99.6%`, `0.4%`), where whole ones would round to certainty
 * (`format_chance` in `session.rs`). */
export function formatChance(tenths: number): string {
  if (tenths >= 10 && tenths <= 990) return `${Math.round(tenths / 10)}%`;
  return `${(tenths / 10).toFixed(1)}%`;
}

/** What a mark's glyph says, with the engine's choice if there is one. */
export function markTitle(mark: MoveMark): string {
  const name = mark.glyph === '??' ? 'Blunder' : mark.glyph === '?' ? 'Mistake' : 'Dubious';
  const text = mark.best ? `${name}, by the engine. Best was ${mark.best}` : `${name}, by the engine`;
  return mark.provisional ? `${text} (provisional: more search may change it)` : text;
}

/** Minutes and seconds, `3:05`. */
function minutes(ms: number): string {
  const s = Math.round(ms / 1000);
  return `${Math.floor(s / 60)}:${String(s % 60).padStart(2, '0')}`;
}

/** A run over the game's progress, in words: time left, or in a run
 * until stopped the time so far. */
export function sweepText(v: SweepView): string {
  const time = v.remainingMs === null ? `${minutes(v.spentMs)} so far` : `${minutes(v.remainingMs)} left`;
  switch (v.phase) {
    case 'quick':
      return `Quick pass ${v.done}/${v.total}`;
    case 'refining':
      return v.settled && v.remainingMs === null ? `Settled, deepening, ${time}` : `Refining, ${time}`;
    case 'following':
      return `Following the game, ${time}`;
  }
}

/** One move on the eval graph. */
export interface GraphPoint {
  id: NodeId;
  label: string;
  eval: Eval;
  /** Across the graph, 0..1. */
  at: number;
  /** Gold's share of the eval bar. */
  share: number;
}

/** The main line's moves with evals, spaced by ply over the moves that
 * have one. */
export function evalGraph(tree: MoveNodeView[]): GraphPoint[] {
  const known = tree.filter((m) => m.depth === 0 && m.eval != null);
  if (known.length === 0) return [];
  const first = known[0].ply;
  const span = Math.max(known[known.length - 1].ply - first, 1);
  return known.map((m) => ({
    id: m.id,
    label: m.label,
    eval: m.eval!,
    at: (m.ply - first) / span,
    share: goldShare(m.eval),
  }));
}

/** Centi-rabbits as signed rabbits (`+0.35`, `−1.20`). */
export function formatRabbits(centiRabbits: number): string {
  const rabbits = centiRabbits / 100;
  return `${rabbits > 0 ? '+' : rabbits < 0 ? '−' : ''}${Math.abs(rabbits).toFixed(2)}`;
}

/** The PV's moves in notation, up to and including turn `index`. */
export function pvMoves(line: AnalysisLine, index: number): string[] {
  return line.pv.slice(0, index + 1).map((t) => t.notation);
}

/** Nodes per second, shortened (`850k`, `1.2M`), if the engine reported both. */
export function nodesPerSecond(line: AnalysisLine): string | null {
  if (line.nodes == null || !line.timeMs) return null;
  const nps = (line.nodes * 1000) / line.timeMs;
  if (nps >= 1e6) return `${(nps / 1e6).toFixed(1)}M`;
  if (nps >= 1e3) return `${Math.round(nps / 1e3)}k`;
  return `${Math.round(nps)}`;
}

/** The engine to analyse with: the one used last, if it's still there,
 * otherwise the first. */
export function analysisEngine(engines: EngineSpec[], last: string | null): EngineSpec | null {
  return engines.find((e) => e.id === last) ?? engines[0] ?? null;
}
