// Typed wrappers for the Tauri commands in src-tauri/src/commands.rs.
// Mutating commands resolve with nothing; the new state arrives as a
// `game://changed` event (see events.ts).
import { invoke } from '@tauri-apps/api/core';
import type { ApiError } from './bindings/ApiError';
import type { EngineIdentity } from './bindings/EngineIdentity';
import type { EngineSpec } from './bindings/EngineSpec';
import type { MatchSpec } from './bindings/MatchSpec';
import type { MoveReplay } from './bindings/MoveReplay';
import type { SessionView } from './bindings/SessionView';
import type { Square } from './bindings/Square';
import type { StepTarget } from './bindings/StepTarget';

export const api = {
  getState: () => invoke<SessionView>('get_state'),
  newGame: () => invoke<void>('new_game'),
  loadGame: (record: string) => invoke<void>('load_game', { record }),
  exportGame: () => invoke<string>('export_game'),
  gotoPly: (ply: number) => invoke<void>('goto_ply', { ply }),
  /** The shown move's starting pieces and animation, or null after a setup. */
  moveReplay: () => invoke<MoveReplay | null>('move_replay'),
  legalTargets: (from: Square) => invoke<StepTarget[]>('legal_targets', { from }),
  tryStep: (from: Square, to: Square) => invoke<void>('try_step', { from, to }),
  /** Squares a drop would walk the piece through, or null if it can't get there. */
  planRoute: (from: Square, to: Square, path: Square[]) =>
    invoke<Square[] | null>('plan_route', { from, to, path }),
  tryRoute: (from: Square, to: Square, path: Square[]) => invoke<void>('try_route', { from, to, path }),
  undoStep: () => invoke<void>('undo_step'),
  cancelTurn: () => invoke<void>('cancel_turn'),
  commitTurn: () => invoke<void>('commit_turn'),
  setupSwap: (a: Square, b: Square) => invoke<void>('setup_swap', { a, b }),
  commitSetup: () => invoke<void>('commit_setup'),
  startMatch: (spec: MatchSpec) => invoke<void>('start_match', { spec }),
  endMatch: () => invoke<void>('end_match'),
  engineMoveNow: () => invoke<void>('engine_move_now'),
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
