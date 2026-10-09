//! View types sent to the frontend. TypeScript definitions are generated from
//! these with ts-rs (`npm run bindings`); don't hand-edit `src/lib/bindings`.

use howdah_arimaa::{Color, GameResult, NodeId, Piece, PieceKind, Square, StepKind};
use serde::{Deserialize, Serialize};
use ts_rs::TS;

/// Stable identity for a piece across plies, so the UI can animate it.
pub type PieceId = u16;

/// Identifies one of the backend's sessions (a game shown in a window).
/// Every session command takes one, and session events carry it as a
/// `session` field.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize, TS)]
#[ts(export)]
pub struct SessionId(pub u32);

impl std::fmt::Display for SessionId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        self.0.fmt(f)
    }
}

/// The sessions open now, sent as `sessions://changed` whenever one opens
/// or closes (each is a game window's).
#[derive(Clone, Debug, Serialize, TS)]
#[ts(export)]
pub struct SessionsChanged {
    pub sessions: Vec<SessionId>,
}

#[derive(Clone, Debug, Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct PieceView {
    pub id: PieceId,
    pub piece: Piece,
    pub square: Square,
    pub frozen: bool,
}

#[derive(Clone, Debug, Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct PositionView {
    pub side_to_move: Color,
    pub pieces: Vec<PieceView>,
    /// AEI short format, handy for debugging and engine hand-off.
    pub short: String,
}

#[derive(Clone, Debug, Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct MoveView {
    /// Ply after this move; `goto_ply(ply)` shows the position it produced.
    pub ply: usize,
    /// Move number label, e.g. `2g`.
    pub label: String,
    pub notation: String,
}

/// One move of the game tree, as the move list shows it. `SessionView.tree`
/// lists them in display order: each main move, then the variations that
/// replace it (and continuations after a line's last move), nested.
#[derive(Clone, Debug, Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct MoveNodeView {
    pub id: NodeId,
    pub parent: NodeId,
    /// Ply after this move.
    pub ply: usize,
    /// Nesting: 0 on the main line, one more per level of variation.
    pub depth: usize,
    /// The first move of a variation.
    pub starts_variation: bool,
    /// How many variations end with this move (nested ones can end
    /// together).
    pub closes: u32,
    /// Move number label, e.g. `2g`.
    pub label: String,
    pub notation: String,
    /// How long the move took (its `%emt`), if known.
    #[ts(type = "number | null")]
    pub elapsed_ms: Option<u64>,
    /// Time from the start of the game to the end of this move: the move
    /// times on its line added up, if every one is known.
    #[ts(type = "number | null")]
    pub game_time_ms: Option<u64>,
    /// Annotation glyphs as written (`!?`, `$14`).
    pub glyphs: Vec<String>,
    pub comment: Option<String>,
    /// Introduction to the variation this move starts.
    pub intro: Option<String>,
    /// On the line being shown.
    pub on_line: bool,
    /// How the game ended at this move, if it did.
    pub result: Option<GameResult>,
    /// The first move of a variation with more moves after it, which can
    /// be folded.
    pub collapsible: bool,
    /// While folded, how many moves of the variation are hidden after this
    /// one (0 when it's open).
    pub folded: u32,
}

/// A piece on a square.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct PieceAt {
    pub piece: Piece,
    pub square: Square,
}

/// One step of the move that produced the shown position.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct LastStepView {
    /// The piece that stepped; an enemy of the mover when pushed or pulled.
    pub piece: Piece,
    pub from: Square,
    pub to: Square,
    /// A piece captured on a trap as a result of this step.
    pub captured: Option<PieceAt>,
}

/// The move that produced the shown position, for the last-move display.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct LastMoveView {
    pub color: Color,
    pub steps: Vec<LastStepView>,
}

/// Pieces of each color captured so far, strongest first.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct CapturedView {
    pub gold: Vec<PieceKind>,
    pub silver: Vec<PieceKind>,
}

/// What the board needs to replay the move that produced the shown position.
#[derive(Clone, Debug, Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct MoveReplay {
    /// Pieces before the move.
    pub before: Vec<PieceView>,
    pub animation: Vec<AnimStep>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub enum Phase {
    /// The side to move is arranging its setup.
    Setup,
    Play,
    /// The game has a result; no more moves at the end of the record.
    Over,
}

#[derive(Clone, Debug, Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct TurnStepView {
    pub notation: String,
    pub kind: StepKind,
}

#[derive(Clone, Debug, Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct TurnView {
    pub steps: Vec<TurnStepView>,
    pub steps_left: usize,
    /// Square a pushing piece must step into, if a push is under way.
    pub push_pending: Option<Square>,
    /// Why the turn can't be committed yet, or `None` if it can.
    pub commit_blocker: Option<String>,
}

/// A takeback request in a game on a server.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct TakebackView {
    /// The side asking.
    pub by: Color,
    /// Whether the server shows the request (the user's has reached it).
    pub shown: bool,
    /// The user's answer to the opponent's request, while it's on its way.
    pub answer: Option<bool>,
}

#[derive(Clone, Debug, Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct SessionView {
    /// The line being shown, one entry per move.
    pub moves: Vec<MoveView>,
    /// Every move of the game tree, in display order.
    pub tree: Vec<MoveNodeView>,
    /// The comment on the whole game (before the first move).
    pub game_comment: Option<String>,
    /// The node being shown (the root before any move).
    pub cursor: NodeId,
    /// In a match, the live node: the end of the game being played.
    pub live: Option<NodeId>,
    /// In a game with a remote side, the user's move sent to the server
    /// and not yet played there (a child of `live`).
    pub sent: Option<NodeId>,
    /// In a game with a remote side, an open takeback request.
    pub takeback: Option<TakebackView>,
    /// Why the user can't ask for a takeback now, or `None` if they can (a
    /// game they play on a server, with a move of theirs to take back).
    pub takeback_blocker: Option<String>,
    /// Ply being shown (0 = empty board, `moves.len()` = latest).
    pub ply: usize,
    pub phase: Phase,
    /// Position at `ply`, including any in-progress turn or setup draft.
    pub position: PositionView,
    /// Present while a turn is being entered.
    pub turn: Option<TurnView>,
    /// The move that produced the position at `ply`, unless it was a setup.
    pub last_move: Option<LastMoveView>,
    /// Pieces captured up to `ply`, including in the turn being entered.
    pub captured: CapturedView,
    /// Moves after `ply` on the line being shown.
    pub moves_after_cursor: usize,
    pub result: Option<GameResult>,
    pub end_marker: Option<String>,
    /// Why the game ended, when it wasn't on the board (illegal engine move,
    /// engine crash, ...).
    pub end_detail: Option<String>,
    /// Present during a match (a game with an engine or a clock).
    pub players: Option<PlayersView>,
    /// The players' names from the record's `Gold` and `Silver` tags (a
    /// loaded game's players), for when there's no match.
    pub tag_names: [Option<String>; 2],
    /// The players' ratings from the `GoldRating` and `SilverRating` tags.
    pub tag_ratings: [Option<String>; 2],
    /// The match's live clock.
    pub clock: Option<ClockView>,
    /// The clocks the player bars show: the live clock while the match
    /// goes on or at its live position, otherwise the clocks after the
    /// shown move, worked out from the move times (`ClockView.past`).
    pub shown_clock: Option<ClockView>,
    /// The side whose engine is thinking.
    pub thinking: Option<Color>,
    /// Whether board input is accepted now: any time outside a match; in a
    /// match, moves anywhere (as plans) but setups only on a human's turn.
    pub can_input: bool,
    /// Whether committing now plays a move in the match (a human's turn at
    /// the live position), rather than adding a variation.
    pub plays_live: bool,
    /// The plan move Enter would play (`3g Ee2n …`), when the cursor is in
    /// a plan for the user's move.
    pub plan_move: Option<String>,
    /// In a match, the latest move played (`3s ed7s …`), if any.
    pub live_move: Option<String>,
    /// Whether Undo step (Backspace) has something to undo: a step, the
    /// move into the shown position, or a takeback.
    pub can_undo: bool,
    /// Whether played moves can be taken back now (a match with takebacks,
    /// shown at the live position).
    pub can_take_back: bool,
    /// In a match, the ply of the live position if it's on the line being
    /// shown.
    pub live_ply: Option<usize>,
    /// The engine analysing the shown position, while analysis is on.
    pub analysis_engine: Option<String>,
    /// The analysis engine's options for this session (over its saved ones).
    pub analysis_options: Vec<EngineOption>,
    /// Whether analysis may be turned on (not while the user plays an
    /// online game).
    pub analysis_allowed: bool,
    /// In a server's game, the side to move when its every move would be a
    /// third repetition: it can't move, but the server may not end the
    /// game (Howdah doesn't, there; the side's clock runs out instead).
    pub stuck: Option<Color>,
    /// What analysis found at the shown node earlier, if anything, so the
    /// eval shows at once while the new search starts.
    pub stored_analysis: Option<AnalysisLine>,
}

/// An evaluation from gold's side.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, TS)]
#[serde(tag = "kind", rename_all = "camelCase")]
#[ts(export)]
pub enum Eval {
    /// Centi-rabbits: a rabbit up in the opening is about +100.
    CentiRabbits { value: i32 },
    /// A proven win.
    Decided { winner: Color },
}

/// One turn of an engine's principal variation, checked to be legal.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct PvTurn {
    /// Move number label, e.g. `12s`.
    pub label: String,
    /// In normal form, with capture tokens.
    pub notation: String,
    /// The steps, for drawing (`None` for a setup).
    pub steps: Option<LastMoveView>,
}

/// What an engine has found about one node.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct AnalysisLine {
    /// The node analysed: the line starts from its position.
    pub node: NodeId,
    /// As the engine writes it, e.g. `12+`.
    pub depth: Option<String>,
    pub eval: Option<Eval>,
    pub pv: Vec<PvTurn>,
    #[ts(type = "number | null")]
    pub nodes: Option<u64>,
    #[ts(type = "number | null")]
    pub time_ms: Option<u64>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub enum AnalysisState {
    /// Analysis is off.
    Off,
    /// The engine is starting.
    Starting,
    Searching,
    /// The engine stopped by itself (a proven result, a single move, a setup).
    Finished,
    /// The shown position has a result on the board; nothing to search.
    Idle,
    /// The engine failed; analysis was turned off. `detail` says why.
    Failed,
}

/// Payload of the `analysis://update` event: a snapshot of the analysis,
/// sent at most every 100 ms while the engine reports.
#[derive(Clone, Debug, Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct AnalysisView {
    pub state: AnalysisState,
    /// The engine's configured name.
    pub engine: Option<String>,
    /// The node being analysed.
    pub node: Option<NodeId>,
    /// The move being searched for, e.g. `12s`.
    pub label: Option<String>,
    /// The best line known for `node`: this search's, or a deeper one
    /// stored from an earlier visit.
    pub line: Option<AnalysisLine>,
    /// `line` is from an earlier visit; this search hasn't reached its depth.
    pub stored: bool,
    /// Engine output since the previous update.
    pub log: Vec<String>,
    pub detail: Option<String>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub enum PlayerKind {
    Human,
    Engine,
    /// Plays elsewhere, such as on arimaa.com.
    Remote,
}

#[derive(Clone, Debug, Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct PlayerView {
    pub kind: PlayerKind,
    pub name: String,
    /// An engine's id in the engine list.
    pub engine_id: Option<String>,
    /// An engine's options for this game only, over its saved ones.
    pub options: Vec<EngineOption>,
}

#[derive(Clone, Debug, Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct PlayersView {
    pub gold: PlayerView,
    pub silver: PlayerView,
}

/// One side's clock.
#[derive(Clone, Debug, Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct SideClockView {
    pub time_control: String,
    #[ts(type = "number")]
    pub move_time_ms: u64,
    #[ts(type = "number")]
    pub reserve_ms: u64,
    /// The time this side has for a turn from its start: move time plus
    /// reserve, within the time control's turn limit (the setup's minute
    /// included). The UI colors the clock by it.
    #[ts(type = "number")]
    pub turn_allowance_ms: u64,
    /// In a clock worked out for a past move, the time that move took,
    /// on the side that made it (shown in place of the move time, as
    /// arimaa.com's game viewer does).
    #[ts(type = "number | null")]
    pub last_used_ms: Option<u64>,
}

/// Clock snapshot. The UI counts down locally from `turnElapsedMs` for the
/// running side.
#[derive(Clone, Debug, Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct ClockView {
    /// Per side; `None` for an untimed side.
    pub gold: Option<SideClockView>,
    pub silver: Option<SideClockView>,
    /// Side whose clock is running, if any.
    pub running: Option<Color>,
    /// Time used so far on the running side's turn.
    #[ts(type = "number")]
    pub turn_elapsed_ms: u64,
    /// Total time the running side may take this turn.
    #[ts(type = "number")]
    pub turn_allowance_ms: u64,
    /// Time left before the game time limit, if there is one.
    #[ts(type = "number | null")]
    pub game_remaining_ms: Option<u64>,
    /// Worked out for the shown move from the move times and the time
    /// control, rather than the live clock.
    pub past: bool,
}

/// A player choice when starting a game.
#[derive(Clone, Debug, Deserialize, TS)]
#[serde(tag = "kind", rename_all = "camelCase")]
#[ts(export)]
pub enum PlayerSpec {
    Human,
    #[serde(rename_all = "camelCase")]
    Engine {
        engine_id: String,
        /// Options for this game only, sent over the engine's saved ones.
        #[serde(default)]
        #[ts(optional)]
        options: Option<Vec<EngineOption>>,
    },
}

#[derive(Clone, Debug, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct MatchSpec {
    pub gold: PlayerSpec,
    pub silver: PlayerSpec,
    /// Arimaa time control per side, e.g. `30s/5m`; `None` leaves that side
    /// untimed.
    pub gold_time_control: Option<String>,
    pub silver_time_control: Option<String>,
    /// Whether played moves can be taken back.
    #[serde(default)]
    pub takebacks: bool,
}

/// A configured engine.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct EngineSpec {
    /// Stable id; empty when adding a new engine.
    pub id: String,
    pub name: String,
    pub program: String,
    pub args: Vec<String>,
    pub working_dir: Option<String>,
    /// Sent with `setoption` after the handshake, for play and analysis
    /// (`threads`, `hash`, ...). They override the options the app sets for
    /// a known engine.
    #[serde(default)]
    pub options: Vec<EngineOption>,
    /// The manifest release it was installed from, if any.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub installed: Option<InstalledFrom>,
}

/// Which manifest release an engine was installed from.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct InstalledFrom {
    /// The manifest's `id`.
    pub manifest: String,
    pub version: String,
}

/// The engine manifests the user has added, and the suggested ones not
/// added yet.
#[derive(Clone, Debug, Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct EngineCatalogView {
    /// This computer's platform, as manifests name it (`linux-x86_64`).
    pub platform: String,
    pub manifests: Vec<ManifestView>,
    pub suggested: Vec<SuggestedEngine>,
}

/// An engine Howdah suggests, by the URL of its newest manifest.
#[derive(Clone, Debug, Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct SuggestedEngine {
    pub name: String,
    pub url: String,
}

/// An engine manifest the user added.
#[derive(Clone, Debug, Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct ManifestView {
    pub id: String,
    pub name: String,
    /// The release this manifest describes.
    pub version: String,
    pub author: Option<String>,
    pub description: Option<String>,
    pub homepage: Option<String>,
    pub license: Option<String>,
    /// Where it was fetched from (none for a file).
    pub source: Option<String>,
    /// Whether it says where updates are (or came from a URL).
    pub updatable: bool,
    /// Whether it has a download for this computer.
    pub downloadable: bool,
    /// The newest version installed from it, and that engine's id.
    pub installed_version: Option<String>,
    pub engine_id: Option<String>,
    pub options: Vec<ManifestOptionView>,
}

/// An option a manifest describes.
#[derive(Clone, Debug, Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct ManifestOptionView {
    pub name: String,
    /// `check`, `spin`, `float`, `combo`, `string`, `file`, `path` or `button`.
    pub kind: String,
    /// The default as `setoption` would send it.
    pub default: Option<String>,
    pub min: Option<f64>,
    pub max: Option<f64>,
    pub choices: Vec<String>,
    pub description: Option<String>,
}

impl From<&howdah_aei::manifest::OptionSpec> for ManifestOptionView {
    fn from(o: &howdah_aei::manifest::OptionSpec) -> Self {
        use howdah_aei::manifest::OptionKind as K;
        let (kind, default, min, max, choices) = match &o.kind {
            K::Check { default } => ("check", default.map(|b| b.to_string()), None, None, vec![]),
            K::Spin { default, min, max } => {
                ("spin", default.map(|d| d.to_string()), min.map(|x| x as f64), max.map(|x| x as f64), vec![])
            }
            K::Float { default, min, max } => ("float", default.map(|d| d.to_string()), *min, *max, vec![]),
            K::Combo { default, choices } => ("combo", default.clone(), None, None, choices.clone()),
            K::String { default } => ("string", default.clone(), None, None, vec![]),
            K::File { default } => ("file", default.clone(), None, None, vec![]),
            K::Path { default } => ("path", default.clone(), None, None, vec![]),
            K::Button => ("button", None, None, None, vec![]),
        };
        ManifestOptionView {
            name: o.name.clone(),
            kind: kind.into(),
            default,
            min,
            max,
            choices,
            description: o.description.clone(),
        }
    }
}

/// One engine option, as `setoption name <name> value <value>` sends it.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct EngineOption {
    pub name: String,
    pub value: String,
}

/// What an engine reported about itself in the handshake.
#[derive(Clone, Debug, Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct EngineIdentity {
    pub name: Option<String>,
    pub author: Option<String>,
    pub version: Option<String>,
    pub protocol_version: u32,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub enum EngineOutputKind {
    /// Parsed `info` line.
    Info,
    Log,
    /// Something the engine shouldn't have sent.
    Unexpected,
    /// A controller status message (started, thinking, failed...).
    Status,
}

/// Payload of the `engine://output` event.
#[derive(Clone, Debug, Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct EngineOutput {
    pub side: Color,
    pub kind: EngineOutputKind,
    /// Display text, e.g. `depth 12+` or the log line.
    pub text: String,
    pub depth: Option<String>,
    pub score: Option<i32>,
    pub pv: Option<Vec<String>>,
}

/// A piece appearing or disappearing during an animation.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct AnimPiece {
    pub id: PieceId,
    pub piece: Piece,
    pub square: Square,
}

/// One step of an animation. Play in order: fade in `restored`, slide `id`
/// from `from` to `to`, then fade out `captured`.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct AnimStep {
    pub id: PieceId,
    pub from: Square,
    pub to: Square,
    pub captured: Option<AnimPiece>,
    pub restored: Option<AnimPiece>,
    /// The side whose turn the step is in (a push moves the other side's
    /// piece), so a capture can be told from losing one's own piece.
    pub mover: Color,
}

impl AnimStep {
    /// The same step played backward.
    pub fn reversed(self) -> AnimStep {
        AnimStep { from: self.to, to: self.from, captured: None, restored: self.captured, ..self }
    }
}

/// Payload of the `game://changed` event: the new state, plus the steps to
/// animate from the previous state (empty means snap).
#[derive(Clone, Debug, Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct SessionUpdate {
    pub view: SessionView,
    pub animation: Vec<AnimStep>,
    /// For a live move: how long it took off the clock. Its animation
    /// shouldn't take longer than this.
    #[ts(type = "number | null")]
    pub animation_budget_ms: Option<u64>,
}

#[derive(Clone, Copy, Debug, Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct StepTarget {
    pub to: Square,
    /// `None` for setup swaps.
    pub kind: Option<StepKind>,
    /// Steps the piece needs to get there (more than 1 for a multi-step route).
    pub steps: u8,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub enum ErrorKind {
    /// A game record failed to parse or replay.
    Record,
    /// The requested step or move isn't legal.
    Illegal,
    /// The request doesn't fit the current state (e.g. a step during setup).
    State,
}

/// Error returned by commands.
#[derive(Clone, Debug, Serialize, TS, thiserror::Error)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
#[error("{message}")]
pub struct ApiError {
    pub kind: ErrorKind,
    pub message: String,
    /// 1-based record line, for `Record` errors.
    pub line: Option<usize>,
}

impl ApiError {
    pub fn new(kind: ErrorKind, message: impl ToString) -> ApiError {
        ApiError { kind, message: message.to_string(), line: None }
    }

    pub fn illegal(message: impl ToString) -> ApiError {
        ApiError::new(ErrorKind::Illegal, message)
    }

    pub fn state(message: impl ToString) -> ApiError {
        ApiError::new(ErrorKind::State, message)
    }
}

impl From<howdah_arimaa::RecordError> for ApiError {
    fn from(e: howdah_arimaa::RecordError) -> ApiError {
        ApiError { kind: ErrorKind::Record, message: e.error.to_string(), line: Some(e.line) }
    }
}

/// The arimaa.com gameroom login.
#[derive(Clone, Debug, Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct GameroomStatus {
    /// Who is logged in, if anyone.
    pub username: Option<String>,
    /// The username of the remembered login, if one is saved (its password
    /// stays in the backend).
    pub saved_username: Option<String>,
}

/// A game in one of the gameroom's lists: being played, open for a
/// player to sit, or the user's own. A free seat has no player.
#[derive(Clone, Debug, Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct LiveGameView {
    pub gid: String,
    pub gold: Option<String>,
    pub silver: Option<String>,
    pub time_control: Option<String>,
    pub rated: bool,
    pub postal: bool,
    /// The side to move, where the list says (the user's games).
    pub turn: Option<Color>,
}

/// A postal game being played on arimaa.com, from the gameroom's postal
/// games page.
#[derive(Clone, Debug, Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct PostalGameView {
    /// The gameroom id, to watch it by.
    pub gid: String,
    pub gold: String,
    pub silver: String,
    pub gold_rating: Option<String>,
    pub silver_rating: Option<String>,
    pub time_control: Option<String>,
    pub rated: bool,
}

/// A game recently finished on arimaa.com, from the gameroom's list.
#[derive(Clone, Debug, Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct RecentGameView {
    /// The permanent id.
    pub gid: String,
    pub gold: Option<String>,
    pub silver: Option<String>,
    pub gold_rating: Option<String>,
    pub silver_rating: Option<String>,
    pub time_control: Option<String>,
    pub rated: bool,
    pub postal: bool,
    pub result: Option<GameResult>,
    /// The last move's number (`5` for a game ending at 5b).
    pub moves: Option<u32>,
    /// When it ended, in milliseconds since the Unix epoch.
    #[ts(type = "number | null")]
    pub ended_ms: Option<u64>,
}

/// A player found by the gameroom's player search.
#[derive(Clone, Debug, Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct PlayerMatchView {
    /// The player id, which the player's games are asked for by.
    pub id: String,
    pub username: String,
    /// The real name the player gave.
    pub name: Option<String>,
}

/// A game from a player's past games.
#[derive(Clone, Debug, Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct PastGameView {
    /// The permanent id.
    pub gid: String,
    pub gold: String,
    pub silver: String,
    /// The players' ratings now, not at the time of the game.
    pub gold_rating: Option<String>,
    pub silver_rating: Option<String>,
    pub time_control: Option<String>,
    pub rated: bool,
    pub result: Option<GameResult>,
    /// The last move's number.
    pub moves: Option<u32>,
    /// When it finished, as the server writes it.
    pub finished: Option<String>,
}

/// A page of a player's past games, newest first.
#[derive(Clone, Debug, Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct PlayerGamesView {
    pub games: Vec<PastGameView>,
    /// The offset of the next (older) page, if there is one.
    pub next: Option<u32>,
}

/// The gameroom's game lists.
#[derive(Clone, Debug, Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct GameroomGames {
    /// Who the lists are for: the logged-in user.
    pub user: Option<String>,
    pub live: Vec<LiveGameView>,
    /// The last few games finished, newest first.
    pub recent: Vec<RecentGameView>,
    /// The user's games: ones they created, waiting for an opponent, and
    /// ones they play.
    pub mine: Vec<LiveGameView>,
    /// Games others created, with a seat to take.
    pub open: Vec<LiveGameView>,
    /// Open invitations, to the user and from them.
    pub invitations: Vec<InvitationView>,
}

/// An open invitation to play, to the user (`incoming`) or from them.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct InvitationView {
    pub incoming: bool,
    /// The other player's username and rating.
    pub opponent: Option<String>,
    pub opponent_rating: Option<String>,
    /// The side the user would play.
    pub side: Color,
    pub time_control: Option<String>,
    pub rated: bool,
    /// The inviter's message.
    pub message: Option<String>,
    /// The other player's id and when it was sent, which name the
    /// invitation in the accept, decline and cancel commands.
    pub other_id: String,
    pub created: String,
}

/// How an invitation the user sent was answered, sent as
/// `gameroom://invitation`.
#[derive(Clone, Debug, Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct InvitationAnswer {
    pub opponent: String,
    pub outcome: InvitationOutcome,
}

#[derive(Clone, Debug, Serialize, TS)]
#[serde(tag = "kind", rename_all = "camelCase")]
#[ts(export)]
pub enum InvitationOutcome {
    /// Accepted: the new game, and the user's side in it.
    Accepted { gid: String, side: Color },
    /// Declined, in the server's words with the reason.
    Declined { message: String },
    /// Gone without an answer (cancelled elsewhere, or expired).
    Gone,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub enum WatchState {
    /// Waiting for the server's next change.
    Following,
    /// The connection failed; trying again (`detail` says why).
    Reconnecting,
    /// The game is over.
    Ended,
    /// The user stopped watching, or the session moved on to another game;
    /// the watch is over and forgotten.
    Stopped,
    /// Watching failed and stopped (`detail` says why).
    Failed,
}

/// A session following an arimaa.com game, sent as `gameroom://watch`
/// whenever it changes. (The server doesn't show spectators the game's
/// chat.)
#[derive(Clone, Debug, PartialEq, Eq, Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct WatchView {
    pub gid: String,
    pub state: WatchState,
    pub detail: Option<String>,
    /// Whether the seat is an ASIP viewer seat, which the server sends
    /// moves to only every ~10 s.
    pub delayed: bool,
    /// The game's permanent arimaa.com id, once it has ended and the
    /// server has given one.
    pub finished_id: Option<String>,
    /// The user's side, when they play the game rather than watch it.
    pub side: Option<Color>,
    /// Whether it's a postal game (moves days apart).
    pub postal: bool,
    /// The game's time control in the gameroom's format, and whether it's
    /// rated (for a new game with the same settings).
    pub time_control: Option<String>,
    pub rated: bool,
    /// Whether gold and silver have left the table (the server says so;
    /// their clocks keep running).
    pub away: [bool; 2],
    /// Whether the user's seat waits for an opponent to sit down. Their
    /// first move is held until then.
    pub waiting: bool,
    /// Why the server refused the user's last move, until a move goes
    /// through.
    pub refused: Option<String>,
    /// The game's chat, oldest first (players get it; spectators don't).
    pub chat: Vec<ChatLineView>,
    /// Whether the user can write in the chat: at their seat, also after
    /// the game ends (players often stay to talk), until the server clears
    /// the table away or the user leaves.
    pub chat_open: bool,
}

/// A line of a gameroom game's chat.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct ChatLineView {
    /// Who wrote it, when the server says.
    pub side: Option<Color>,
    /// The move it was written at (`6g`), when the server says.
    pub label: Option<String>,
    pub text: String,
}
