// The lobby's list of open game windows: what it shows about each, from
// the session's view (`OpenGames`).
import type { SessionId } from './bindings/SessionId';
import type { SessionView } from './bindings/SessionView';
import type { WatchView } from './bindings/WatchView';
import { formatClock, sideTimes } from './clock';
import type { BoardLoading } from './boardLoading';
import { boardTitle, testGameLabel, waitsOnUser } from './windowList';

export interface ListedWindow {
  id: SessionId;
  view: SessionView;
  title: string;
  /** "Against an engine", "Watching on arimaa.com", … */
  kind: string;
  yourMove: boolean;
  /** Each side's move time and reserve, as the player bars show them. */
  clocks: { gold: string | null; silver: string | null } | null;
  /** "Gold won", once the game has a result. */
  result: string | null;
}

function kind(v: SessionView, watch: WatchView | undefined): string {
  if (watch)
    return watch.side != null
      ? watch.postal
        ? 'Postal game on arimaa.com'
        : 'Playing on arimaa.com'
      : 'Watching on arimaa.com';
  if (v.testGame) return `Engine test ${testGameLabel(v)}`;
  if (v.puzzle) return 'Puzzle';
  if (v.players) {
    const engine = v.players.gold.kind === 'engine' || v.players.silver.kind === 'engine';
    return engine ? 'Against an engine' : 'Game';
  }
  if (v.analysisEngine) return 'Analysis';
  return v.moves.length > 0 ? `${v.moves.length} moves` : 'Empty board';
}

/** One board's entry; `now - at` is how long ago its view arrived, and
 * `loading` what it waits for (`boardLoading.ts`), if anything. */
export function listWindow(
  id: SessionId,
  entry: { view: SessionView; at: number },
  watch: WatchView | undefined,
  now: number,
  loading: BoardLoading | null = null,
): ListedWindow {
  const v = entry.view;
  const clock = (side: 'gold' | 'silver') => {
    const times = v.clock && sideTimes(v.clock, side, now - entry.at);
    return times && `${formatClock(times.move)} · ${formatClock(times.reserve)}`;
  };
  return {
    id,
    view: v,
    title: loading ? (loading.error ? "Couldn't open the game" : 'Loading…') : boardTitle(v),
    kind: loading ? (loading.error ?? loading.text) : kind(v, watch),
    yourMove: waitsOnUser(v),
    clocks: v.clock ? { gold: clock('gold'), silver: clock('silver') } : null,
    result: v.result ? `${v.result.winner === 'gold' ? 'Gold' : 'Silver'} won` : null,
  };
}
