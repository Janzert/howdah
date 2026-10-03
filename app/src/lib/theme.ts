// Board/piece themes. A theme is a `*.theme.json` manifest in
// src/themes/<dir>/ plus its image files; asset paths in the manifest are
// relative to the manifest. Adding a theme needs no code changes.
import type { Piece } from './bindings/Piece';

export interface GridRect { x: number; y: number; width: number; height: number }

export type BoardSpec =
  | { kind: 'procedural'; light: string; dark: string; trap: string; line?: string }
  /** `grid` locates the 8×8 playing area inside the image, in image pixels. */
  | { kind: 'image'; src: string; width: number; height: number; grid: GridRect };

export interface GlyphColors { fill: string; stroke: string; text: string }

export type PieceSpec =
  | { kind: 'procedural'; gold: GlyphColors; silver: GlyphColors }
  /** `srcs` is keyed by piece letter (E M H D C R gold, e m h d c r silver). */
  | { kind: 'image'; srcs: Record<string, string>; scale?: number };

export interface ThemeUi {
  highlight: string;
  arrow: string;
  target: string;
  coord: string;
  pushPending: string;
  /** Last-move paths; defaults in `LAST_MOVE_COLORS`. */
  lastMove?: string;
  /** Last-move paths of pushed or pulled enemy pieces. */
  lastMoveDisplaced?: string;
}

export const LAST_MOVE_COLORS = { lastMove: 'rgba(255, 205, 40, 0.6)', lastMoveDisplaced: 'rgba(235, 90, 40, 0.75)' };

/** The analysis engine's next move, drawn like the last move. */
export const PV_MOVE_COLORS = { move: 'rgba(40, 125, 225, 0.65)', displaced: 'rgba(150, 70, 215, 0.75)' };

export interface Theme {
  id: string;
  name: string;
  attribution: string | null;
  board: BoardSpec;
  pieces: PieceSpec;
  ui: ThemeUi;
}

const manifests = import.meta.glob<Theme>('../themes/*/*.theme.json', { eager: true, import: 'default' });
const assetUrls = import.meta.glob<string>('../themes/**/*.{png,jpg,jpeg,svg,webp}', {
  eager: true,
  query: '?url',
  import: 'default',
});

function resolveAsset(manifestPath: string, rel: string): string {
  const dir = manifestPath.slice(0, manifestPath.lastIndexOf('/') + 1);
  const url = assetUrls[dir + rel];
  if (!url) console.warn(`theme asset not found: ${dir + rel}`);
  return url ?? '';
}

function load(path: string, raw: Theme): Theme {
  const t: Theme = structuredClone(raw);
  if (t.board.kind === 'image') t.board.src = resolveAsset(path, t.board.src);
  if (t.pieces.kind === 'image') {
    for (const k of Object.keys(t.pieces.srcs)) t.pieces.srcs[k] = resolveAsset(path, t.pieces.srcs[k]);
  }
  return t;
}

export const themes: Theme[] = Object.entries(manifests)
  .map(([path, raw]) => load(path, raw))
  .sort((a, b) => a.name.localeCompare(b.name));

export const DEFAULT_THEME = 'classic-stone';
export const FALLBACK_THEME = 'placeholder';

export function findTheme(id: string | null): Theme {
  return (
    themes.find((t) => t.id === id) ??
    themes.find((t) => t.id === DEFAULT_THEME) ??
    themes.find((t) => t.id === FALLBACK_THEME)!
  );
}

export function pieceLetter(p: Piece): string {
  const l = { elephant: 'E', camel: 'M', horse: 'H', dog: 'D', cat: 'C', rabbit: 'R' }[p.kind];
  return p.color === 'gold' ? l : l.toLowerCase();
}

/** SVG viewBox showing the whole board image, with the playing grid at 0..800. */
export function viewBox(t: Theme): string {
  if (t.board.kind !== 'image') return '0 0 800 800';
  const { width, height, grid } = t.board;
  const sx = 800 / grid.width;
  const sy = 800 / grid.height;
  return `${-grid.x * sx} ${-grid.y * sy} ${width * sx} ${height * sy}`;
}
