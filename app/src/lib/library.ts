// Words and dates for the lobby's Games panel (the game database's list).
import type { LibraryFilter } from './bindings/LibraryFilter';
import type { LibraryGameView } from './bindings/LibraryGameView';

/** The endings by arimaa.com's letters (`docs/RESULT-CODES.md`), in the
 * order the filter offers them. */
export const ENDINGS: [string, string][] = [
  ['g', 'goal'],
  ['e', 'elimination'],
  ['m', 'immobilization'],
  ['t', 'time'],
  ['r', 'resignation'],
  ['i', 'illegal move'],
  ['s', 'score'],
  ['f', 'forfeit'],
  ['a', 'abandoned'],
  ['p', 'repetition'],
  ['n', 'no rabbits left'],
];

const endingWord = (t: string | null) => ENDINGS.find(([l]) => l === t)?.[1] ?? null;

/** "Gold won by goal", "Draw (no rabbits left)", "No result" */
export function resultWords(g: Pick<LibraryGameView, 'winner' | 'termination'>): string {
  const how = endingWord(g.termination);
  switch (g.winner) {
    case 'g':
    case 's':
      return `${g.winner === 'g' ? 'Gold' : 'Silver'} won${how ? ` by ${how}` : ''}`;
    case 'd':
      return how ? `Draw (${how})` : 'Draw';
    case 'u':
      return how ? `Unknown result (${how})` : 'Unknown result';
    default:
      return 'No result';
  }
}

const join = (parts: (string | null | undefined | false)[]) => parts.filter((s) => s).join(' · ');

/** "Gold won by goal · 33 moves · 2014.01.04 · 2m/2m · rated · #93252" */
export function libraryMeta(g: LibraryGameView): string {
  const moves = g.headerOnly
    ? 'not fetched yet'
    : g.setPosition
      ? `${g.plies} ${g.plies === 1 ? 'move' : 'moves'} from a position`
      : g.plies > 0 && `${Math.ceil(g.plies / 2)} moves`;
  return join([
    g.winner || g.termination ? resultWords(g) : null,
    moves,
    g.date,
    g.timeControl,
    g.rated && 'rated',
    g.postal && 'postal',
    g.variations && 'with variations',
    g.arimaaId && `#${g.arimaaId}`,
  ]);
}

/** Unix seconds at the start of a `YYYY-MM-DD` day (UTC, as the
 * archive's dates are), the day after with `next`; null for no date. */
export function dayStart(date: string, next = false): number | null {
  const m = /^(\d{4})-(\d{2})-(\d{2})$/.exec(date.trim());
  if (!m) return null;
  const ms = Date.UTC(Number(m[1]), Number(m[2]) - 1, Number(m[3]) + (next ? 1 : 0));
  return Math.floor(ms / 1000);
}

/** The filter's form, as the panel keeps it. */
export interface FilterForm {
  player: string;
  opponent: string;
  side: '' | 'gold' | 'silver';
  outcome: '' | 'won' | 'lost';
  ending: string;
  from: string;
  to: string;
  origin: '' | 'arimaa' | 'local';
  rated: '' | 'rated' | 'unrated';
}

export const EMPTY_FORM: FilterForm = {
  player: '',
  opponent: '',
  side: '',
  outcome: '',
  ending: '',
  from: '',
  to: '',
  origin: '',
  rated: '',
};

/** The backend's filter from the form. The side and outcome only count
 * with a player. */
export function toFilter(f: FilterForm): LibraryFilter {
  const text = (s: string) => s.trim() || null;
  const player = text(f.player);
  return {
    player,
    opponent: text(f.opponent),
    side: player && f.side ? f.side : null,
    outcome: player && f.outcome ? f.outcome : null,
    termination: f.ending || null,
    from: dayStart(f.from),
    to: dayStart(f.to, true),
    origin: f.origin || null,
    rated: f.rated === '' ? null : f.rated === 'rated',
  };
}
