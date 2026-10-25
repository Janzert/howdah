// The board package declares its own copies of the types it takes; this
// fails svelte-check if they drift from the bindings generated from Rust.
import type * as Board from '@howdah/board/types';
import type { AnimPiece } from './bindings/AnimPiece';
import type { AnimStep } from './bindings/AnimStep';
import type { ClockView } from './bindings/ClockView';
import type { Color } from './bindings/Color';
import type { LastMoveView } from './bindings/LastMoveView';
import type { LastStepView } from './bindings/LastStepView';
import type { Piece } from './bindings/Piece';
import type { PieceAt } from './bindings/PieceAt';
import type { PieceKind } from './bindings/PieceKind';
import type { PieceView } from './bindings/PieceView';
import type { PositionView } from './bindings/PositionView';
import type { SideClockView } from './bindings/SideClockView';
import type { Square } from './bindings/Square';
import type { StepKind } from './bindings/StepKind';
import type { StepTarget } from './bindings/StepTarget';

type Same<A, B> = [A] extends [B] ? ([B] extends [A] ? true : false) : false;

export const sameTypes: [
  Same<Board.AnimPiece, AnimPiece>,
  Same<Board.AnimStep, AnimStep>,
  Same<Board.ClockView, ClockView>,
  Same<Board.Color, Color>,
  Same<Board.LastMoveView, LastMoveView>,
  Same<Board.LastStepView, LastStepView>,
  Same<Board.Piece, Piece>,
  Same<Board.PieceAt, PieceAt>,
  Same<Board.PieceKind, PieceKind>,
  Same<Board.PieceView, PieceView>,
  Same<Board.PositionView, PositionView>,
  Same<Board.SideClockView, SideClockView>,
  Same<Board.Square, Square>,
  Same<Board.StepKind, StepKind>,
  Same<Board.StepTarget, StepTarget>,
] = [true, true, true, true, true, true, true, true, true, true, true, true, true, true, true];
