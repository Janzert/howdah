// Engine tests (the howdah-tourney daemon): how the lobby words a test's
// state and results.
import type { TestView } from './bindings/TestView';

/** A test's state in a word: active, paused, or how it ended. */
export function testState(t: TestView): string {
  if (t.status === 'finished') return t.outcome ?? 'finished';
  return t.status;
}

/** Where the LLR stands between the SPRT's bounds, from 0 (the lower) to
 * 1 (the upper); null without an SPRT. */
export function llrPosition(t: TestView): number | null {
  if (t.llr == null || t.llrBounds == null) return null;
  const [lo, hi] = t.llrBounds;
  return Math.min(1, Math.max(0, (t.llr - lo) / (hi - lo)));
}

/** The SPRT in brief: `LLR 1.23 (-2.94, 2.94) for [0, 10]`. */
export function sprtText(t: TestView): string | null {
  if (t.llr == null || t.llrBounds == null || t.sprt == null) return null;
  const [lo, hi] = t.llrBounds;
  return `LLR ${t.llr.toFixed(2)} (${lo.toFixed(2)}, ${hi.toFixed(2)}) for Elo [${t.sprt.elo0}, ${t.sprt.elo1}]`;
}

/** Games, pairs and the score: `12-8 in 20 games (10 pairs)`. */
export function scoreText(t: TestView): string {
  const pairs = t.pairs === 1 ? '1 pair' : `${t.pairs} pairs`;
  return `${t.wins[0]}–${t.wins[1]} in ${t.games} games (${pairs})`;
}

/** Fast time controls for engine tests, with a turn limit. */
export const TEST_TIME_CONTROLS: [string, string][] = [
  ['1s/10s/100/0/100t', '1 s a move'],
  ['2s/30s/100/0/100t', '2 s a move'],
  ['5s/1m/100/0/100t', '5 s a move'],
  ['15s/2m/100/0/100t', '15 s a move'],
];

/** A pair limit from a number field (empty: none). */
export function pairCount(value: number | string | null | undefined): number | null {
  if (value == null || value === '') return null;
  const n = Number(value);
  return Number.isFinite(n) ? n : null;
}

/** How a test's game ended, in words: the log's reason names, with
 * `resourceLimit` (a forfeit for going over a memory limit) spelled out. */
export function endingText(reason: string): string {
  const words: Record<string, string> = {
    illegalMove: 'illegal move',
    resourceLimit: 'a memory-limit forfeit',
  };
  return words[reason] ?? reason;
}
