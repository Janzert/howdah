// Typed wrappers for the Tauri commands in src-tauri/src/commands.rs.
// Mutating commands resolve with nothing; the new state arrives as a
// `game://changed` event (see events.ts).
import { invoke } from '@tauri-apps/api/core';
import type { ApiError } from './bindings/ApiError';
import type { Color } from './bindings/Color';
import type { EngineIdentity } from './bindings/EngineIdentity';
import type { EngineSpec } from './bindings/EngineSpec';
import type { GameroomGames } from './bindings/GameroomGames';
import type { GameroomStatus } from './bindings/GameroomStatus';
import type { MatchSpec } from './bindings/MatchSpec';
import type { MoveReplay } from './bindings/MoveReplay';
import type { NodeId } from './bindings/NodeId';
import type { PlayerGamesView } from './bindings/PlayerGamesView';
import type { PlayerMatchView } from './bindings/PlayerMatchView';
import type { PostalGameView } from './bindings/PostalGameView';
import type { PositionView } from './bindings/PositionView';
import type { SessionId } from './bindings/SessionId';
import type { SessionView } from './bindings/SessionView';
import type { Square } from './bindings/Square';
import type { StepTarget } from './bindings/StepTarget';
import type { WatchView } from './bindings/WatchView';

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
  /** In a game on arimaa.com, asks the opponent to take back your last move. */
  requestTakeback: () => invoke<void>('request_takeback', { session }),
  /** Accepts or declines the opponent's takeback request. */
  answerTakeback: (accept: boolean) => invoke<void>('answer_takeback', { session, accept }),
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
  /** Who is logged in to the arimaa.com gameroom. */
  gameroomStatus: () => invoke<GameroomStatus>('gameroom_status'),
  /** Logs in to arimaa.com. An empty password uses the saved one; `remember`
   * saves the login (the password obfuscated), and without it a saved login
   * is forgotten. */
  gameroomLogin: (username: string, password: string, remember: boolean) =>
    invoke<GameroomStatus>('gameroom_login', { username, password, remember }),
  gameroomLogout: () => invoke<void>('gameroom_logout'),
  /** The games being played on arimaa.com now, and the last few finished. */
  gameroomGames: () => invoke<GameroomGames>('gameroom_games'),
  /** The postal games being played on arimaa.com (the live list leaves
   * them out); one page fetch, so only when asked. */
  gameroomPostalGames: () => invoke<PostalGameView[]>('gameroom_postal_games'),
  /** Invites `who` to an arimaa.com game with the user as `side`; the
   * answer comes as `gameroom://invitation`. */
  inviteGameroomPlayer: (who: string, side: Color, timeControl: string, rated: boolean, message: string) =>
    invoke<void>('invite_gameroom_player', { who, side, timeControl, rated, message }),
  /** Accepts an invitation (`InvitationView.otherId` and `created`) and
   * plays the game it makes in this session. */
  acceptGameroomInvitation: (otherId: string, created: string) =>
    invoke<void>('accept_gameroom_invitation', { session, otherId, created }),
  /** Declines an invitation; the inviter sees `reason`. */
  declineGameroomInvitation: (otherId: string, created: string, reason: string) =>
    invoke<void>('decline_gameroom_invitation', { otherId, created, reason }),
  /** Cancels the user's own invitation. */
  cancelGameroomInvitation: (otherId: string, created: string) =>
    invoke<void>('cancel_gameroom_invitation', { otherId, created }),
  /** The arimaa.com players whose username or real name contains `text`. */
  searchGameroomPlayers: (text: string) => invoke<PlayerMatchView[]>('search_gameroom_players', { text }),
  /** A player's finished games, newest first, 50 from `offset`. */
  gameroomPlayerGames: (playerId: string, offset: number) =>
    invoke<PlayerGamesView>('gameroom_player_games', { playerId, offset }),
  /** Opens an arimaa.com game in this session. A live game (its gameroom
   * id) is followed as a viewer, with changes arriving as `game://changed`
   * and `gameroom://watch` events; a finished game (its permanent id) is
   * loaded whole. */
  openGameroomGame: (gid: string) => invoke<void>('open_gameroom_game', { session, gid }),
  /** Plays live arimaa.com game `gid` (its gameroom id) as `side`: this
   * session becomes the game, and the user's moves are sent to the
   * server. */
  playGameroomGame: (gid: string, side: Color) =>
    invoke<void>('play_gameroom_game', { session, gid, side }),
  /** Creates an arimaa.com game with the user as `side` and plays it as
   * `playGameroomGame` does. `timeControl` is in the gameroom's format
   * (`2m/5m/100/0/30m`). The user's first move waits for an opponent
   * (`WatchView.waiting`). */
  createGameroomGame: (side: Color, timeControl: string, rated: boolean) =>
    invoke<void>('create_gameroom_game', { session, side, timeControl, rated }),
  /** Cancels a game the user created that nobody has joined; if this
   * session plays it, the session starts a new game. */
  cancelGameroomGame: (gid: string) => invoke<void>('cancel_gameroom_game', { session, gid }),
  /** Resigns the arimaa.com game this session plays; the result arrives
   * with the server's next update. */
  resignGameroomGame: () => invoke<void>('resign_gameroom_game', { session }),
  /** Sends a line to the chat of the arimaa.com game this session plays;
   * it comes back in `WatchView.chat` with the server's next update. */
  sendGameroomChat: (text: string) => invoke<void>('send_gameroom_chat', { session, text }),
  /** Stops following the game; it stays on the board. */
  stopWatching: () => invoke<void>('stop_watching', { session }),
  /** The game this session follows, if any. */
  watchStatus: () => invoke<WatchView | null>('watch_status', { session }),
};

export function isApiError(e: unknown): e is ApiError {
  return typeof e === 'object' && e !== null && 'kind' in e && 'message' in e;
}

export function errorMessage(e: unknown): string {
  if (isApiError(e)) return e.line != null ? `Line ${e.line}: ${e.message}` : e.message;
  return String(e);
}
