// Reading analysis results for display: evals as text and as an eval-bar
// share, and the moves a PV click or hover sends.
import type { AnalysisLine } from './bindings/AnalysisLine';
import type { EngineSpec } from './bindings/EngineSpec';
import type { Eval } from './bindings/Eval';

/** Centi-rabbits giving gold about 73% of the eval bar (a logistic: a
 * rabbit up in the opening, about +100, is about 58%). */
export const EVAL_BAR_SCALE = 300;

/** Gold's share of the eval bar, 0..1. */
export function goldShare(e: Eval | null): number {
  if (!e) return 0.5;
  if (e.kind === 'decided') return e.winner === 'gold' ? 1 : 0;
  return 1 / (1 + Math.exp(-e.value / EVAL_BAR_SCALE));
}

/** An eval in rabbits from gold's side (`+0.35`), or who has a proven win. */
export function formatEval(e: Eval | null): string {
  if (!e) return '';
  if (e.kind === 'decided') return e.winner === 'gold' ? 'Gold wins' : 'Silver wins';
  const rabbits = e.value / 100;
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
