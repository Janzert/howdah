// Typed wrappers for the Tauri commands in src-tauri/src/commands.rs.
// Mutating commands resolve with nothing; the new state arrives as a
// `game://changed` event (see events.ts).
import { invoke } from '@tauri-apps/api/core';
import type { ApiError } from './bindings/ApiError';
import type { EngineIdentity } from './bindings/EngineIdentity';
import type { EngineSpec } from './bindings/EngineSpec';
import type { MatchSpec } from './bindings/MatchSpec';
import type { MoveReplay } from './bindings/MoveReplay';
import type { NodeId } from './bindings/NodeId';
import type { PositionView } from './bindings/PositionView';
import type { SessionId } from './bindings/SessionId';
import type { SessionView } from './bindings/SessionView';
import type { Square } from './bindings/Square';
import type { StepTarget } from './bindings/StepTarget';

/** The main window's session; `MAIN_SESSION` in backend.rs. */
export const MAIN_SESSION = 1;

/** The session this window shows: `?session=<id>` in its URL, otherwise
 * the main one. Every session command passes it. */
export const session: SessionId = Number(new URLSearchParams(location.search).get('session') ?? MAIN_SESSION);

export const api = {
  /** Opens a session with an empty game, for another window. */
  openSession: () => invoke<SessionId>('open_session'),
  closeSession: (id: SessionId) => invoke<void>('close_session', { session: id }),
  listSessions: () => invoke<SessionId[]>('list_sessions'),
  getState: () => invoke<SessionView>('get_state', { session }),
  newGame: () => invoke<void>('new_game', { session }),
  loadGame: (record: string) => invoke<void>('load_game', { session, record }),
  /** The game as a record: in full, or only its main line as a plain record. */
  exportGame: (mainLineOnly = false) => invoke<string>('export_game', { session, mainLineOnly }),
  gotoPly: (ply: number) => invoke<void>('goto_ply', { session, ply }),
  /** Shows the live position of a match (the end of the line in free play). */
  gotoLive: () => invoke<void>('goto_live', { session }),
  /** Shows any move of the game tree, switching to its line if needed. */
  gotoNode: (node: NodeId) => invoke<void>('goto_node', { session, node }),
  /** Shows the previous (-1) or next (1) alternative to the shown move. */
  gotoSibling: (offset: number) => invoke<void>('goto_sibling', { session, offset }),
  /** Shows the next or previous move with alternatives on the shown line. */
  gotoBranch: (forward: boolean) => invoke<void>('goto_branch', { session, forward }),
  /** Sets the comment after a move (the game comment for the root); empty removes it. */
  setComment: (node: NodeId, text: string) => invoke<void>('set_comment', { session, node, text }),
  /** Adds or removes a glyph, numbered as in PGN (1 = `!`). */
  toggleGlyph: (node: NodeId, glyph: number) => invoke<void>('toggle_glyph', { session, node, glyph }),
  /** Folds a variation to its first move, or unfolds it. */
  toggleCollapsed: (node: NodeId) => invoke<void>('toggle_collapsed', { session, node }),
  promote: (node: NodeId) => invoke<void>('promote', { session, node }),
  demote: (node: NodeId) => invoke<void>('demote', { session, node }),
  makeMainLine: (node: NodeId) => invoke<void>('make_main_line', { session, node }),
  /** Deletes the move and everything after it. */
  deleteFrom: (node: NodeId) => invoke<void>('delete_from', { session, node }),
  /** The shown move's starting pieces and animation, or null after a setup. */
  moveReplay: () => invoke<MoveReplay | null>('move_replay', { session }),
  legalTargets: (from: Square) => invoke<StepTarget[]>('legal_targets', { session, from }),
  tryStep: (from: Square, to: Square) => invoke<void>('try_step', { session, from, to }),
  /** Squares a drop would walk the piece through, or null if it can't get there. */
  planRoute: (from: Square, to: Square, path: Square[]) =>
    invoke<Square[] | null>('plan_route', { session, from, to, path }),
  tryRoute: (from: Square, to: Square, path: Square[]) =>
    invoke<void>('try_route', { session, from, to, path }),
  undoStep: () => invoke<void>('undo_step', { session }),
  cancelTurn: () => invoke<void>('cancel_turn', { session }),
  /** Ends the turn; `plan` keeps a turn on your move in a match as a plan
   * instead of playing it. With no turn, plays the shown plan's move. */
  commitTurn: (plan = false) => invoke<void>('commit_turn', { session, plan }),
  /** Takes back played moves, back to the last human move. */
  takeBack: () => invoke<void>('take_back', { session }),
  setContinueTurns: (on: boolean) => invoke<void>('set_continue_turns', { session, on }),
  setupSwap: (a: Square, b: Square) => invoke<void>('setup_swap', { session, a, b }),
  commitSetup: () => invoke<void>('commit_setup', { session }),
  startMatch: (spec: MatchSpec) => invoke<void>('start_match', { session, spec }),
  endMatch: () => invoke<void>('end_match', { session }),
  engineMoveNow: () => invoke<void>('engine_move_now', { session }),
  /** Turns analysis on with an engine, or off with null. Updates arrive as
   * `analysis://update` events. */
  setAnalysis: (engineId: string | null) => invoke<void>('set_analysis', { session, engineId }),
  /** Adds moves (in notation) as a line from a node and shows its end. */
  addLine: (from: NodeId, moves: string[]) => invoke<void>('add_line', { session, from, moves }),
  /** The position after playing moves from a node, without changing anything. */
  previewLine: (from: NodeId, moves: string[]) =>
    invoke<PositionView>('preview_line', { session, from, moves }),
  listEngines: () => invoke<EngineSpec[]>('list_engines'),
  saveEngine: (spec: EngineSpec) => invoke<EngineSpec>('save_engine', { spec }),
  deleteEngine: (id: string) => invoke<void>('delete_engine', { id }),
  testEngine: (spec: EngineSpec) => invoke<EngineIdentity>('test_engine', { spec }),
};

export function isApiError(e: unknown): e is ApiError {
  return typeof e === 'object' && e !== null && 'kind' in e && 'message' in e;
}

export function errorMessage(e: unknown): string {
  if (isApiError(e)) return e.line != null ? `Line ${e.line}: ${e.message}` : e.message;
  return String(e);
}
