// Dev builds only: `window.__arimaa`, for driving and inspecting the UI from
// scripts (the browser pane's JavaScript tool, Playwright) without
// screenshots or pixel coordinates. Input goes through the board's own
// pointer handlers, so it exercises the same code as a real mouse.
import { api } from './api';
import type { AnalysisView } from './bindings/AnalysisView';
import type { SessionView } from './bindings/SessionView';
import type { Square } from './bindings/Square';
import type { BoardModel } from './board/boardModel.svelte';
import { on } from './events';
import { squareName, TRAPS } from './geometry';

interface AppParts {
  state: () => SessionView | null;
  message: () => string | null;
  analysis: () => AnalysisView | null;
  model: BoardModel;
}

interface BoardParts {
  svg: SVGSVGElement;
  /** Waiting on the backend for something it will draw (hover arrows). */
  busy: () => boolean;
  /** Client (CSS pixel) coordinates of a square's center. */
  clientPoint: (sq: Square) => { x: number; y: number };
}

/** The position editor, while it's open. */
export interface EditorState {
  /** The board in the short format with the side to move, `g [...]`. */
  short: string;
  label: string;
  problems: string[];
  /** The selected palette tool: `pointer`, `delete` or a piece letter. */
  tool: string;
}

let app: AppParts | null = null;
let editor: (() => EditorState) | null = null;
let board: BoardParts | null = null;
let lastEvent = 0;

const sleep = (ms: number) => new Promise((r) => setTimeout(r, ms));
// A frame, or a short wait when the page is hidden and frames don't run.
const frame = () => Promise.race([new Promise((r) => requestAnimationFrame(() => r(undefined))), sleep(50)]);

function parseSquare(name: string): Square {
  const m = /^([a-h])([1-8])$/.exec(name.trim().toLowerCase());
  if (!m) throw new Error(`not a square: ${JSON.stringify(name)}`);
  return (Number(m[2]) - 1) * 8 + 'abcdefgh'.indexOf(m[1]);
}

const LETTERS = { rabbit: 'r', cat: 'c', dog: 'd', horse: 'h', camel: 'm', elephant: 'e' } as const;

function need<T>(part: T | null, what: string): T {
  if (!part) throw new Error(`__arimaa: the ${what} isn't mounted yet`);
  return part;
}

/** Sends a pointer event at a square's center, or 35% of the way toward `toward`. */
function pointer(type: string, sq: Square, buttons: number, toward?: Square, button = 0) {
  const b = need(board, 'board');
  let p = b.clientPoint(sq);
  if (toward != null) {
    const t = b.clientPoint(toward);
    p = { x: p.x + 0.35 * (t.x - p.x), y: p.y + 0.35 * (t.y - p.y) };
  }
  b.svg.dispatchEvent(
    new PointerEvent(type, {
      bubbles: true,
      cancelable: true,
      pointerId: 1,
      pointerType: 'mouse',
      isPrimary: true,
      button,
      buttons,
      clientX: p.x,
      clientY: p.y,
    }),
  );
}

/** Resolves once no animation is playing, the board isn't waiting on the
 * backend, and no update has arrived for a short while. */
async function idle(quietMs = 150, timeoutMs = 10_000): Promise<void> {
  const parts = need(app, 'app');
  const start = performance.now();
  await frame();
  await frame();
  while (parts.model.animating || board?.busy() || performance.now() - lastEvent < quietMs) {
    if (performance.now() - start > timeoutMs) throw new Error('__arimaa.idle: timed out');
    await sleep(25);
  }
}

const hooks = {
  /** The typed command wrappers, as the UI uses them. */
  api,
  /** The latest `SessionView` the UI received, as a plain copy (the
   * reactive proxy doesn't serialize in some tools). */
  state: (): SessionView | null => JSON.parse(JSON.stringify(need(app, 'app').state())),
  /** The latest analysis update (`analysis://update`), as a plain copy. */
  analysis: (): AnalysisView | null => JSON.parse(JSON.stringify(need(app, 'app').analysis())),
  /** The error or notice the UI is currently showing, if any. */
  message: (): string | null => need(app, 'app').message(),
  /** The board as displayed (mid-animation included), as text: gold upper
   * case, silver lower case, `x` an empty trap, rank 8 at the top. */
  board(): string {
    const parts = need(app, 'app');
    const cells: string[] = Array.from({ length: 64 }, (_, sq) => (TRAPS.includes(sq) ? 'x' : '.'));
    for (const p of parts.model.pieces) {
      if (p.fading === 'out') continue;
      const l = LETTERS[p.piece.kind];
      cells[p.square] = p.piece.color === 'gold' ? l.toUpperCase() : l;
    }
    const rows = [' +-----------------+'];
    for (let r = 7; r >= 0; r--) rows.push(`${r + 1}| ${cells.slice(r * 8, r * 8 + 8).join(' ')} |`);
    rows.push(' +-----------------+', '   a b c d e f g h');
    return rows.join('\n');
  },
  idle,
  /** Drags the piece on `from` to `to`, passing over `via` squares on the
   * way (which steers the route), then waits for the board to settle. */
  async drag(from: string, to: string, via: string[] = []): Promise<void> {
    const squares = [from, ...via, to].map(parseSquare);
    pointer('pointerdown', squares[0], 1);
    await frame();
    for (const sq of squares.slice(1)) {
      pointer('pointermove', sq, 1);
      await frame();
    }
    pointer('pointerup', squares[squares.length - 1], 0);
    await idle();
  },
  /** Moves the mouse (no button) over a square, as for hover arrows. For
   * step mode, `toward` leans the pointer toward a neighbouring square. */
  async hover(square: string, toward?: string): Promise<void> {
    pointer('pointermove', parseSquare(square), 0, toward == null ? undefined : parseSquare(toward));
    await idle();
  },
  /** Squares the hover arrows currently point to. */
  hoverTargets: (): string[] =>
    [...document.querySelectorAll('[data-hover-target]')].map((e) => e.getAttribute('data-hover-target')!),
  /** Presses and releases the left button on a square (leaning toward
   * `toward`, as `hover` does). */
  async click(square: string, toward?: string): Promise<void> {
    const sq = parseSquare(square);
    const t = toward == null ? undefined : parseSquare(toward);
    // A real mouse arrives before it clicks; hover input depends on that.
    pointer('pointermove', sq, 0, t);
    await frame();
    pointer('pointerdown', sq, 1, t);
    await frame();
    pointer('pointerup', sq, 0, t);
    await idle();
  },
  /** Presses and releases the right button on a square (the position
   * editor takes a piece off with it). */
  async rightClick(square: string): Promise<void> {
    const sq = parseSquare(square);
    pointer('pointerdown', sq, 2, undefined, 2);
    await frame();
    pointer('pointerup', sq, 0, undefined, 2);
    await idle();
  },
  /** The position editor's state, while it's open. */
  editor: (): EditorState => JSON.parse(JSON.stringify(need(editor, 'position editor')())),
  /** Client coordinates of a square's center, for tools that click by position. */
  squareCenter: (square: string) => need(board, 'board').clientPoint(parseSquare(square)),
  squareName,
};

export type ArimaaHooks = typeof hooks;

declare global {
  interface Window {
    __arimaa?: ArimaaHooks;
  }
}

export function registerApp(parts: AppParts): void {
  app = parts;
  if (!window.__arimaa) {
    window.__arimaa = hooks;
    void on('game://changed', () => (lastEvent = performance.now()));
  }
}

export function registerEditor(state: (() => EditorState) | null): void {
  editor = state;
}

export function registerBoard(parts: BoardParts | null): void {
  board = parts;
}
