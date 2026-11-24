// Ordering a player's record by opponent by any of its columns.
import type { RecordLineView } from './bindings/RecordLineView';
import type { TallyView } from './bindings/TallyView';

export type RecordKey = 'opponent' | 'rating' | 'games' | 'wl' | 'score' | 'gold' | 'silver';

export interface RecordSort {
  key: RecordKey;
  /** Highest first (names Z–A). */
  descending: boolean;
}

/** The server's order: most games first. */
export const DEFAULT_SORT: RecordSort = { key: 'games', descending: true };

/** A share of the points, draws counting half; none without games. */
export const score = (t: TallyView): number | null => (t.games ? (t.wins + t.draws / 2) / t.games : null);

/** The values a key compares, most significant first; null sorts last
 * whichever way. */
function values(r: RecordLineView, key: RecordKey): (number | null)[] {
  switch (key) {
    case 'rating':
      return [r.rating];
    case 'games':
      return [r.all.games];
    case 'wl':
      return [r.all.wins, -r.all.losses];
    case 'score':
      return [score(r.all), r.all.games];
    case 'gold':
      return [r.gold.games, r.gold.wins];
    case 'silver':
      return [r.silver.games, r.silver.wins];
    case 'opponent':
      return [];
  }
}

const byName = (a: RecordLineView, b: RecordLineView) =>
  (a.opponent ?? '').localeCompare(b.opponent ?? '', undefined, { sensitivity: 'base' });

/** The rows in the order asked for; ties keep the order they came in. */
export function sortRecord(rows: RecordLineView[], sort: RecordSort): RecordLineView[] {
  const sign = sort.descending ? -1 : 1;
  const compare = (a: RecordLineView, b: RecordLineView): number => {
    if (sort.key === 'opponent') return sign * byName(a, b);
    const va = values(a, sort.key);
    const vb = values(b, sort.key);
    for (let i = 0; i < va.length; i++) {
      const x = va[i];
      const y = vb[i];
      if (x === y) continue;
      if (x === null) return 1;
      if (y === null) return -1;
      return sign * (x - y);
    }
    return 0;
  };
  return [...rows].sort(compare);
}

/** The sort after clicking a column's header: the same column turns
 * around; another starts with names A–Z and numbers highest first. */
export function nextSort(current: RecordSort, key: RecordKey): RecordSort {
  if (current.key === key) return { key, descending: !current.descending };
  return { key, descending: key !== 'opponent' };
}
