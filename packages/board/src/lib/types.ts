// The data the board components take. They match the app's bindings
// generated from Rust (`app/src/lib/bindings/`, by ts-rs), which the app
// checks stay the same (`app/src/lib/boardTypes.check.ts`), so a backend's
// views pass straight in; another user (the Maqha site) builds them itself.

/** A square, 0 (a1) to 63 (h8). */
export type Square = number;

export type Color = 'gold' | 'silver';

/** Weakest to strongest. */
export type PieceKind = 'rabbit' | 'cat' | 'dog' | 'horse' | 'camel' | 'elephant';

export type Piece = { color: Color; kind: PieceKind };

/** A piece on the board, with an id that follows it through its steps. */
export type PieceView = { id: number; piece: Piece; square: Square; frozen: boolean };

/** A piece appearing or disappearing during an animation. */
export type AnimPiece = { id: number; piece: Piece; square: Square };

/** One step of an animation. Play in order: fade in `restored`, slide `id`
 * from `from` to `to`, then fade out `captured`. `mover` is the side whose
 * turn it is (a push moves the other side's piece). */
export type AnimStep = {
  id: number;
  from: Square;
  to: Square;
  captured: AnimPiece | null;
  restored: AnimPiece | null;
  mover: Color;
};

/** How a step fits into the turn. */
export type StepKind = 'simple' | 'pushStart' | 'pushFinish' | 'pullFinish';

/** Where a piece can go: `kind` is null for setup swaps; `steps` is more
 * than 1 for a route of several steps. */
export type StepTarget = { to: Square; kind: StepKind | null; steps: number };

export type PieceAt = { piece: Piece; square: Square };

/** One step of the move that produced the shown position; `piece` is an
 * enemy of the mover when pushed or pulled. */
export type LastStepView = { piece: Piece; from: Square; to: Square; captured: PieceAt | null };

/** The move that produced the shown position, for the last-move display. */
export type LastMoveView = { color: Color; steps: Array<LastStepView> };

export type PositionView = { sideToMove: Color; pieces: Array<PieceView>; short: string };

/** One side's clock. `turnAllowanceMs` is the time it has for a turn from
 * its start; `lastUsedMs`, in a clock for a past move, the time that move
 * took. */
export type SideClockView = {
  timeControl: string;
  moveTimeMs: number;
  reserveMs: number;
  turnAllowanceMs: number;
  lastUsedMs: number | null;
};

/** A clock snapshot; the running side counts down from `turnElapsedMs`.
 * `past` marks clocks worked out for a past move. */
export type ClockView = {
  gold: SideClockView | null;
  silver: SideClockView | null;
  running: Color | null;
  turnElapsedMs: number;
  turnAllowanceMs: number;
  gameRemainingMs: number | null;
  past: boolean;
};

/** Board coordinates: none, the traps' names, or every square's. */
export type Coordinates = 'none' | 'traps' | 'all';

/** Input from pointer movement without a button: none, arrows for a hovered
 * piece's legal steps, or step mode (the step toward the pointer, by click). */
export type HoverInput = 'off' | 'arrows' | 'step';
