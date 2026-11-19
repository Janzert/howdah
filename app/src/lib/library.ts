// Words and dates for the lobby's Games panel (the game database's list).
import type { LibraryFilter } from './bindings/LibraryFilter';
import type { LibraryGameView } from './bindings/LibraryGameView';
import type { Color } from './bindings/Color';
import type { PieceKind } from './bindings/PieceKind';

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
  /** Setups from the setup explorer (`setupPieces`' keys): gold's, and
   * silver's reply to it (or with no gold setup, silver's alone). */
  goldSetup: string;
  silverSetup: string;
  /** Only games reaching a position: its short format (`[` + 64 squares
   * + `]`, '' for none) and side to move; `pieces` matches its pieces on
   * their squares with anything else anywhere, either side to move. */
  position: string;
  positionSide: Color;
  positionMatch: 'exact' | 'pieces' | 'mirrored';
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
  goldSetup: '',
  silverSetup: '',
  position: '',
  positionSide: 'gold',
  positionMatch: 'exact',
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
    goldSetup: text(f.goldSetup ?? ''),
    silverSetup: text(f.silverSetup ?? ''),
    position: f.position ? { short: f.position, sideToMove: f.positionSide ?? 'gold', moveNumber: 2 } : null,
    positionMatch: f.position ? (f.positionMatch ?? 'exact') : null,
    exclude: null,
  };
}

const KINDS: Record<string, PieceKind> = {
  R: 'rabbit',
  C: 'cat',
  D: 'dog',
  H: 'horse',
  M: 'camel',
  E: 'elephant',
};
const FILES = 'abcdefgh';

/** A setup key's pieces: 16 letters (`RCDHME`, `.` empty), a1 … h1 then
 * a2 … h2, seen from gold's side. `rank` is the square's rank for
 * `color` (silver's 1 and 2 are its 8 and 7), `back` whether it's the
 * home row. */
export function setupPieces(
  key: string,
  color: Color,
): { file: number; back: boolean; rank: number; kind: PieceKind }[] {
  const out = [];
  for (let i = 0; i < 16 && i < key.length; i++) {
    const kind = KINDS[key[i]];
    if (!kind) continue;
    const back = i < 8;
    const rank = color === 'gold' ? (back ? 1 : 2) : back ? 8 : 7;
    out.push({ file: i % 8, back, rank, kind });
  }
  return out;
}

/** "elephant e2, camel d2, horses b2 g2, dogs c2 f2, cats d1 e1": the
 * pieces other than rabbits, strongest first. */
export function setupWords(key: string, color: Color): string {
  const order: PieceKind[] = ['elephant', 'camel', 'horse', 'dog', 'cat'];
  const pieces = setupPieces(key, color);
  return order
    .map((kind) => {
      const squares = pieces
        .filter((p) => p.kind === kind)
        .map((p) => `${FILES[p.file]}${p.rank}`)
        .sort();
      if (squares.length === 0) return null;
      return `${squares.length > 1 ? `${kind}s` : kind} ${squares.join(' ')}`;
    })
    .filter((s) => s)
    .join(', ');
}

/** "55%", or "–" with no games. */
export function percent(n: number, of: number): string {
  return of > 0 ? `${Math.round((100 * n) / of)}%` : '–';
}
