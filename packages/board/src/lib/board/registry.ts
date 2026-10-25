// A mounted board's parts, for an app's scripted hooks (Howdah's dev
// `window.__arimaa`) to drive it by squares. Nothing listens unless an app
// sets a listener.
import type { Square } from '../types';

export interface BoardParts {
  svg: SVGSVGElement;
  /** Waiting on the backend for something it will draw (hover arrows). */
  busy: () => boolean;
  /** Client (CSS pixel) coordinates of a square's center. */
  clientPoint: (sq: Square) => { x: number; y: number };
}

let listener: ((parts: BoardParts | null) => void) | null = null;

/** Hears of each board mounted (and `null` when it goes). */
export function onBoardRegistered(fn: ((parts: BoardParts | null) => void) | null): void {
  listener = fn;
}

/** Called by the board as it mounts and unmounts. */
export function registerBoard(parts: BoardParts | null): void {
  listener?.(parts);
}
