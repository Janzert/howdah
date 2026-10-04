//! The game being viewed and edited in a window: pure state logic with no
//! Tauri types, so it can be unit tested (and reused by other front ends).
//!
//! The game is a [`GameTree`]: entering a move anywhere adds a branch, and
//! nothing is lost without an explicit delete. The session shows one line of
//! the tree at a time (the root, then one node per ply), with a cursor on it.
//!
//! A session is either free play (anyone may enter moves for either side,
//! at any ply) or a match: each side has a human or engine player, and
//! optionally a clock. A match plays one line, ending at its live node. A
//! human's move at the live node on their turn is played; any other move
//! (earlier, or while the opponent is to move) is a variation for planning
//! and is never sent. Engine moves arrive via `apply_engine_move` from the
//! controller.
//!
//! A remote player's moves are played elsewhere (a gameroom game) and
//! arrive through `sync_remote` as the full move list the server reports.
//! In local games the session is the authority on the game; when either
//! side is remote, the server is. The played moves are exactly the
//! server's, the clocks show the times it reports (`set_remote_clock`),
//! and only it ends the game on time or the turn limit (`finish_remote`).
//! Nothing played here goes into the game directly: a human's move stays
//! a plan, and no engine is asked to move, until sending moves to the
//! server is built (use case 2).

use std::collections::{HashMap, HashSet};
use std::time::{Duration, Instant};

use howdah_arimaa::{
    Color, Game, GameError, GameRecord, GameResult, GameTree, Glyph, Move, NodeId, Placement, Position,
    Route, Square, Step, StepEffect, StepKind, TimeControl, Turn, TurnBuilder, WinReason, default_setup,
    limit_score_winner, notation, outcome_after_turn,
};

use crate::dto::{
    AnalysisLine, AnimPiece, AnimStep, ApiError, CapturedView, ClockView, LastMoveView, LastStepView,
    MoveNodeView, MoveReplay, MoveView, Phase, PieceAt, PieceId, PieceView, PlayerKind, PlayerView,
    PlayersView, PositionView, SessionView, SideClockView, StepTarget, TurnStepView, TurnView,
};

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Player {
    Human,
    Engine {
        id: String,
        name: String,
    },
    /// Plays elsewhere, such as an arimaa.com player; the name is theirs.
    Remote {
        name: String,
    },
}

impl Player {
    fn view(&self) -> PlayerView {
        match self {
            Player::Human => PlayerView { kind: PlayerKind::Human, name: "Human".into() },
            Player::Engine { name, .. } => PlayerView { kind: PlayerKind::Engine, name: name.clone() },
            Player::Remote { name } => PlayerView { kind: PlayerKind::Remote, name: name.clone() },
        }
    }
}

#[derive(Clone, Debug)]
struct Clock {
    /// Time control per side (gold, silver); `None` means that side is untimed.
    tcs: [Option<TimeControl>; 2],
    reserves: [Duration; 2],
    game_started: Instant,
}

impl Clock {
    fn tc(&self, side: Color) -> Option<TimeControl> {
        self.tcs[side.index()]
    }

    /// When the game time limit (`G`) runs out: the earliest of the sides'.
    fn game_deadline(&self) -> Option<Instant> {
        self.tcs
            .iter()
            .flatten()
            .filter(|tc| tc.time_limit > 0)
            .map(|tc| self.game_started + Duration::from_secs(tc.time_limit.into()))
            .min()
    }

    /// The turn limit: the smaller of the sides' nonzero limits.
    fn turn_limit(&self) -> Option<u32> {
        self.tcs.iter().flatten().map(|tc| tc.turn_limit).filter(|&t| t > 0).min()
    }
}

#[derive(Clone, Debug)]
struct Match {
    players: [Player; 2],
    /// The end of the game being played. It's always on the main line.
    live: NodeId,
    clock: Option<Clock>,
    thinking: Option<Color>,
    /// When the current turn (at the live end of the game) started. Kept
    /// even without a clock, to time moves.
    turn_started: Instant,
    /// How long the most recent move took.
    last_move_time: Option<Duration>,
    /// Whether played moves can be taken back ([`Session::take_back`]).
    takebacks: bool,
    /// The clocks when each live node's turn began, so a takeback can
    /// restore them.
    turn_starts: HashMap<NodeId, TurnStart>,
}

/// The clocks at the start of a turn.
#[derive(Clone, Copy, Debug)]
struct TurnStart {
    /// Game time used so far (for the game time limit).
    elapsed: Duration,
    reserves: Option<[Duration; 2]>,
}

impl Match {
    /// Whether a server keeps the clock and decides results (a remote side
    /// plays).
    fn server_clock(&self) -> bool {
        self.players.iter().any(|p| matches!(p, Player::Remote { .. }))
    }

    fn turn_start(&self, now: Instant) -> TurnStart {
        TurnStart {
            elapsed: self
                .clock
                .as_ref()
                .map_or(Duration::ZERO, |c| now.saturating_duration_since(c.game_started)),
            reserves: self.clock.as_ref().map(|c| c.reserves),
        }
    }
}

/// The clocks as a server reports them, for a match with a remote side.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct RemoteClock {
    /// Reserves (gold, silver).
    pub reserves: [Duration; 2],
    /// Time the side to move has used on this turn.
    pub turn_elapsed: Duration,
    /// Time since the game started, when the game has a time limit.
    pub game_elapsed: Option<Duration>,
}

/// What the controller needs to ask an engine for a move.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct EngineTurn {
    pub generation: u64,
    pub side: Color,
    pub engine_id: String,
    pub ply: usize,
    /// Every move so far, in notation.
    pub moves: Vec<String>,
    pub time_control: Option<TimeControl>,
    /// Reserves (gold, silver) at the start of this turn.
    pub reserves: Option<[Duration; 2]>,
    pub deadline: Option<Instant>,
}

/// The engine chosen for analysis.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AnalysisEngine {
    pub id: String,
    pub name: String,
}

/// What analysis should search now: the shown node.
#[derive(Clone, Debug)]
pub struct AnalysisTarget {
    pub generation: u64,
    pub node: NodeId,
    /// The move being searched for, e.g. `12s`.
    pub label: String,
    /// Every move up to the node, in notation.
    pub moves: Vec<String>,
    /// The game up to the node, open for more moves (an outside result
    /// such as a resignation is dropped), to check the engine's lines.
    pub game: Game,
}

impl AnalysisTarget {
    /// Whether two targets are the same search.
    pub fn same(&self, other: &AnalysisTarget) -> bool {
        (self.generation, self.node) == (other.generation, other.node)
    }
}

/// Orders analysis depths as engines write them (`12`, `12+`, `12.4+`):
/// by number, then an unfinished iteration after a finished one. A line
/// without a depth comes first.
pub fn depth_key(depth: Option<&str>) -> (f64, bool) {
    let Some(d) = depth else { return (-1.0, false) };
    (d.trim_matches('+').parse().unwrap_or(0.0), d.contains('+'))
}

/// A move's steps, for drawing (`None` for a setup). `color` is the mover.
pub fn steps_view(color: Color, mv: &Move) -> Option<LastMoveView> {
    let Move::Steps(steps) = mv else { return None };
    let steps = steps
        .iter()
        .map(|e| LastStepView {
            piece: e.step.piece,
            from: e.step.from,
            to: e.to,
            captured: e.capture.map(|c| PieceAt { piece: c.piece, square: c.square }),
        })
        .collect();
    Some(LastMoveView { color, steps })
}

type IdMap = [Option<PieceId>; 64];

fn id_base(color: Color) -> PieceId {
    match color {
        Color::Gold => 0,
        Color::Silver => 16,
    }
}

/// Moves ids through one applied step and describes it for animation.
fn apply_ids(map: &mut IdMap, e: &StepEffect) -> AnimStep {
    let id = map[e.step.from.index() as usize].take().unwrap_or(PieceId::MAX);
    map[e.to.index() as usize] = Some(id);
    let captured = e.capture.map(|c| AnimPiece {
        id: map[c.square.index() as usize].take().unwrap_or(PieceId::MAX),
        piece: c.piece,
        square: c.square,
    });
    AnimStep { id, from: e.step.from, to: e.to, captured, restored: None }
}

/// Piece ids for every ply. Setup placements get `base + index`, and ids
/// then follow pieces through their steps.
fn compute_ids(game: &Game) -> Vec<IdMap> {
    let mut maps = vec![[None; 64]];
    for m in game.moves() {
        let mut map = *maps.last().unwrap();
        match m {
            Move::Setup(placements) => {
                for (i, p) in placements.iter().enumerate() {
                    map[p.square.index() as usize] = Some(id_base(p.piece.color) + i as PieceId);
                }
            }
            Move::Steps(effects) => {
                for e in effects {
                    apply_ids(&mut map, e);
                }
            }
        }
        maps.push(map);
    }
    maps
}

pub struct Session {
    tree: GameTree,
    /// Tags of a loaded record, kept for export.
    tags: Vec<(String, String)>,
    /// The line being shown: the root, then one node per ply.
    line: Vec<NodeId>,
    /// The child last shown after each node, so that coming back to a node
    /// follows the line the user was in rather than its main continuation.
    followed: HashMap<NodeId, NodeId>,
    /// First moves of variations shown folded.
    collapsed: HashSet<NodeId>,
    /// The line being shown as a plain game.
    game: Game,
    /// Ply being shown (an index into `line`).
    cursor: usize,
    /// Turn being entered from the position at `cursor`.
    turn: Option<TurnBuilder>,
    /// Setup being arranged, present when `cursor` is the last ply and a setup is due.
    setup_draft: Option<Vec<Placement>>,
    ids: Vec<IdMap>,
    matchup: Option<Match>,
    end_detail: Option<String>,
    /// Bumped whenever the game is replaced or the match changes, so
    /// background work for an old game can tell it's stale.
    generation: u64,
    /// The engine analysing the shown position, while analysis is on.
    analysis: Option<AnalysisEngine>,
    /// The deepest line analysis found at each node.
    evals: HashMap<NodeId, AnalysisLine>,
    /// Whether a step after a full turn finishes it and starts the next
    /// side's (see [`Session::try_step`]). A preference, kept across games.
    continue_turns: bool,
}

impl Default for Session {
    fn default() -> Self {
        Session::new()
    }
}

impl Session {
    pub fn new() -> Session {
        Session::with_tree(GameTree::new(), Vec::new(), GameTree::ROOT)
    }

    /// A session showing the line through `at`, with the cursor on it.
    fn with_tree(tree: GameTree, tags: Vec<(String, String)>, at: NodeId) -> Session {
        let cursor = tree[at].ply();
        let line = tree.line_through(at);
        let game = tree.to_game(*line.last().expect("the line has the root")).expect("the line exists");
        let mut s = Session {
            tree,
            tags,
            line,
            followed: HashMap::new(),
            collapsed: HashSet::new(),
            game,
            cursor,
            turn: None,
            setup_draft: None,
            ids: Vec::new(),
            matchup: None,
            end_detail: None,
            generation: 0,
            analysis: None,
            evals: HashMap::new(),
            continue_turns: true,
        };
        s.refresh();
        s
    }

    /// Replaces the game, keeping the generation counter moving forward.
    /// Analysis stays on.
    fn replace(&mut self, tree: GameTree, tags: Vec<(String, String)>, at: NodeId, matchup: Option<Match>) {
        let generation = self.generation + 1;
        let (analysis, continue_turns) = (self.analysis.take(), self.continue_turns);
        *self = Session::with_tree(tree, tags, at);
        self.generation = generation;
        self.analysis = analysis;
        self.continue_turns = continue_turns;
        self.matchup = matchup;
        self.refresh();
    }

    pub fn generation(&self) -> u64 {
        self.generation
    }

    /// Replaces the record's tags (players, event and so on).
    pub fn set_tags(&mut self, tags: Vec<(String, String)>) {
        self.tags = tags;
    }

    /// Sets one tag, replacing any earlier value.
    pub fn set_tag(&mut self, name: &str, value: &str) {
        match self.tags.iter_mut().find(|(n, _)| n == name) {
            Some((_, v)) => *v = value.to_string(),
            None => self.tags.push((name.to_string(), value.to_string())),
        }
    }

    fn player(&self, side: Color) -> &Player {
        self.matchup.as_ref().map_or(&Player::Human, |m| &m.players[side.index()])
    }

    fn cursor_node(&self) -> NodeId {
        self.line[self.cursor]
    }

    fn line_end(&self) -> NodeId {
        *self.line.last().expect("the line has the root")
    }

    /// The end of the game: the match's live node, or in free play the end
    /// of the line being shown.
    fn live(&self) -> NodeId {
        self.matchup.as_ref().map_or_else(|| self.line_end(), |m| m.live)
    }

    fn live_result(&self) -> Option<GameResult> {
        self.tree[self.live()].result()
    }

    /// Side to move at the live end of the game.
    fn live_side(&self) -> Color {
        self.tree[self.live()].position().side_to_move()
    }

    /// Whether committing now plays a move in the match (a human's turn at
    /// the live node), rather than adding a variation. Never in a game
    /// with a remote side: its moves come only from the server.
    pub fn plays_live(&self) -> bool {
        self.matchup.as_ref().is_some_and(|m| !m.server_clock())
            && self.cursor_node() == self.live()
            && self.live_result().is_none()
            && *self.player(self.live_side()) == Player::Human
    }

    /// Whether board input is accepted now. In a match, moves can be
    /// entered anywhere to plan (they become variations), but a setup only
    /// on a human's turn at the live node.
    pub fn can_input(&self) -> bool {
        self.matchup.is_none() || !Game::is_setup_ply(self.cursor) || self.plays_live()
    }

    fn require_input(&self) -> Result<(), ApiError> {
        if self.can_input() {
            Ok(())
        } else {
            Err(ApiError::state("a setup can only be entered on your own turn"))
        }
    }

    /// Shows `node`: on the current line if it's there, otherwise on the
    /// line through it.
    fn show(&mut self, node: NodeId) {
        match self.line.iter().position(|&n| n == node) {
            Some(i) => self.cursor = i,
            None => {
                self.line = self.line_through(node);
                self.cursor = self.tree[node].ply();
            }
        }
        self.refresh();
    }

    /// The path to `node`, then on to the end of a line: the line last
    /// shown after each node where there is one, otherwise the main
    /// continuation.
    fn line_through(&self, node: NodeId) -> Vec<NodeId> {
        let mut line = self.tree.path(node);
        let mut at = node;
        loop {
            let n = &self.tree[at];
            let followed = self.followed.get(&at).filter(|c| n.children().contains(c));
            let next = match followed {
                Some(&c) => c,
                None if n.result().is_some() => break,
                None => match n.children().first() {
                    Some(&c) => c,
                    None => break,
                },
            };
            line.push(next);
            at = next;
        }
        line
    }

    /// Brings the shown line up to date with the tree (it grows when moves
    /// are added at its end) and recomputes what depends on it.
    fn refresh(&mut self) {
        self.line = self.line_through(self.line_end());
        for pair in self.line.windows(2) {
            self.followed.insert(pair[0], pair[1]);
        }
        self.game = self.tree.to_game(self.line_end()).expect("the line exists");
        self.ids = compute_ids(&self.game);
        let setup_due = Game::is_setup_ply(self.cursor)
            && self.tree[self.cursor_node()].result().is_none()
            && match &self.matchup {
                None => self.cursor_node() == self.line_end(),
                Some(_) => self.plays_live(),
            };
        if !setup_due {
            self.setup_draft = None;
        } else if self.setup_draft.is_none() {
            let side = self.game.current_position().side_to_move();
            self.setup_draft = Some(default_setup(side));
        }
    }

    fn cursor_position(&self) -> &Position {
        self.tree[self.cursor_node()].position()
    }

    pub fn new_game(&mut self) {
        self.replace(GameTree::new(), Vec::new(), GameTree::ROOT, None);
    }

    /// Loads a record, showing the end of its main line.
    pub fn load(&mut self, record: &str) -> Result<(), ApiError> {
        let record = GameRecord::parse(record)?;
        let end = record.tree.line_end(GameTree::ROOT);
        self.replace(record.tree, record.tags, end, None);
        Ok(())
    }

    /// Starts a new game between the given players, with a time control per
    /// side (gold, silver); `None` leaves that side untimed.
    /// With `takebacks`, played moves can be taken back.
    pub fn start_match(
        &mut self,
        players: [Player; 2],
        time_controls: [Option<TimeControl>; 2],
        takebacks: bool,
    ) {
        let now = Instant::now();
        let clock = time_controls.iter().any(Option::is_some).then(|| Clock {
            tcs: time_controls,
            reserves: time_controls.map(|tc| tc.map_or(Duration::ZERO, |t| t.starting_reserve())),
            game_started: now,
        });
        let mut matchup = Match {
            players,
            live: GameTree::ROOT,
            clock,
            thinking: None,
            turn_started: Instant::now(),
            last_move_time: None,
            takebacks,
            turn_starts: HashMap::new(),
        };
        matchup.turn_starts.insert(GameTree::ROOT, matchup.turn_start(now));
        self.replace(GameTree::new(), Vec::new(), GameTree::ROOT, Some(matchup));
    }

    /// Stops the match: the game stays as it is, and both sides become free
    /// play again.
    pub fn end_match(&mut self) {
        if self.matchup.take().is_some() {
            self.generation += 1;
            self.refresh();
        }
    }

    pub fn set_thinking(&mut self, side: Option<Color>) {
        if let Some(m) = &mut self.matchup {
            m.thinking = side;
        }
    }

    /// The engine turn to request now, if an engine is to move.
    pub fn engine_turn(&self) -> Option<EngineTurn> {
        let m = self.matchup.as_ref().filter(|m| !m.server_clock())?;
        if self.live_result().is_some() {
            return None;
        }
        let side = self.live_side();
        let Player::Engine { id, .. } = &m.players[side.index()] else { return None };
        let path = self.tree.path(m.live);
        Some(EngineTurn {
            generation: self.generation,
            side,
            engine_id: id.clone(),
            ply: self.tree[m.live].ply(),
            moves: path[1..].iter().filter_map(|&id| self.tree[id].mv().map(Move::notation)).collect(),
            time_control: m.clock.as_ref().and_then(|c| c.tc(side)),
            reserves: m.clock.as_ref().map(|c| c.reserves),
            deadline: self.turn_deadline(),
        })
    }

    /// When the side to move runs out of time (its turn allowance or the
    /// game time limit, whichever comes first), in a timed match.
    pub fn turn_deadline(&self) -> Option<Instant> {
        let m = self.matchup.as_ref()?;
        let clock = m.clock.as_ref()?;
        if self.live_result().is_some() {
            return None;
        }
        let side = self.live_side();
        let turn = clock.tc(side).map(|tc| m.turn_started + tc.turn_allowance(clock.reserves[side.index()]));
        match (turn, clock.game_deadline()) {
            (Some(t), Some(g)) => Some(t.min(g)),
            (t, g) => t.or(g),
        }
    }

    /// Ends the game if time has run out: the side to move loses on time, or
    /// if the game time limit passed, the game is decided by score. Returns
    /// whether it ended.
    /// With a remote side the server flags time, so this never ends it.
    pub fn check_timeout(&mut self, now: Instant) -> bool {
        if self.matchup.as_ref().is_some_and(Match::server_clock) {
            return false;
        }
        if !self.turn_deadline().is_some_and(|d| now >= d) {
            return false;
        }
        let game_limit = self.matchup.as_ref().and_then(|m| m.clock.as_ref()).and_then(Clock::game_deadline);
        if game_limit.is_some_and(|g| now >= g) {
            let winner = limit_score_winner(self.tree[self.live()].position());
            self.finish(
                GameResult { winner, reason: WinReason::Score },
                Some("game time limit reached".into()),
            );
        } else {
            let side = self.live_side();
            self.finish(GameResult { winner: side.opponent(), reason: WinReason::Timeout }, None);
        }
        true
    }

    /// Ends the game with a result decided outside the board. Plans made
    /// after the live position stay, as analysis after the end.
    fn finish(&mut self, result: GameResult, detail: Option<String>) {
        let live = self.live();
        let at_live = self.cursor_node() == live;
        if self.tree.end_line(live, result).is_ok() {
            self.end_detail = detail;
            if at_live {
                self.turn = None;
            }
            if let Some(m) = &mut self.matchup {
                m.thinking = None;
            }
        }
        self.refresh();
    }

    /// Engine ids playing (gold, silver) in the current match.
    pub fn engine_players(&self) -> [Option<String>; 2] {
        let id = |p: &Player| match p {
            Player::Engine { id, .. } => Some(id.clone()),
            Player::Human | Player::Remote { .. } => None,
        };
        match &self.matchup {
            Some(m) => [id(&m.players[0]), id(&m.players[1])],
            None => [None, None],
        }
    }

    /// Ends the game because the engine for `side` failed (crashed, didn't
    /// start, ...), at any point of the game.
    pub fn engine_failed(&mut self, generation: u64, side: Color, detail: String) {
        if generation == self.generation && self.live_result().is_none() {
            self.finish(GameResult { winner: side.opponent(), reason: WinReason::Forfeit }, Some(detail));
        }
    }

    /// Timing and clock bookkeeping for a move by the side to move, before
    /// it's added. Returns false (and ends the game) if the move came too
    /// late. A server keeping the clock sets the reserves itself.
    fn clock_move(&mut self, now: Instant) -> bool {
        let setup = Game::is_setup_ply(self.tree[self.live()].ply());
        let side = self.live_side();
        let Some(m) = self.matchup.as_mut() else { return true };
        let used = now.saturating_duration_since(m.turn_started);
        m.last_move_time = Some(used);
        m.turn_started = now;
        if m.server_clock() {
            return true;
        }
        let Some(clock) = m.clock.as_mut() else { return true };
        let Some(tc) = clock.tc(side) else { return true };
        let reserve = clock.reserves[side.index()];
        if used > tc.turn_allowance(reserve) {
            self.finish(GameResult { winner: side.opponent(), reason: WinReason::Timeout }, None);
            return false;
        }
        if !setup {
            clock.reserves[side.index()] = tc.reserve_after(reserve, used);
        }
        true
    }

    /// How long the most recent move in the match took. Live animations of
    /// that move shouldn't take longer than this.
    pub fn last_move_time(&self) -> Option<Duration> {
        self.matchup.as_ref().and_then(|m| m.last_move_time)
    }

    /// End-of-turn checks that depend on the match (the turn limit). A
    /// server keeping the clock applies them itself.
    fn after_move(&mut self, mover: Color, ply: usize) {
        let limit = self
            .matchup
            .as_ref()
            .filter(|m| !m.server_clock())
            .and_then(|m| m.clock.as_ref())
            .and_then(Clock::turn_limit);
        if let Some(limit) = limit
            && mover == Color::Silver
            && (ply / 2 + 1) as u32 >= limit
        {
            let winner = limit_score_winner(self.game.current_position());
            self.finish(
                GameResult { winner, reason: WinReason::Score },
                Some(format!("turn limit of {limit} reached")),
            );
        }
    }

    /// Plays a move from the engine for `side` on turn `ply`. A stale reply
    /// (the game moved on) is ignored. An illegal move loses the game. The
    /// board follows the new move if the user was watching the live position.
    pub fn apply_engine_move(
        &mut self,
        generation: u64,
        side: Color,
        ply: usize,
        text: &str,
    ) -> Result<Vec<AnimStep>, ApiError> {
        let Some(live) = self.matchup.as_ref().map(|m| m.live) else {
            return Err(ApiError::state("no match"));
        };
        if generation != self.generation
            || ply != self.tree[live].ply()
            || side != self.live_side()
            || self.live_result().is_some()
        {
            return Err(ApiError::state("stale engine move"));
        }
        if let Some(m) = &mut self.matchup {
            m.thinking = None;
        }
        if !self.clock_move(Instant::now()) {
            return Ok(Vec::new());
        }
        let following = self.cursor_node() == live;
        if text.trim().eq_ignore_ascii_case("resign") {
            self.finish(GameResult { winner: side.opponent(), reason: WinReason::Resignation }, None);
            return Ok(Vec::new());
        }
        let node = match self.tree.add_notation(live, text) {
            Ok(node) => node,
            Err(e) => {
                let detail = format!("{side:?} engine played {text:?}: {e}");
                self.finish(
                    GameResult { winner: side.opponent(), reason: WinReason::IllegalMove },
                    Some(detail),
                );
                return Ok(Vec::new());
            }
        };
        self.play_live(node);
        self.after_move(side, ply);
        if following {
            self.turn = None;
            self.show(node);
            return Ok(self.move_animation(ply));
        }
        self.refresh();
        Ok(Vec::new())
    }

    /// Brings the match's played moves in line with `moves`, every move a
    /// server reports for a game with a remote side (in notation, setups
    /// included). Moves past the live node are played. If the server's list
    /// is shorter or differs (a takeback), the live node goes back to where
    /// they agree first, and the moves dropped stay as the continuation, as
    /// with [`Session::take_back`]. The board follows if the user was
    /// watching the live position, animating a single new move.
    ///
    /// A move the rules refuse is an error; the moves before it are still
    /// played.
    pub fn sync_remote(&mut self, generation: u64, moves: &[String]) -> Result<Vec<AnimStep>, ApiError> {
        let Some(m) = self.matchup.as_ref().filter(|m| m.server_clock()) else {
            return Err(ApiError::state("no game with a remote player"));
        };
        if generation != self.generation {
            return Err(ApiError::state("stale remote moves"));
        }
        let old_live = m.live;
        let path = self.tree.path(old_live);
        let following = self.cursor_node() == old_live;
        let refuse = |i: usize, e: GameError| ApiError::state(format!("server move {}: {e}", i + 1));
        // How many of the server's moves the played line already has.
        let mut agree = 0;
        while agree + 1 < path.len() && agree < moves.len() {
            let node = self.tree.add_notation(path[agree], &moves[agree]).map_err(|e| refuse(agree, e))?;
            if node != path[agree + 1] {
                break;
            }
            agree += 1;
        }
        if agree + 1 == path.len() && agree == moves.len() {
            return Ok(Vec::new());
        }
        if self.live_result().is_some() {
            return Err(ApiError::state("the server's moves changed after the game ended"));
        }
        if agree + 1 < path.len() {
            let Some(m) = &mut self.matchup else { unreachable!("checked above") };
            m.live = path[agree];
            m.thinking = None;
        }
        let mut added = 0;
        let mut refused = None;
        for (i, text) in moves.iter().enumerate().skip(agree) {
            let live = self.live();
            let node = match self.tree.add_notation(live, text) {
                Ok(node) => node,
                Err(e) => {
                    refused = Some(refuse(i, e));
                    break;
                }
            };
            let (mover, ply) = (self.live_side(), self.tree[live].ply());
            self.clock_move(Instant::now());
            self.play_live(node);
            self.after_move(mover, ply);
            added += 1;
        }
        let mut anim = Vec::new();
        if following {
            self.turn = None;
            let live = self.live();
            self.show(live);
            if agree + 1 == path.len() && added == 1 && refused.is_none() {
                anim = self.move_animation(self.tree[live].ply() - 1);
            }
        } else {
            self.refresh();
        }
        refused.map_or(Ok(anim), Err)
    }

    /// Sets the clocks to what the server reports, in a game with a remote
    /// side.
    pub fn set_remote_clock(&mut self, generation: u64, reported: RemoteClock) -> Result<(), ApiError> {
        if generation != self.generation {
            return Err(ApiError::state("stale remote clock"));
        }
        let Some(m) = self.matchup.as_mut().filter(|m| m.server_clock()) else {
            return Err(ApiError::state("no game with a remote player"));
        };
        let now = Instant::now();
        m.turn_started = now.checked_sub(reported.turn_elapsed).unwrap_or(now);
        if let Some(clock) = &mut m.clock {
            clock.reserves = reported.reserves;
            if let Some(elapsed) = reported.game_elapsed {
                clock.game_started = now.checked_sub(elapsed).unwrap_or(now);
            }
        }
        Ok(())
    }

    /// Ends a game with a remote side with the result the server reports.
    /// A result the rules already gave the last move (a goal, say) must be
    /// the same one; a different one is an error, since the server decides.
    pub fn finish_remote(
        &mut self,
        generation: u64,
        result: GameResult,
        detail: Option<String>,
    ) -> Result<(), ApiError> {
        if generation != self.generation || !self.matchup.as_ref().is_some_and(Match::server_clock) {
            return Err(ApiError::state("no game with a remote player"));
        }
        match self.live_result() {
            None => self.finish(result, detail),
            Some(r) if r == result => {}
            Some(r) => {
                return Err(ApiError::state(format!(
                    "the server's result {result:?} differs from the rules' {r:?}"
                )));
            }
        }
        Ok(())
    }

    /// Makes `node`, a child of the live node, the match's new live node
    /// and the main continuation (a plan for the same move becomes it).
    fn play_live(&mut self, node: NodeId) {
        self.tree.make_first(node).expect("the node exists");
        if let Some(m) = &mut self.matchup {
            m.live = node;
            let start = m.turn_start(m.turn_started);
            m.turn_starts.insert(node, start);
        }
    }

    /// The game as a record: a plain record when it's one line without
    /// comments or tags (or `main_line_only` is set), otherwise the full
    /// record with variations.
    pub fn export(&self, main_line_only: bool) -> String {
        if main_line_only || (self.tags.is_empty() && self.tree.is_plain()) {
            return self.tree.main_game().to_record();
        }
        GameRecord { tags: self.tags.clone(), tree: self.tree.clone() }.to_record()
    }

    /// Shows the live position of a match (or the end of the line in free
    /// play), discarding any turn in progress.
    pub fn goto_live(&mut self) {
        self.turn = None;
        let live = self.live();
        self.show(live);
    }

    /// Shows `node`, discarding any turn in progress: on the current line if
    /// it's there, otherwise on the line through it. A move forward or back
    /// from the shown node is animated.
    pub fn goto_node(&mut self, node: NodeId) -> Result<Vec<AnimStep>, ApiError> {
        if !self.tree.contains(node) {
            return Err(ApiError::state("that move no longer exists"));
        }
        if let Some(ply) = self.line.iter().position(|&n| n == node) {
            return self.goto(ply);
        }
        let from = self.cursor_node();
        let had_turn = self.turn.take().is_some();
        self.show(node);
        // Off the line, only a step forward can be next to the old cursor.
        let forward = self.tree[node].parent() == Some(from);
        Ok(if forward && !had_turn { self.move_animation(self.cursor - 1) } else { Vec::new() })
    }

    /// Shows the move `offset` places away among the alternatives for the
    /// shown move (the previous or next variation), if there is one.
    pub fn goto_sibling(&mut self, offset: isize) -> Result<Vec<AnimStep>, ApiError> {
        let node = self.cursor_node();
        let Some(parent) = self.tree[node].parent() else { return Ok(Vec::new()) };
        let siblings = self.tree[parent].children();
        let i = siblings.iter().position(|&c| c == node).expect("a child of its parent");
        match i.checked_add_signed(offset).and_then(|j| siblings.get(j)) {
            Some(&other) => self.goto_node(other),
            None => Ok(Vec::new()),
        }
    }

    /// Shows the next (or previous) move on the shown line that has
    /// alternatives, or the end (or start) of the line if there's none.
    pub fn goto_branch(&mut self, forward: bool) -> Result<Vec<AnimStep>, ApiError> {
        let branches = |ply: &usize| {
            let node = &self.tree[self.line[*ply]];
            node.parent().is_some_and(|p| self.tree[p].children().len() > 1)
        };
        let ply = if forward {
            (self.cursor + 1..self.line.len()).find(branches).unwrap_or(self.line.len() - 1)
        } else {
            (1..self.cursor).rev().find(branches).unwrap_or(0)
        };
        if ply == self.cursor { Ok(Vec::new()) } else { self.goto(ply) }
    }

    /// Sets the comment after `node` (the game comment for the root). An
    /// empty comment removes it.
    pub fn set_comment(&mut self, node: NodeId, text: &str) -> Result<Vec<AnimStep>, ApiError> {
        let text = text.trim_end();
        let annotation = self.tree.annotation_mut(node).map_err(ApiError::illegal)?;
        annotation.comment = (!text.is_empty()).then(|| text.to_string());
        Ok(Vec::new())
    }

    /// Adds a glyph to `node`'s move, replacing one of the same kind, or
    /// removes it if the move has it already.
    pub fn toggle_glyph(&mut self, node: NodeId, glyph: Glyph) -> Result<Vec<AnimStep>, ApiError> {
        if node == GameTree::ROOT {
            return Err(ApiError::state("glyphs go on moves"));
        }
        let annotation = self.tree.annotation_mut(node).map_err(ApiError::illegal)?;
        if annotation.glyphs.contains(&glyph) {
            annotation.remove_glyph(glyph);
        } else {
            annotation.set_glyph(glyph);
        }
        Ok(Vec::new())
    }

    /// True if `node` is the first move of a variation: not its parent's
    /// main continuation, or after the end of the game.
    fn starts_variation(&self, node: NodeId) -> bool {
        self.tree[node].parent().is_some_and(|p| {
            let parent = &self.tree[p];
            parent.result().is_some() || parent.children().first() != Some(&node)
        })
    }

    /// Folds the variation starting at `node` to its first move, or unfolds it.
    pub fn toggle_collapsed(&mut self, node: NodeId) -> Result<Vec<AnimStep>, ApiError> {
        if !self.tree.contains(node) || !self.starts_variation(node) {
            return Err(ApiError::state("only variations can be folded"));
        }
        if !self.collapsed.remove(&node) {
            self.collapsed.insert(node);
        }
        Ok(Vec::new())
    }

    /// Turns analysis on with `engine`, or off with `None`.
    pub fn set_analysis(&mut self, engine: Option<AnalysisEngine>) {
        self.analysis = engine;
    }

    pub fn analysis_engine(&self) -> Option<&AnalysisEngine> {
        self.analysis.as_ref()
    }

    /// What analysis should search now: the shown node, unless analysis is
    /// off or the node's position has a result on the board. A turn being
    /// entered is ignored (AEI can't hand an engine a partial turn), so the
    /// search stays on the turn's start.
    pub fn analysis_target(&self) -> Option<AnalysisTarget> {
        self.analysis.as_ref()?;
        let node = self.cursor_node();
        if self.tree[node].is_terminal() {
            return None;
        }
        let mut game = self.tree.to_game(node).expect("the node exists");
        game.reopen();
        Some(AnalysisTarget {
            generation: self.generation,
            node,
            label: notation::move_label(self.tree[node].ply()),
            moves: game.moves().iter().map(Move::notation).collect(),
            game,
        })
    }

    /// Keeps `line` as what analysis found at its node, unless a deeper
    /// line is kept already. Lines for an old game are ignored.
    pub fn store_analysis(&mut self, generation: u64, line: &AnalysisLine) {
        if generation != self.generation || !self.tree.contains(line.node) {
            return;
        }
        let deeper = |old: &AnalysisLine| depth_key(old.depth.as_deref()) > depth_key(line.depth.as_deref());
        if !self.evals.get(&line.node).is_some_and(deeper) {
            self.evals.insert(line.node, line.clone());
        }
    }

    /// The line kept for `node` by [`Session::store_analysis`].
    pub fn stored_analysis(&self, generation: u64, node: NodeId) -> Option<&AnalysisLine> {
        (generation == self.generation).then(|| self.evals.get(&node)).flatten()
    }

    /// Adds `moves` (in notation) as a line from `from`, reusing moves that
    /// are there already, and shows its last move. Nothing is added if a
    /// move is illegal. In a match this never plays the live move: a line
    /// from the live node is a plan, like any move entered there.
    pub fn add_line(&mut self, from: NodeId, moves: &[String]) -> Result<Vec<AnimStep>, ApiError> {
        if !self.tree.contains(from) {
            return Err(ApiError::state("that move no longer exists"));
        }
        if moves.is_empty() {
            return Err(ApiError::state("no moves to add"));
        }
        let mut tree = self.tree.clone();
        let mut at = from;
        for m in moves {
            if self.matchup.is_some() && Game::is_setup_ply(tree[at].ply()) {
                return Err(ApiError::state("a setup can't be planned during a match"));
            }
            at = tree.add_notation(at, m).map_err(|e| ApiError::illegal(format!("{m}: {e}")))?;
        }
        self.tree = tree;
        let shown = self.cursor_node();
        let had_turn = self.turn.take().is_some();
        self.show(at);
        if from != shown || had_turn {
            return Ok(Vec::new());
        }
        let start = self.tree[from].ply();
        Ok((start..self.cursor).flat_map(|ply| self.move_animation(ply)).collect())
    }

    /// The position after playing `moves` (in notation) from `from`, for
    /// previewing a line.
    pub fn preview_line(&self, from: NodeId, moves: &[String]) -> Result<PositionView, ApiError> {
        let mut game = self.tree.to_game(from).map_err(|_| ApiError::state("that move no longer exists"))?;
        game.reopen();
        for m in moves {
            game.play_notation(m).map_err(|e| ApiError::illegal(format!("{m}: {e}")))?;
        }
        Ok(position_view(game.current_position(), &[None; 64]))
    }

    /// Applies an edit to the tree's lines (promote, delete, ...). While a
    /// match is being played its line stays the main line, so an edit that
    /// would change or remove it is refused.
    fn edit_lines(
        &mut self,
        f: impl FnOnce(&mut GameTree) -> Result<(), GameError>,
    ) -> Result<Vec<AnimStep>, ApiError> {
        let mut tree = self.tree.clone();
        f(&mut tree).map_err(ApiError::illegal)?;
        if let Some(m) = &self.matchup {
            if !tree.contains(m.live) {
                return Err(ApiError::illegal("the match's moves can't be deleted"));
            }
            if self.tree[m.live].result().is_none() && !tree.is_main_line(m.live) {
                return Err(ApiError::illegal("the game being played stays the main line"));
            }
        }
        self.tree = tree;
        self.evals.retain(|&n, _| self.tree.contains(n));
        // Keep the shown line if it still exists; otherwise fall back to
        // the deepest part of it that does.
        let keep = self.line.iter().take_while(|&&n| self.tree.contains(n)).count();
        if keep < self.line.len() {
            self.line.truncate(keep);
            if self.cursor >= keep {
                self.cursor = keep - 1;
                self.turn = None;
            }
        }
        self.refresh();
        Ok(Vec::new())
    }

    /// Moves `node` one place earlier among the alternatives for its ply.
    pub fn promote(&mut self, node: NodeId) -> Result<Vec<AnimStep>, ApiError> {
        self.edit_lines(|t| t.promote(node))
    }

    /// Moves `node` one place later among the alternatives for its ply.
    pub fn demote(&mut self, node: NodeId) -> Result<Vec<AnimStep>, ApiError> {
        self.edit_lines(|t| t.demote(node))
    }

    /// Makes the line through `node` the main line.
    pub fn make_main_line(&mut self, node: NodeId) -> Result<Vec<AnimStep>, ApiError> {
        self.edit_lines(|t| t.make_main_line(node))
    }

    /// Deletes `node` and every move after it.
    pub fn delete_from(&mut self, node: NodeId) -> Result<Vec<AnimStep>, ApiError> {
        self.edit_lines(|t| t.delete(node))
    }

    /// Shows `ply` on the current line, discarding any turn in progress.
    /// Moving one ply forward or back animates that move.
    pub fn goto(&mut self, ply: usize) -> Result<Vec<AnimStep>, ApiError> {
        if ply > self.game.ply_count() {
            return Err(ApiError::state(format!("ply {ply} is past the end of the game")));
        }
        let mut anim = Vec::new();
        if self.turn.take().is_none() {
            if ply == self.cursor + 1 {
                anim = self.move_animation(self.cursor);
            } else if ply + 1 == self.cursor {
                anim = self.move_animation(ply).into_iter().rev().map(AnimStep::reversed).collect();
            }
        }
        self.cursor = ply;
        self.refresh();
        Ok(anim)
    }

    /// The move that produced the position at the cursor, for replaying it:
    /// the pieces before it and its animation. `None` at the start and after
    /// a setup.
    pub fn move_replay(&self) -> Option<MoveReplay> {
        let ply = self.cursor.checked_sub(1)?;
        let animation = self.move_animation(ply);
        if animation.is_empty() {
            return None;
        }
        let before = self.game.position_at(ply)?;
        Some(MoveReplay { before: position_view(before, &self.ids[ply]).pieces, animation })
    }

    /// Forward animation of the move made from `ply`.
    fn move_animation(&self, ply: usize) -> Vec<AnimStep> {
        match &self.game.moves()[ply] {
            Move::Steps(effects) => {
                let mut map = self.ids[ply];
                effects.iter().map(|e| apply_ids(&mut map, e)).collect()
            }
            Move::Setup(_) => Vec::new(),
        }
    }

    /// Animation of the in-progress turn's steps so far, plus the resulting ids.
    fn turn_animation(&self) -> (Vec<AnimStep>, IdMap) {
        let mut map = self.ids[self.cursor];
        let anim = match &self.turn {
            Some(tb) => tb.steps().iter().map(|s| apply_ids(&mut map, &s.effect)).collect(),
            None => Vec::new(),
        };
        (anim, map)
    }

    fn turn_builder(&self) -> Result<TurnBuilder, ApiError> {
        if let Some(tb) = &self.turn {
            return Ok(tb.clone());
        }
        if Game::is_setup_ply(self.cursor) {
            return Err(ApiError::state("the setup must be made first"));
        }
        if self.tree[self.cursor_node()].is_terminal() {
            return Err(ApiError::state("the game is over"));
        }
        Ok(TurnBuilder::new(self.cursor_position()))
    }

    pub fn set_continue_turns(&mut self, on: bool) {
        self.continue_turns = on;
    }

    /// The in-progress turn, finished, when the next step starts the other
    /// side's turn: it has used all its steps, it could be committed, and
    /// it doesn't end the game.
    fn full_turn(&self) -> Option<Turn> {
        let tb = self.turn.as_ref().filter(|tb| self.continue_turns && tb.steps_left() == 0)?;
        let turn = tb.clone().finish().ok()?;
        if self.game.is_third_repetition(self.cursor, &turn.end)
            || outcome_after_turn(&turn.end, turn.start.side_to_move()).is_some()
        {
            return None;
        }
        Some(turn)
    }

    /// The turn the next step goes into, and the full turn that has to be
    /// finished first if it starts the other side's turn.
    fn input_builder(&self) -> Result<(TurnBuilder, Option<Turn>), ApiError> {
        match self.full_turn() {
            Some(turn) => Ok((TurnBuilder::new(&turn.end), Some(turn))),
            None => Ok((self.turn_builder()?, None)),
        }
    }

    /// Adds a full turn to the tree before the next turn starts. It never
    /// plays a move in a match: at the live node it's a plan.
    fn finish_full_turn(&mut self, turn: Turn) -> Result<(), ApiError> {
        let node = self.tree.add_turn(self.cursor_node(), turn).map_err(ApiError::illegal)?;
        self.turn = None;
        self.show(node);
        Ok(())
    }

    /// Takes one step. With `continue_turns` on, a step after a full turn
    /// finishes it (as a plan when it's the user's move in a match, which
    /// only `commit_turn` plays) and starts the other side's turn.
    pub fn try_step(&mut self, from: Square, to: Square) -> Result<Vec<AnimStep>, ApiError> {
        self.require_input()?;
        let (mut tb, full) = self.input_builder()?;
        tb.try_move(from, to).map_err(ApiError::illegal)?;
        if let Some(turn) = full {
            self.finish_full_turn(turn)?;
        }
        self.turn = Some(tb);
        let (anim, _) = self.turn_animation();
        Ok(anim.last().copied().into_iter().collect())
    }

    /// The route a drop from `from` on `to` would take: the shortest walk of
    /// that piece, following as many squares of the dragged `path` as it can.
    /// A drop on an adjacent square is a single step of any kind (including
    /// pushes and pulls), as in [`Session::try_step`].
    fn route(&self, tb: &TurnBuilder, from: Square, to: Square, path: &[Square]) -> Option<Vec<Step>> {
        if from.direction_to(to).is_some() {
            let piece = tb.position().piece_at(from)?;
            let step = Step::new(piece, from, from.direction_to(to)?);
            return tb.classify(step).is_ok().then(|| vec![step]);
        }
        Route::best_along(tb.shortest_routes(from, to), path).map(|r| r.steps)
    }

    /// Squares the piece would enter if dropped on `to` (see [`Session::try_route`]).
    pub fn plan_route(&self, from: Square, to: Square, path: &[Square]) -> Option<Vec<Square>> {
        if !self.can_input() || self.setup_draft.is_some() {
            return None;
        }
        let (tb, _) = self.input_builder().ok()?;
        let steps = self.route(&tb, from, to, path)?;
        Some(steps.iter().filter_map(Step::to).collect())
    }

    /// Moves the piece on `from` to `to` by the route [`Session::plan_route`] shows.
    pub fn try_route(
        &mut self,
        from: Square,
        to: Square,
        path: &[Square],
    ) -> Result<Vec<AnimStep>, ApiError> {
        self.require_input()?;
        let (mut tb, full) = self.input_builder()?;
        let Some(steps) = self.route(&tb, from, to, path) else {
            return Err(ApiError::illegal("that piece can't get there this turn"));
        };
        for &step in &steps {
            tb.try_step(step).map_err(ApiError::illegal)?;
        }
        if let Some(turn) = full {
            self.finish_full_turn(turn)?;
        }
        self.turn = Some(tb);
        let (anim, _) = self.turn_animation();
        Ok(anim[anim.len() - steps.len()..].to_vec())
    }

    /// Undoes the last step. With no step to undo, it reopens the move that
    /// led to the shown position (the move stays in the tree) and undoes its
    /// last step; at the live node of a match that's a takeback.
    pub fn undo_step(&mut self) -> Result<Vec<AnimStep>, ApiError> {
        if self.turn.is_none() {
            if self.matchup.is_none() || self.cursor_node() != self.live() {
                return self.reopen();
            }
            let target = self.check_take_back()?;
            if !self.matchup.as_ref().is_some_and(|m| m.players.contains(&Player::Human)) {
                return Ok(self.take_back_to(target));
            }
            // Land in the human's move at `target`, minus its last step.
            let mut anim = self.back_animation(target + 1);
            self.make_live_again(target);
            self.cursor = target + 1;
            anim.extend(self.reopen()?);
            return Ok(anim);
        }
        let (anim, _) = self.turn_animation();
        let tb = self.turn.as_mut().unwrap();
        tb.undo();
        if tb.steps().is_empty() {
            self.turn = None;
        }
        Ok(anim.last().map(|a| a.reversed()).into_iter().collect())
    }

    /// The move into the shown position as a turn in progress from the
    /// position before it, minus its last step; the cursor moves back to
    /// that position. The move stays in the tree.
    fn reopen(&mut self) -> Result<Vec<AnimStep>, ApiError> {
        let Some(Move::Steps(effects)) = self.tree[self.cursor_node()].mv().cloned() else {
            return Err(ApiError::state("no step to undo"));
        };
        let ply = self.cursor - 1;
        let anim: Vec<AnimStep> = self.move_animation(ply).last().map(|a| a.reversed()).into_iter().collect();
        let mut tb = TurnBuilder::new(self.tree[self.line[ply]].position());
        for e in &effects[..effects.len() - 1] {
            tb.try_step(e.step).map_err(ApiError::illegal)?;
        }
        self.show(self.line[ply]);
        self.turn = (!tb.steps().is_empty()).then_some(tb);
        Ok(anim)
    }

    /// Whether `undo_step` has something to undo.
    fn can_undo(&self) -> bool {
        if self.turn.is_some() {
            return true;
        }
        if !matches!(self.tree[self.cursor_node()].mv(), Some(Move::Steps(_))) {
            return false;
        }
        self.matchup.is_none() || self.cursor_node() != self.live() || self.take_back_target().is_some()
    }

    /// The ply a takeback goes back to: the latest position before the live
    /// one with a human to move (skipping engine moves), or one ply back
    /// when engines play both sides. Never into the setups.
    fn take_back_target(&self) -> Option<usize> {
        let m = self.matchup.as_ref().filter(|m| m.takebacks)?;
        if self.live_result().is_some() || self.cursor_node() != m.live {
            return None;
        }
        let live_ply = self.tree[m.live].ply();
        let human = m.players.contains(&Player::Human);
        let side = |ply: usize| if ply.is_multiple_of(2) { Color::Gold } else { Color::Silver };
        (2..live_ply).rev().find(|&p| !human || *self.player(side(p)) == Player::Human)
    }

    fn check_take_back(&self) -> Result<usize, ApiError> {
        if !self.matchup.as_ref().is_some_and(|m| m.takebacks) {
            return Err(ApiError::state("takebacks are off in this game"));
        }
        self.take_back_target().ok_or_else(|| ApiError::state("there's no move to take back"))
    }

    /// Takes back played moves to [`Session::take_back_target`]: the whole
    /// move, with no turn in progress. They stay in the tree, as a
    /// variation once a different move is played.
    pub fn take_back(&mut self) -> Result<Vec<AnimStep>, ApiError> {
        let target = self.check_take_back()?;
        Ok(self.take_back_to(target))
    }

    /// Makes the node at `target` on the shown line live and shows it,
    /// returning the animation back to it.
    fn take_back_to(&mut self, target: usize) -> Vec<AnimStep> {
        self.turn = None;
        let anim = self.back_animation(target);
        self.make_live_again(target);
        self.show(self.line[target]);
        anim
    }

    /// The moves from the cursor back to ply `target`, played backward.
    fn back_animation(&self, target: usize) -> Vec<AnimStep> {
        (target..self.cursor)
            .rev()
            .flat_map(|p| self.move_animation(p).into_iter().rev().map(AnimStep::reversed))
            .collect()
    }

    /// Makes the node at `target` on the shown line the live one again, with
    /// the clocks as they were when its turn began.
    fn make_live_again(&mut self, target: usize) {
        let node = self.line[target];
        let Some(m) = &mut self.matchup else { return };
        let now = Instant::now();
        m.live = node;
        m.thinking = None;
        m.turn_started = now;
        let Some(start) = m.turn_starts.get(&node).copied() else { return };
        if let (Some(clock), Some(reserves)) = (&mut m.clock, start.reserves) {
            clock.reserves = reserves;
            clock.game_started = now.checked_sub(start.elapsed).unwrap_or(now);
        }
    }

    pub fn cancel_turn(&mut self) -> Vec<AnimStep> {
        let (anim, _) = self.turn_animation();
        self.turn = None;
        anim.into_iter().rev().map(AnimStep::reversed).collect()
    }

    /// Adds the in-progress turn to the game: as a played move at the live
    /// node of a match on a human's turn (unless `plan` is set), otherwise
    /// as a new branch (or the existing move, if it's already there). With
    /// no turn in progress, plays the next move of the shown line when it's
    /// a plan for the user's move.
    pub fn commit_turn(&mut self, plan: bool) -> Result<(), ApiError> {
        let Some(tb) = self.turn.take() else {
            if plan {
                return Err(ApiError::state("no turn in progress"));
            }
            return self.play_plan();
        };
        if let Err(e) = tb.can_finish() {
            self.turn = Some(tb);
            return Err(ApiError::illegal(e));
        }
        let turn = tb.clone().finish().expect("checked above");
        if self.game.is_third_repetition(self.cursor, &turn.end) {
            self.turn = Some(tb);
            return Err(ApiError::illegal(GameError::Repetition));
        }
        if let Err(e) = self.require_input() {
            self.turn = Some(tb);
            return Err(e);
        }
        let (mover, ply, parent) = (turn.start.side_to_move(), self.cursor, self.cursor_node());
        if plan || !self.plays_live() {
            let node = self.tree.add_turn(parent, turn).map_err(ApiError::illegal)?;
            self.show(node);
            return Ok(());
        }
        if self.clock_move(Instant::now()) {
            let node = self.tree.add_turn(parent, turn).map_err(ApiError::illegal)?;
            self.play_live(node);
            self.after_move(mover, ply);
            self.show(node);
        }
        Ok(())
    }

    /// The move `commit_turn` would play with no turn in progress: the one
    /// after the live node on the shown line, when the cursor is past it
    /// and it's the user's move (a plan entered earlier).
    fn plan_to_play(&self) -> Option<NodeId> {
        let live = self.live();
        let ply = self.tree[live].ply();
        if self.matchup.is_none()
            || self.cursor <= ply
            || Game::is_setup_ply(ply)
            || self.live_result().is_some()
            || *self.player(self.live_side()) != Player::Human
        {
            return None;
        }
        self.line.get(ply + 1).copied().filter(|_| self.line.get(ply) == Some(&live))
    }

    /// Plays [`Session::plan_to_play`].
    fn play_plan(&mut self) -> Result<(), ApiError> {
        let Some(node) = self.plan_to_play() else { return Err(ApiError::state("no turn in progress")) };
        let (mover, ply) = (self.live_side(), self.tree[self.live()].ply());
        if self.clock_move(Instant::now()) {
            self.play_live(node);
            self.after_move(mover, ply);
            self.refresh();
        }
        Ok(())
    }

    /// Swaps two pieces in the setup draft.
    pub fn setup_swap(&mut self, a: Square, b: Square) -> Result<Vec<AnimStep>, ApiError> {
        self.require_input()?;
        let Some(draft) = &mut self.setup_draft else {
            return Err(ApiError::state("no setup is being arranged"));
        };
        let ia = draft.iter().position(|p| p.square == a);
        let ib = draft.iter().position(|p| p.square == b);
        let (Some(ia), Some(ib)) = (ia, ib) else {
            return Err(ApiError::illegal("pieces can only be placed on your two home ranks"));
        };
        if ia == ib {
            return Ok(Vec::new());
        }
        draft[ia].square = b;
        draft[ib].square = a;
        let base = id_base(draft[ia].piece.color);
        let slide = |i: usize, from, to| AnimStep {
            id: base + i as PieceId,
            from,
            to,
            captured: None,
            restored: None,
        };
        Ok(vec![slide(ia, a, b), slide(ib, b, a)])
    }

    pub fn commit_setup(&mut self) -> Result<(), ApiError> {
        self.require_input()?;
        let Some(draft) = self.setup_draft.take() else {
            return Err(ApiError::state("no setup is being arranged"));
        };
        let parent = self.cursor_node();
        if !self.plays_live() {
            let node = self.tree.add_setup(parent, draft).map_err(ApiError::illegal)?;
            self.show(node);
        } else if self.clock_move(Instant::now()) {
            let node = self.tree.add_setup(parent, draft).map_err(ApiError::illegal)?;
            self.play_live(node);
            self.show(node);
        }
        Ok(())
    }

    /// Where the piece on `from` may go next, for drag hints. The core still
    /// validates the actual step.
    pub fn legal_targets(&self, from: Square) -> Vec<StepTarget> {
        if !self.can_input() {
            return Vec::new();
        }
        if let Some(draft) = &self.setup_draft {
            if !draft.iter().any(|p| p.square == from) {
                return Vec::new();
            }
            return draft
                .iter()
                .filter(|p| p.square != from)
                .map(|p| StepTarget { to: p.square, kind: None, steps: 1 })
                .collect();
        }
        let Ok((tb, _)) = self.input_builder() else { return Vec::new() };
        let mut targets: Vec<StepTarget> = tb
            .legal_steps_from(from)
            .into_iter()
            .filter_map(|(step, kind)| step.to().map(|to| StepTarget { to, kind: Some(kind), steps: 1 }))
            .collect();
        for (to, steps) in tb.reachable(from) {
            if steps > 1 {
                targets.push(StepTarget { to, kind: Some(StepKind::Simple), steps: steps as u8 });
            }
        }
        targets
    }

    /// Why the in-progress turn can't be committed, if it can't.
    fn commit_blocker(&self, tb: &TurnBuilder) -> Option<String> {
        if let Err(e) = tb.can_finish() {
            return Some(e.to_string());
        }
        let mut end = tb.position().clone();
        end.set_side_to_move(end.side_to_move().opponent());
        self.game
            .is_third_repetition(self.cursor, &end)
            .then(|| howdah_arimaa::GameError::Repetition.to_string())
    }

    fn phase(&self) -> Phase {
        if Game::is_setup_ply(self.cursor) {
            Phase::Setup
        } else if self.tree[self.cursor_node()].is_terminal() {
            Phase::Over
        } else {
            Phase::Play
        }
    }

    pub fn view(&self) -> SessionView {
        let moves = self
            .game
            .moves()
            .iter()
            .enumerate()
            .map(|(ply, m)| MoveView {
                ply: ply + 1,
                label: notation::move_label(ply),
                notation: m.notation(),
            })
            .collect();

        let (position, ids) = match (&self.turn, &self.setup_draft) {
            (Some(tb), _) => (tb.position().clone(), self.turn_animation().1),
            (None, Some(draft)) => {
                let mut pos = self.cursor_position().clone();
                let mut ids = self.ids[self.cursor];
                for (i, p) in draft.iter().enumerate() {
                    pos.set(p.square, Some(p.piece));
                    ids[p.square.index() as usize] = Some(id_base(p.piece.color) + i as PieceId);
                }
                (pos, ids)
            }
            (None, None) => (self.cursor_position().clone(), self.ids[self.cursor]),
        };

        let turn = self.turn.as_ref().map(|tb| TurnView {
            steps: tb
                .steps()
                .iter()
                .map(|s| TurnStepView { notation: s.effect.to_string(), kind: s.kind })
                .collect(),
            steps_left: tb.steps_left(),
            push_pending: tb.push_pending(),
            commit_blocker: self.commit_blocker(tb),
        });

        SessionView {
            moves,
            tree: self.tree_view(),
            game_comment: self.tree[GameTree::ROOT].annotation().comment.clone(),
            cursor: self.cursor_node(),
            live: self.matchup.as_ref().map(|m| m.live),
            ply: self.cursor,
            phase: self.phase(),
            position: position_view(&position, &ids),
            turn,
            last_move: self.last_move_view(),
            captured: self.captured_view(),
            moves_after_cursor: self.game.ply_count() - self.cursor,
            result: self.tree[self.cursor_node()].result(),
            end_marker: self.tree[self.line_end()].end_marker().map(str::to_string),
            end_detail: self.end_detail.clone(),
            players: self
                .matchup
                .as_ref()
                .map(|m| PlayersView { gold: m.players[0].view(), silver: m.players[1].view() }),
            clock: self.clock_view(),
            thinking: self.matchup.as_ref().and_then(|m| m.thinking),
            can_input: self.can_input(),
            plays_live: self.plays_live(),
            live_move: self.matchup.as_ref().and_then(|m| {
                let n = &self.tree[m.live];
                Some(format!("{} {}", notation::move_label(n.ply().checked_sub(1)?), n.mv()?.notation()))
            }),
            can_undo: self.can_undo(),
            can_take_back: self.take_back_target().is_some(),
            plan_move: self.plan_to_play().and_then(|n| {
                let mv = self.tree[n].mv()?;
                Some(format!("{} {}", notation::move_label(self.tree[n].ply() - 1), mv.notation()))
            }),
            live_ply: self.matchup.as_ref().and_then(|m| self.line.iter().position(|&n| n == m.live)),
            analysis_engine: self.analysis.as_ref().map(|a| a.id.clone()),
            stored_analysis: self.evals.get(&self.cursor_node()).cloned(),
        }
    }

    /// The game tree in display order, as the record format writes it.
    fn tree_view(&self) -> Vec<MoveNodeView> {
        let mut out = Vec::new();
        self.tree_view_from(GameTree::ROOT, 0, &mut out);
        out
    }

    /// Adds the moves after `node` on its line, each followed by the
    /// variations that replace it.
    fn tree_view_from(&self, mut node: NodeId, depth: usize, out: &mut Vec<MoveNodeView>) {
        let tree = &self.tree;
        loop {
            let children = tree[node].children();
            if tree[node].result().is_some() {
                // After the game's end, every move is a continuation.
                for &c in children {
                    self.variation_view(c, depth + 1, out);
                }
                return;
            }
            let Some(&main) = children.first() else { return };
            out.push(self.node_view(main, depth));
            for &v in &children[1..] {
                self.variation_view(v, depth + 1, out);
            }
            node = main;
        }
    }

    fn variation_view(&self, first: NodeId, depth: usize, out: &mut Vec<MoveNodeView>) {
        let start = out.len();
        out.push(self.node_view(first, depth));
        out[start].starts_variation = true;
        self.tree_view_from(first, depth, out);
        let rest = (out.len() - start - 1) as u32;
        out[start].collapsible = rest > 0;
        // A folded variation still opens while the board shows a move inside it.
        let inside = self.line[..self.cursor].contains(&first);
        if rest > 0 && self.collapsed.contains(&first) && !inside {
            out.truncate(start + 1);
            out[start].folded = rest;
        }
        out.last_mut().expect("the first move").closes += 1;
    }

    fn node_view(&self, id: NodeId, depth: usize) -> MoveNodeView {
        let node = &self.tree[id];
        let annotation = node.annotation();
        MoveNodeView {
            id,
            parent: node.parent().expect("only the root has no parent"),
            ply: node.ply(),
            depth,
            starts_variation: false,
            closes: 0,
            label: notation::move_label(node.ply() - 1),
            notation: node.mv().map(Move::notation).unwrap_or_default(),
            glyphs: annotation.glyphs.iter().map(ToString::to_string).collect(),
            comment: annotation.comment.clone(),
            intro: annotation.intro.clone(),
            on_line: self.line.contains(&id),
            result: node.result(),
            collapsible: false,
            folded: 0,
        }
    }

    fn last_move_view(&self) -> Option<LastMoveView> {
        let ply = self.cursor.checked_sub(1)?;
        let color = self.game.position_at(ply)?.side_to_move();
        steps_view(color, &self.game.moves()[ply])
    }

    fn captured_view(&self) -> CapturedView {
        let played = self.game.moves()[..self.cursor].iter().flat_map(|m| match m {
            Move::Steps(steps) => steps.as_slice(),
            Move::Setup(_) => &[],
        });
        let entering = self.turn.iter().flat_map(|tb| tb.steps().iter().map(|s| &s.effect));
        let mut view = CapturedView::default();
        for c in played.chain(entering).filter_map(|e| e.capture) {
            match c.piece.color {
                Color::Gold => view.gold.push(c.piece.kind),
                Color::Silver => view.silver.push(c.piece.kind),
            }
        }
        view.gold.sort_by(|a, b| b.cmp(a));
        view.silver.sort_by(|a, b| b.cmp(a));
        view
    }

    fn clock_view(&self) -> Option<ClockView> {
        let m = self.matchup.as_ref()?;
        let clock = m.clock.as_ref()?;
        let ms = |d: Duration| d.as_millis() as u64;
        let running =
            self.live_result().is_none().then(|| self.live_side()).filter(|s| clock.tc(*s).is_some());
        let (elapsed, allowance) = match running {
            Some(side) => {
                let tc = clock.tc(side).expect("running side is timed");
                (m.turn_started.elapsed(), tc.turn_allowance(clock.reserves[side.index()]))
            }
            None => (Duration::ZERO, Duration::ZERO),
        };
        let side_view = |side: Color| {
            clock.tc(side).map(|tc| SideClockView {
                time_control: tc.to_string(),
                move_time_ms: ms(tc.move_time()),
                reserve_ms: ms(clock.reserves[side.index()]),
            })
        };
        Some(ClockView {
            gold: side_view(Color::Gold),
            silver: side_view(Color::Silver),
            running,
            turn_elapsed_ms: ms(elapsed),
            turn_allowance_ms: ms(allowance),
            game_remaining_ms: clock.game_deadline().map(|d| ms(d.saturating_duration_since(Instant::now()))),
        })
    }
}

fn position_view(pos: &Position, ids: &IdMap) -> PositionView {
    let pieces = pos
        .pieces()
        .map(|(sq, piece)| PieceView {
            // Fallback ids can't collide with real ones (0..32).
            id: ids[sq.index() as usize].unwrap_or(1000 + sq.index() as PieceId),
            piece,
            square: sq,
            frozen: pos.is_frozen(sq),
        })
        .collect();
    PositionView { side_to_move: pos.side_to_move(), pieces, short: pos.to_short_string() }
}

#[cfg(test)]
mod tests {
    use super::*;

    const SAMPLE: &str = include_str!("../../../crates/howdah-arimaa/tests/data/sample_game.txt");

    fn sq(s: &str) -> Square {
        s.parse().unwrap()
    }

    fn id_at(view: &SessionView, s: &str) -> PieceId {
        view.position.pieces.iter().find(|p| p.square == sq(s)).unwrap().id
    }

    #[test]
    fn new_session_starts_with_gold_setup_draft() {
        let s = Session::new();
        let v = s.view();
        assert_eq!(v.phase, Phase::Setup);
        assert_eq!(v.position.pieces.len(), 16);
    }

    #[test]
    fn setup_swap_and_commit() {
        let mut s = Session::new();
        let e = id_at(&s.view(), "e2");
        let anim = s.setup_swap(sq("e2"), sq("a1")).unwrap();
        assert_eq!(anim[0].id, e);
        assert_eq!(id_at(&s.view(), "a1"), e);
        assert!(s.setup_swap(sq("a1"), sq("a3")).is_err());
        s.commit_setup().unwrap();
        assert_eq!(s.view().position.pieces.len(), 32, "gold placed plus silver draft");
        // Ids survive the commit.
        assert_eq!(id_at(&s.view(), "a1"), e);
        s.commit_setup().unwrap();
        assert_eq!(s.view().phase, Phase::Play);
    }

    #[test]
    fn steps_keep_ids_and_commit() {
        let mut s = Session::new();
        s.commit_setup().unwrap();
        s.commit_setup().unwrap();
        let e = id_at(&s.view(), "e2");
        let anim = s.try_step(sq("e2"), sq("e3")).unwrap();
        assert_eq!(
            anim,
            vec![AnimStep { id: e, from: sq("e2"), to: sq("e3"), captured: None, restored: None }]
        );
        assert_eq!(id_at(&s.view(), "e3"), e);
        assert!(s.try_step(sq("a1"), sq("a2")).is_err());
        assert_eq!(s.view().turn.unwrap().steps.len(), 1);
        s.commit_turn(false).unwrap();
        assert_eq!(s.view().moves.last().unwrap().notation, "Ee2n");
        assert_eq!(id_at(&s.view(), "e3"), e);
    }

    #[test]
    fn route_follows_the_dragged_path() {
        let mut s = Session::new();
        s.commit_setup().unwrap();
        s.commit_setup().unwrap();
        let squares = |v: Vec<Square>| v.iter().map(|q| q.to_string()).collect::<Vec<_>>().join(" ");
        let plan = |s: &Session, path: &[&str]| {
            let path: Vec<Square> = path.iter().map(|p| sq(p)).collect();
            s.plan_route(sq("e2"), sq("f4"), &path).map(squares)
        };
        assert_eq!(plan(&s, &["e3", "f3", "f4"]).as_deref(), Some("e3 f3 f4"));
        assert_eq!(plan(&s, &["e3", "e4", "f4"]).as_deref(), Some("e3 e4 f4"));
        assert_eq!(s.plan_route(sq("e2"), sq("e7"), &[]), None, "five steps away");

        let e = id_at(&s.view(), "e2");
        let anim = s.try_route(sq("e2"), sq("f4"), &[sq("e3"), sq("e4"), sq("f4")]).unwrap();
        assert_eq!(anim.iter().map(|a| a.to).collect::<Vec<_>>(), vec![sq("e3"), sq("e4"), sq("f4")]);
        assert!(anim.iter().all(|a| a.id == e));
        assert_eq!(id_at(&s.view(), "f4"), e);
        assert!(s.try_route(sq("f4"), sq("f7"), &[]).is_err(), "only one step left");
        assert_eq!(s.try_route(sq("f4"), sq("g4"), &[]).unwrap().len(), 1);
    }

    #[test]
    fn multi_step_targets_have_distances() {
        let mut s = Session::new();
        s.commit_setup().unwrap();
        s.commit_setup().unwrap();
        let targets = s.legal_targets(sq("e2"));
        let steps = |q: &str| targets.iter().find(|t| t.to == sq(q)).map(|t| t.steps);
        assert_eq!(steps("e3"), Some(1));
        assert_eq!(steps("e5"), Some(3));
        assert_eq!(steps("e7"), None);
    }

    #[test]
    fn net_null_turn_cannot_commit() {
        let mut s = Session::new();
        s.commit_setup().unwrap();
        s.commit_setup().unwrap();
        s.try_step(sq("e2"), sq("e3")).unwrap();
        s.try_step(sq("e3"), sq("e2")).unwrap();
        assert!(s.view().turn.unwrap().commit_blocker.is_some());
        assert!(s.commit_turn(false).is_err());
        assert!(s.view().turn.is_some(), "failed commit keeps the turn");
        let anim = s.undo_step().unwrap();
        assert_eq!((anim[0].from, anim[0].to), (sq("e2"), sq("e3")));
        s.cancel_turn();
        assert!(s.view().turn.is_none());
    }

    #[test]
    fn navigation_animates_single_moves() {
        let mut s = Session::new();
        s.load(SAMPLE).unwrap();
        assert_eq!(s.view().ply, 10);
        // 4g hg3w hf3x Eg4s Eg3n: the horse is captured on f3.
        s.goto(6).unwrap();
        let fwd = s.goto(7).unwrap();
        assert_eq!(fwd.len(), 3);
        assert_eq!(fwd[0].captured.unwrap().square, sq("f3"));
        let back = s.goto(6).unwrap();
        assert_eq!(back.len(), 3);
        assert_eq!(back[2].restored.unwrap().square, sq("f3"));
        assert_eq!((back[0].from, back[0].to), (sq("g4"), sq("g3")));
        assert!(s.goto(2).unwrap().is_empty(), "jumps snap");
        assert!(s.goto(11).is_err());
    }

    #[test]
    fn move_from_earlier_ply_adds_a_branch() {
        let mut s = Session::new();
        s.load(SAMPLE).unwrap();
        s.goto(2).unwrap();
        assert_eq!(s.view().moves_after_cursor, 8);
        s.try_step(sq("a2"), sq("a3")).unwrap();
        s.commit_turn(false).unwrap();
        // The new branch is shown; the game is still there.
        let v = s.view();
        assert_eq!((v.ply, v.moves.len()), (3, 3));
        assert_eq!(v.moves[2].notation, "Ha2n");
        assert_eq!(s.tree.main_line().len(), 11);
        let record = s.export(false);
        assert!(record.contains("(\n2g Ha2n\n)"), "{record}");
        // Playing the game's own move (2g Ee2n Ee3n Ee4n Ee5e) goes back
        // onto the main line instead of adding a copy.
        s.goto(2).unwrap();
        s.try_route(sq("e2"), sq("e5"), &[]).unwrap();
        s.try_step(sq("e5"), sq("f5")).unwrap();
        s.commit_turn(false).unwrap();
        let v = s.view();
        assert_eq!((v.ply, v.moves.len()), (3, 10), "back on the game's line");
        assert_eq!(s.tree[s.tree.main_line()[2]].children().len(), 2);
    }

    /// The sample game with a variation for 2g (Ha2n) and one inside it.
    fn with_variations() -> Session {
        let mut s = Session::new();
        s.load(SAMPLE).unwrap();
        s.goto(2).unwrap();
        s.try_step(sq("a2"), sq("a3")).unwrap();
        s.commit_turn(false).unwrap();
        s.try_step(sq("h7"), sq("h6")).unwrap();
        s.commit_turn(false).unwrap();
        s.goto(3).unwrap();
        s.try_step(sq("a7"), sq("a6")).unwrap();
        s.commit_turn(false).unwrap();
        s
    }

    #[test]
    fn tree_view_lists_variations_after_the_move_they_replace() {
        let s = with_variations();
        let v = s.view();
        let rows: Vec<String> = v
            .tree
            .iter()
            .map(|m| {
                let mark = if m.starts_variation { "(" } else { "" };
                let end = ")".repeat(m.closes as usize);
                format!(
                    "{}{mark}{} {}{end}",
                    "  ".repeat(m.depth),
                    m.label,
                    m.notation.split(' ').next().unwrap()
                )
            })
            .collect();
        assert_eq!(rows[2..6], ["2g Ee2n", "  (2g Ha2n", "  2s hh7s", "    (2s ha7s))"], "{rows:#?}");
        assert_eq!(rows[6], "2s hh7s");
        assert_eq!(v.cursor, s.cursor_node());
        let shown: Vec<_> = v.tree.iter().filter(|m| m.on_line).map(|m| m.notation.as_str()).collect();
        assert_eq!(shown.len(), 4);
        assert_eq!(shown[2..], ["Ha2n", "ha7s"]);
    }

    #[test]
    fn goto_node_switches_lines() {
        let mut s = with_variations();
        let main_2s = s.tree.main_line()[4];
        let anim = s.goto_node(main_2s).unwrap();
        assert!(anim.is_empty(), "a jump to another line snaps");
        let v = s.view();
        assert_eq!((v.ply, v.moves.len()), (4, 10));
        // A move forward off the line animates.
        let alt = s.tree[s.tree.main_line()[2]].children()[1];
        s.goto(2).unwrap();
        let anim = s.goto_node(alt).unwrap();
        assert_eq!(anim.len(), 1);
        assert_eq!(s.view().moves[2].notation, "Ha2n");
        assert!(s.goto_node(NodeId(999)).is_err());
    }

    #[test]
    fn line_edits() {
        let mut s = with_variations();
        let alt = s.tree[s.tree.main_line()[2]].children()[1];
        s.promote(alt).unwrap();
        assert_eq!(s.tree.main_line()[3], alt);
        s.demote(alt).unwrap();
        s.make_main_line(s.cursor_node()).unwrap();
        assert_eq!(s.tree.main_line().len(), 5);
        // Deleting the shown move moves the board back to its parent.
        let shown = s.cursor_node();
        let parent = s.tree[shown].parent().unwrap();
        s.delete_from(shown).unwrap();
        assert_eq!(s.cursor_node(), parent);
        assert!(s.delete_from(GameTree::ROOT).is_err());
    }

    #[test]
    fn returning_to_a_move_follows_the_line_last_shown() {
        let mut s = with_variations();
        let alt = s.tree[s.tree.main_line()[2]].children()[1];
        let deep = s.cursor_node();
        s.goto_node(s.tree.main_line()[4]).unwrap();
        assert_eq!(s.line, s.tree.main_line());
        // Back to the variation: its deeper line, not its main continuation.
        s.goto_node(alt).unwrap();
        assert_eq!(s.line_end(), deep);
        assert_eq!(s.view().moves[3].notation.split(' ').next(), Some("ha7s"));
    }

    #[test]
    fn sibling_and_branch_navigation() {
        let mut s = with_variations();
        let main = s.tree.main_line();
        let alt = s.tree[main[2]].children()[1];
        let deep = s.cursor_node();
        s.goto_node(alt).unwrap();
        s.goto_sibling(-1).unwrap();
        assert_eq!(s.cursor_node(), main[3]);
        s.goto_sibling(-1).unwrap();
        assert_eq!(s.cursor_node(), main[3], "no earlier alternative");
        s.goto_sibling(1).unwrap();
        assert_eq!((s.cursor_node(), s.line_end()), (alt, deep));

        s.goto_node(main[4]).unwrap();
        s.goto(0).unwrap();
        s.goto_branch(true).unwrap();
        assert_eq!(s.cursor, 3, "the first move with alternatives");
        s.goto_branch(true).unwrap();
        assert_eq!(s.cursor, main.len() - 1, "no more: the end of the line");
        s.goto_branch(false).unwrap();
        assert_eq!(s.cursor, 3);
        s.goto_branch(false).unwrap();
        assert_eq!(s.cursor, 0);
    }

    #[test]
    fn comments_and_glyphs() {
        let mut s = with_variations();
        let alt = s.tree[s.tree.main_line()[2]].children()[1];
        let row = |s: &Session| s.view().tree.into_iter().find(|m| m.id == alt).unwrap();
        s.set_comment(alt, "Try this.\n").unwrap();
        assert_eq!(row(&s).comment.as_deref(), Some("Try this."));
        s.set_comment(alt, "  ").unwrap();
        assert_eq!(row(&s).comment, None);
        s.set_comment(GameTree::ROOT, "A game").unwrap();
        assert_eq!(s.view().game_comment.as_deref(), Some("A game"));
        s.toggle_glyph(alt, Glyph::GOOD).unwrap();
        s.toggle_glyph(alt, Glyph::MISTAKE).unwrap();
        assert_eq!(row(&s).glyphs, ["?"], "one move glyph replaces another");
        s.toggle_glyph(alt, Glyph::MISTAKE).unwrap();
        assert!(row(&s).glyphs.is_empty());
        assert!(s.toggle_glyph(GameTree::ROOT, Glyph::GOOD).is_err());
        assert!(s.set_comment(NodeId(999), "x").is_err());
        assert!(s.export(false).contains("{A game}"));
        assert_eq!(s.export(true), s.tree.main_game().to_record());
    }

    #[test]
    fn folding_variations() {
        let mut s = with_variations();
        let main = s.tree.main_line();
        let alt = s.tree[main[2]].children()[1];
        assert!(s.toggle_collapsed(main[3]).is_err(), "main line moves don't fold");
        s.toggle_collapsed(alt).unwrap();
        let row = |s: &Session| s.view().tree.into_iter().find(|m| m.id == alt).unwrap();
        assert_eq!((row(&s).collapsible, row(&s).folded), (true, 0), "open while the board is inside it");
        s.goto_node(main[4]).unwrap();
        let v = s.view();
        assert_eq!(row(&s).folded, 2);
        assert_eq!(v.tree.iter().filter(|m| m.depth > 0).count(), 1, "only its first move shows");
        s.toggle_collapsed(alt).unwrap();
        assert_eq!(row(&s).folded, 0);
    }

    #[test]
    fn a_running_match_keeps_its_line() {
        let mut s = human_vs_engine_started();
        s.try_step(sq("e2"), sq("e3")).unwrap();
        s.commit_turn(false).unwrap();
        s.goto(2).unwrap();
        s.try_step(sq("d2"), sq("d3")).unwrap();
        s.commit_turn(false).unwrap();
        let plan = s.cursor_node();
        let live = s.matchup.as_ref().unwrap().live;
        assert!(s.promote(plan).is_err());
        assert!(s.make_main_line(plan).is_err());
        assert!(s.delete_from(live).is_err());
        assert!(s.demote(live).is_err());
        s.delete_from(plan).unwrap();
        // After the game ends its line can be rearranged, but not deleted.
        s.engine_failed(s.generation(), Color::Silver, "crashed".into());
        s.goto(2).unwrap();
        s.try_step(sq("d2"), sq("d3")).unwrap();
        s.commit_turn(false).unwrap();
        s.promote(s.cursor_node()).unwrap();
        assert!(s.delete_from(live).is_err());
    }

    #[test]
    fn loaded_variations_export_in_full() {
        let mut s = Session::new();
        let mut text = SAMPLE.lines().take(3).collect::<Vec<_>>().join("\n");
        text.push_str(" {first} !?\n");
        s.load(&text).unwrap();
        assert!(s.export(false).contains("{first}"));
        let mut s = Session::new();
        s.load(SAMPLE).unwrap();
        assert_eq!(s.export(false), Game::parse(SAMPLE).unwrap().to_record(), "a plain game stays plain");
    }

    #[test]
    fn capture_during_turn_entry() {
        let mut s = Session::new();
        s.load(SAMPLE).unwrap();
        s.goto(6).unwrap(); // gold to move: E on g4, silver horse on g3
        let anim = s.try_step(sq("g3"), sq("f3")).unwrap();
        assert_eq!(anim[0].captured.map(|c| c.square), Some(sq("f3")));
        assert_eq!(s.view().turn.unwrap().push_pending, Some(sq("g3")));
    }

    #[test]
    fn last_move_and_captures() {
        use howdah_arimaa::PieceKind;
        let mut s = Session::new();
        s.load(SAMPLE).unwrap();
        s.goto(2).unwrap();
        assert_eq!(s.view().last_move, None, "setups have no last move");

        // After 4g: the silver horse is pushed onto f3 and captured.
        s.goto(7).unwrap();
        let v = s.view();
        let last = v.last_move.unwrap();
        assert_eq!(last.color, Color::Gold);
        assert_eq!(last.steps.len(), 3);
        assert_eq!(last.steps[0].piece.color, Color::Silver);
        assert_eq!((last.steps[0].from, last.steps[0].to), (sq("g3"), sq("f3")));
        assert_eq!(
            last.steps[0].captured.map(|c| (c.piece.kind, c.square)),
            Some((PieceKind::Horse, sq("f3")))
        );
        assert_eq!(v.captured.silver, vec![PieceKind::Horse]);
        assert!(v.captured.gold.is_empty());

        // Before 4g nothing is captured, until the turn being entered captures.
        s.goto(6).unwrap();
        assert!(s.view().captured.silver.is_empty());
        s.try_step(sq("g3"), sq("f3")).unwrap();
        assert_eq!(s.view().captured.silver, vec![PieceKind::Horse]);
    }

    #[test]
    fn move_replay_animates_the_shown_move() {
        let mut s = Session::new();
        s.load(SAMPLE).unwrap();
        s.goto(2).unwrap();
        assert!(s.move_replay().is_none(), "setups aren't replayed");
        s.goto(7).unwrap();
        let r = s.move_replay().unwrap();
        let shown = s.view();
        // Replaying from `before` reproduces the shown pieces.
        let mut squares: std::collections::HashMap<PieceId, Square> =
            r.before.iter().map(|p| (p.id, p.square)).collect();
        for a in &r.animation {
            squares.insert(a.id, a.to);
            if let Some(c) = a.captured {
                squares.remove(&c.id);
            }
        }
        let expected: std::collections::HashMap<PieceId, Square> =
            shown.position.pieces.iter().map(|p| (p.id, p.square)).collect();
        assert_eq!(squares, expected);
    }

    #[test]
    fn legal_targets_include_pushes() {
        let mut s = Session::new();
        s.load(SAMPLE).unwrap();
        s.goto(6).unwrap(); // gold to move: E on g4, silver horse on g3
        let targets: Vec<Square> = s.legal_targets(sq("g3")).into_iter().map(|t| t.to).collect();
        assert!(targets.contains(&sq("f3")));
        assert!(targets.contains(&sq("h3")));
    }

    fn engine(name: &str) -> Player {
        Player::Engine { id: name.into(), name: name.into() }
    }

    fn setup_text(color: Color) -> String {
        notation::format_placements(&default_setup(color))
    }

    #[test]
    fn human_vs_engine_turn_taking() {
        let mut s = Session::new();
        s.start_match([Player::Human, engine("bot")], [None, None], false);
        let g = s.generation();
        let v = s.view();
        assert!(v.can_input);
        assert_eq!(v.players.unwrap().silver.name, "bot");
        assert_eq!(v.position.pieces.len(), 16, "gold's setup draft is shown");
        assert_eq!(s.engine_turn(), None);

        s.commit_setup().unwrap();
        let turn = s.engine_turn().unwrap();
        assert_eq!((turn.side, turn.ply, turn.moves.len()), (Color::Silver, 1, 1));
        assert!(!s.can_input());
        assert!(s.commit_setup().is_err(), "no draft for the engine's side");
        assert!(s.try_step(sq("a2"), sq("a3")).is_err());

        s.apply_engine_move(g, Color::Silver, 1, &setup_text(Color::Silver)).unwrap();
        assert!(s.can_input());
        s.try_step(sq("e2"), sq("e3")).unwrap();
        s.commit_turn(false).unwrap();
        assert_eq!(s.engine_turn().unwrap().ply, 3);
    }

    #[test]
    fn engine_vs_engine_has_no_draft() {
        let mut s = Session::new();
        s.start_match([engine("a"), engine("b")], [None, None], false);
        assert_eq!(s.view().position.pieces.len(), 0);
        assert!(!s.can_input());
        let t = s.engine_turn().unwrap();
        assert_eq!((t.side, t.engine_id.as_str()), (Color::Gold, "a"));
    }

    #[test]
    fn stale_and_illegal_engine_moves() {
        let mut s = Session::new();
        s.start_match([engine("a"), engine("b")], [None, None], false);
        let g = s.generation();
        assert!(s.apply_engine_move(g + 1, Color::Gold, 0, &setup_text(Color::Gold)).is_err(), "old game");
        assert!(s.apply_engine_move(g, Color::Silver, 0, &setup_text(Color::Silver)).is_err(), "wrong side");
        s.apply_engine_move(g, Color::Gold, 0, &setup_text(Color::Gold)).unwrap();
        s.apply_engine_move(g, Color::Silver, 1, &setup_text(Color::Silver)).unwrap();
        // Gold plays an illegal move and loses.
        s.apply_engine_move(g, Color::Gold, 2, "Ea1n").unwrap();
        let v = s.view();
        assert_eq!(v.result.unwrap(), GameResult { winner: Color::Silver, reason: WinReason::IllegalMove });
        assert!(v.end_detail.unwrap().contains("Ea1n"));
        assert_eq!(s.engine_turn(), None);
    }

    #[test]
    fn engine_moves_follow_only_when_watching_live() {
        let mut s = Session::new();
        s.start_match([engine("a"), engine("b")], [None, None], false);
        let g = s.generation();
        s.apply_engine_move(g, Color::Gold, 0, &setup_text(Color::Gold)).unwrap();
        s.apply_engine_move(g, Color::Silver, 1, &setup_text(Color::Silver)).unwrap();
        let anim = s.apply_engine_move(g, Color::Gold, 2, "Ee2n Ee3n").unwrap();
        assert_eq!(anim.len(), 2, "watching live: animate");
        assert_eq!(s.view().ply, 3);
        s.goto(1).unwrap();
        let anim = s.apply_engine_move(g, Color::Silver, 3, "ee7s").unwrap();
        assert!(anim.is_empty());
        assert_eq!(s.view().ply, 1, "browsing: the cursor stays");
        assert_eq!(s.view().moves.len(), 4);
    }

    fn remote(name: &str) -> Player {
        Player::Remote { name: name.into() }
    }

    /// The first `n` moves of the sample game, as a server reports them.
    fn sample_moves(n: usize) -> Vec<String> {
        SAMPLE.lines().take(n).map(|l| l.split_once(' ').unwrap().1.to_string()).collect()
    }

    #[test]
    fn remote_moves_are_played_from_the_servers_list() {
        let mut s = Session::new();
        s.start_match([remote("a"), remote("b")], [None, None], false);
        let g = s.generation();
        assert!(!s.can_input(), "no setup draft for a remote side");
        assert_eq!(s.engine_turn(), None);
        // Joining a game in progress: no animation for a batch.
        assert!(s.sync_remote(g, &sample_moves(3)).unwrap().is_empty());
        let v = s.view();
        assert_eq!((v.ply, v.moves.len()), (3, 3));
        assert_eq!(v.players.unwrap().gold.kind, PlayerKind::Remote);
        assert!(s.sync_remote(g, &sample_moves(3)).unwrap().is_empty(), "nothing new");
        // One new move while watching live is animated.
        assert_eq!(s.sync_remote(g, &sample_moves(4)).unwrap().len(), 3);
        assert_eq!(s.view().ply, 4);
        assert!(s.sync_remote(g + 1, &sample_moves(5)).is_err(), "old game");
        // Browsing: the move is played but the cursor stays.
        s.goto(2).unwrap();
        assert!(s.sync_remote(g, &sample_moves(5)).unwrap().is_empty());
        let v = s.view();
        assert_eq!((v.ply, v.live_ply), (2, Some(5)));
    }

    #[test]
    fn a_shorter_remote_list_takes_moves_back() {
        let mut s = Session::new();
        s.start_match([remote("a"), remote("b")], [None, None], false);
        let g = s.generation();
        s.sync_remote(g, &sample_moves(5)).unwrap();
        s.sync_remote(g, &sample_moves(4)).unwrap();
        let v = s.view();
        assert_eq!((v.ply, v.live_ply), (4, Some(4)));
        assert_eq!(v.moves_after_cursor, 1, "the taken-back move stays as the continuation");
        // A different move then replaces it on the main line.
        let mut moves = sample_moves(4);
        moves.push("Ef5w".into());
        s.sync_remote(g, &moves).unwrap();
        assert_eq!(s.view().moves[4].notation, "Ef5w");
        assert_eq!(s.export(true).lines().count(), 5);
    }

    #[test]
    fn an_illegal_remote_move_is_refused() {
        let mut s = Session::new();
        s.start_match([remote("a"), remote("b")], [None, None], false);
        let g = s.generation();
        let mut moves = sample_moves(2);
        moves.push("Ea1n".into());
        let e = s.sync_remote(g, &moves).unwrap_err();
        assert!(e.message.contains("server move 3"), "{}", e.message);
        let v = s.view();
        assert_eq!((v.live_ply, v.result), (Some(2), None), "the moves before it stay");
    }

    #[test]
    fn the_server_keeps_a_remote_games_clock() {
        let mut s = Session::new();
        let tc: TimeControl = "1s/0".parse().unwrap();
        s.start_match([remote("a"), remote("b")], [Some(tc); 2], false);
        let g = s.generation();
        s.sync_remote(g, &sample_moves(2)).unwrap();
        // Far past gold's allowance, but only the server ends the game.
        assert!(!s.check_timeout(Instant::now() + Duration::from_secs(60)));
        let reported = RemoteClock {
            reserves: [Duration::from_secs(30), Duration::from_secs(20)],
            turn_elapsed: Duration::from_secs(5),
            game_elapsed: None,
        };
        s.set_remote_clock(g, reported).unwrap();
        let clock = s.view().clock.unwrap();
        assert_eq!(clock.gold.unwrap().reserve_ms, 30_000);
        assert_eq!(clock.silver.unwrap().reserve_ms, 20_000);
        assert!(clock.turn_elapsed_ms >= 5_000);
        let result = GameResult { winner: Color::Silver, reason: WinReason::Timeout };
        s.finish_remote(g, result, None).unwrap();
        assert_eq!(s.view().result, Some(result));
        assert!(s.sync_remote(g, &sample_moves(3)).is_err(), "no moves after the end");
    }

    #[test]
    fn nothing_played_here_enters_a_remote_game() {
        let mut s = Session::new();
        s.start_match([Player::Human, remote("opponent")], [None, None], false);
        let g = s.generation();
        assert!(!s.plays_live() && !s.can_input(), "no setup to send yet");
        s.sync_remote(g, &sample_moves(2)).unwrap();
        s.try_step(sq("e2"), sq("e3")).unwrap();
        s.commit_turn(false).unwrap();
        let v = s.view();
        assert_eq!(v.live_ply, Some(2), "the human's move is a plan");
        assert_eq!(v.moves.len(), 3);
        let mut e = Session::new();
        e.start_match([engine("bot"), remote("opponent")], [None, None], false);
        assert_eq!(e.engine_turn(), None, "no engine moves without the server");
    }

    #[test]
    fn remote_calls_need_a_remote_game() {
        let mut s = Session::new();
        s.start_match([Player::Human, engine("bot")], [None, None], false);
        let g = s.generation();
        assert!(s.sync_remote(g, &sample_moves(1)).is_err());
        let result = GameResult { winner: Color::Gold, reason: WinReason::Resignation };
        assert!(s.finish_remote(g, result, None).is_err());
    }

    /// A human (gold) against an engine, with both setups played.
    fn human_vs_engine_started() -> Session {
        let mut s = Session::new();
        s.start_match([Player::Human, engine("bot")], [None, None], false);
        s.commit_setup().unwrap();
        s.apply_engine_move(s.generation(), Color::Silver, 1, &setup_text(Color::Silver)).unwrap();
        s
    }

    #[test]
    fn planning_during_a_match() {
        let mut s = human_vs_engine_started();
        assert!(s.plays_live());
        s.try_step(sq("e2"), sq("e3")).unwrap();
        s.commit_turn(false).unwrap();
        // The engine's turn: a move at the live position is a plan.
        assert!(!s.plays_live() && s.can_input());
        s.try_step(sq("e7"), sq("e6")).unwrap();
        s.commit_turn(false).unwrap();
        let v = s.view();
        assert_eq!((v.ply, v.live_ply), (4, Some(3)));
        assert_eq!(s.engine_turn().unwrap().ply, 3, "the plan isn't played");
        // The engine plays something else; the plan stays as a variation
        // and the board stays on it.
        let anim = s.apply_engine_move(s.generation(), Color::Silver, 3, "db7s").unwrap();
        assert!(anim.is_empty());
        assert_eq!(s.view().moves[3].notation, "ee7s");
        let live = s.matchup.as_ref().unwrap().live;
        assert_eq!(s.tree[live].mv().unwrap().notation(), "db7s");
        assert!(s.tree.is_main_line(live));
        s.goto_live();
        let v = s.view();
        assert_eq!((v.ply, v.live_ply, v.plays_live), (4, Some(4), true));
    }

    /// Gold's elephant walks e2 to d5, using all four steps.
    fn full_gold_turn(s: &mut Session) {
        for (from, to) in [("e2", "e3"), ("e3", "e4"), ("e4", "e5"), ("e5", "d5")] {
            s.try_step(sq(from), sq(to)).unwrap();
        }
    }

    #[test]
    fn a_step_after_a_full_turn_starts_the_next() {
        let mut s = Session::new();
        s.commit_setup().unwrap();
        s.commit_setup().unwrap();
        full_gold_turn(&mut s);
        assert!(s.legal_targets(sq("e7")).iter().any(|t| t.to == sq("e6")), "silver's steps are offered");
        let anim = s.try_step(sq("e7"), sq("e6")).unwrap();
        assert_eq!(anim.len(), 1);
        let v = s.view();
        assert_eq!(v.moves.last().unwrap().notation, "Ee2n Ee3n Ee4n Ee5w");
        assert_eq!((v.ply, v.turn.unwrap().steps.len()), (3, 1));

        // Turned off, the fifth step is refused.
        s.cancel_turn();
        s.goto(2).unwrap();
        s.set_continue_turns(false);
        full_gold_turn(&mut s);
        assert!(s.legal_targets(sq("e7")).is_empty());
        assert!(s.try_step(sq("e7"), sq("e6")).is_err());
        assert_eq!(s.view().turn.unwrap().steps.len(), 4);
    }

    #[test]
    fn stepping_past_your_move_plans_it_until_played() {
        let mut s = human_vs_engine_started();
        full_gold_turn(&mut s);
        s.try_step(sq("e7"), sq("e6")).unwrap();
        let v = s.view();
        assert_eq!((v.ply, v.live_ply), (3, Some(2)), "gold's turn is a plan, not played");
        assert!(s.engine_turn().is_none(), "the engine isn't asked to move");
        assert_eq!(v.plan_move.as_deref(), Some("2g Ee2n Ee3n Ee4n Ee5w"));

        // Enter with no turn in progress plays the plan's first move.
        s.cancel_turn();
        s.commit_turn(false).unwrap();
        let v = s.view();
        assert_eq!((v.ply, v.live_ply), (3, Some(3)));
        assert_eq!(s.engine_turn().unwrap().ply, 3);
    }

    #[test]
    fn a_short_turn_can_end_as_a_plan() {
        let mut s = human_vs_engine_started();
        s.try_step(sq("e2"), sq("e3")).unwrap();
        s.commit_turn(true).unwrap();
        let v = s.view();
        assert_eq!((v.ply, v.live_ply), (3, Some(2)));
        assert!(s.commit_turn(true).is_err(), "nothing to end");
    }

    #[test]
    fn backspace_reopens_the_previous_move() {
        let mut s = Session::new();
        s.commit_setup().unwrap();
        s.commit_setup().unwrap();
        full_gold_turn(&mut s);
        s.commit_turn(false).unwrap();
        assert!(s.view().can_undo);
        let anim = s.undo_step().unwrap();
        assert_eq!((anim.len(), anim[0].from, anim[0].to), (1, sq("d5"), sq("e5")));
        let v = s.view();
        assert_eq!((v.ply, v.turn.unwrap().steps.len()), (2, 3));
        assert_eq!(s.tree[s.line[3]].mv().unwrap().notation(), "Ee2n Ee3n Ee4n Ee5w", "the move stays");
        // The setups aren't reopened.
        s.cancel_turn();
        assert!(!s.view().can_undo && s.undo_step().is_err());
    }

    /// A human (gold) against an engine with takebacks, after gold's
    /// elephant move and the engine's reply.
    fn takeback_game() -> Session {
        let mut s = Session::new();
        s.start_match([Player::Human, engine("bot")], [None, None], true);
        s.commit_setup().unwrap();
        s.apply_engine_move(s.generation(), Color::Silver, 1, &setup_text(Color::Silver)).unwrap();
        full_gold_turn(&mut s);
        s.commit_turn(false).unwrap();
        s.apply_engine_move(s.generation(), Color::Silver, 3, "ee7s").unwrap();
        s
    }

    #[test]
    fn backspace_takes_back_to_the_humans_move() {
        let mut s = takeback_game();
        assert!(s.view().can_take_back);
        let anim = s.undo_step().unwrap();
        assert_eq!(anim.len(), 2, "the engine's step, then gold's last step");
        let v = s.view();
        assert_eq!((v.ply, v.live_ply, v.plays_live), (2, Some(2), true));
        assert_eq!(v.turn.unwrap().steps.len(), 3);
        assert!(s.engine_turn().is_none());
        // A different move replaces the old line, which stays as a variation.
        s.try_step(sq("d2"), sq("d3")).unwrap();
        s.commit_turn(false).unwrap();
        let live = s.matchup.as_ref().unwrap().live;
        assert_eq!(s.tree[live].mv().unwrap().notation(), "Ee2n Ee3n Ee4n Md2n");
        assert!(s.tree.is_main_line(live));
        assert_eq!(s.tree[s.tree[live].parent().unwrap()].children().len(), 2);
        assert_eq!(s.engine_turn().unwrap().ply, 3);
    }

    #[test]
    fn take_back_while_the_engine_thinks() {
        let mut s = takeback_game();
        s.take_back().unwrap();
        s.try_step(sq("d2"), sq("d3")).unwrap();
        s.commit_turn(false).unwrap();
        assert_eq!(s.engine_turn().unwrap().ply, 3);
        // The engine is to move: taking back undoes only gold's move.
        let anim = s.take_back().unwrap();
        assert_eq!(anim.len(), 1);
        let v = s.view();
        assert_eq!((v.ply, v.live_ply, v.turn.is_none()), (2, Some(2), true));
        assert!(s.take_back().is_err(), "not into the setups");
    }

    #[test]
    fn takebacks_between_engines_undo_one_ply() {
        let mut s = Session::new();
        s.start_match([engine("a"), engine("b")], [None, None], true);
        let g = s.generation();
        s.apply_engine_move(g, Color::Gold, 0, &setup_text(Color::Gold)).unwrap();
        s.apply_engine_move(g, Color::Silver, 1, &setup_text(Color::Silver)).unwrap();
        s.apply_engine_move(g, Color::Gold, 2, "Ee2n Ee3n").unwrap();
        s.apply_engine_move(g, Color::Silver, 3, "ee7s").unwrap();
        s.undo_step().unwrap();
        assert_eq!(s.view().live_ply, Some(3));
        assert_eq!(s.engine_turn().unwrap().side, Color::Silver);
    }

    #[test]
    fn a_takeback_restores_the_clocks() {
        let mut s = Session::new();
        s.start_match([Player::Human, engine("bot")], [Some("30s/2m".parse().unwrap()); 2], true);
        s.commit_setup().unwrap();
        s.apply_engine_move(s.generation(), Color::Silver, 1, &setup_text(Color::Silver)).unwrap();
        let reserves = |s: &Session| s.matchup.as_ref().unwrap().clock.as_ref().unwrap().reserves;
        let before = reserves(&s);
        // Gold takes 50 s over its 30 s move time.
        s.matchup.as_mut().unwrap().turn_started -= Duration::from_secs(50);
        full_gold_turn(&mut s);
        s.commit_turn(false).unwrap();
        s.apply_engine_move(s.generation(), Color::Silver, 3, "ee7s").unwrap();
        assert_ne!(reserves(&s), before);
        s.take_back().unwrap();
        assert_eq!(reserves(&s), before);
    }

    #[test]
    fn takebacks_are_off_by_default() {
        let mut s = human_vs_engine_started();
        s.try_step(sq("e2"), sq("e3")).unwrap();
        s.commit_turn(false).unwrap();
        let v = s.view();
        assert!(!v.can_take_back && !v.can_undo);
        assert!(s.undo_step().is_err());
    }

    #[test]
    fn a_plan_matching_the_engine_move_becomes_the_game() {
        let mut s = human_vs_engine_started();
        s.try_step(sq("e2"), sq("e3")).unwrap();
        s.commit_turn(false).unwrap();
        s.try_step(sq("e7"), sq("e6")).unwrap();
        s.commit_turn(false).unwrap();
        s.goto(3).unwrap();
        let anim = s.apply_engine_move(s.generation(), Color::Silver, 3, "ee7s").unwrap();
        assert_eq!(anim.len(), 1, "watching live: animate");
        let v = s.view();
        assert_eq!((v.ply, v.moves.len(), v.live_ply), (4, 4, Some(4)));
        assert_eq!(s.tree.len(), 5, "no duplicate node");
    }

    #[test]
    fn exploring_earlier_moves_in_a_match() {
        let mut s = human_vs_engine_started();
        s.try_step(sq("e2"), sq("e3")).unwrap();
        s.commit_turn(false).unwrap();
        s.goto(2).unwrap();
        s.try_step(sq("d2"), sq("d3")).unwrap();
        s.commit_turn(false).unwrap();
        let v = s.view();
        assert_eq!((v.ply, v.live_ply, v.plays_live), (3, None, false));
        assert_eq!(s.engine_turn().unwrap().moves.last().unwrap(), "Ee2n");
        // Setups can't be planned.
        s.goto(1).unwrap();
        assert!(!s.can_input());
    }

    #[test]
    fn plans_stay_as_analysis_after_the_game_ends() {
        let mut s = Session::new();
        s.start_match([Player::Human, engine("b")], [Some("1s/0".parse().unwrap()); 2], false);
        s.commit_setup().unwrap();
        s.apply_engine_move(s.generation(), Color::Silver, 1, &setup_text(Color::Silver)).unwrap();
        s.goto(1).unwrap();
        // An earlier variation survives; nothing is planned past the live node here.
        assert!(s.check_timeout(Instant::now() + Duration::from_secs(5)));
        let v = s.view();
        assert_eq!(v.ply, 1);
        s.goto_live();
        assert_eq!(s.view().result.unwrap().reason, WinReason::Timeout);

        let mut s = human_vs_engine_started();
        s.try_step(sq("e2"), sq("e3")).unwrap();
        s.commit_turn(false).unwrap();
        s.try_step(sq("e7"), sq("e6")).unwrap();
        s.commit_turn(false).unwrap();
        let g = s.generation();
        s.engine_failed(g, Color::Silver, "crashed".into());
        let v = s.view();
        assert_eq!((v.ply, v.moves.len()), (4, 4), "the board stays on the plan");
        s.goto_live();
        let v = s.view();
        assert_eq!((v.ply, v.live_ply, v.moves_after_cursor), (3, Some(3), 1), "still on the plan's line");
        assert_eq!(v.result.unwrap().reason, WinReason::Forfeit);
        assert_eq!(v.phase, Phase::Play, "the position can still be analysed");
        // More analysis after the end is a variation, and the plan is still there.
        s.try_step(sq("d7"), sq("d6")).unwrap();
        s.commit_turn(false).unwrap();
        let live = s.matchup.as_ref().unwrap().live;
        assert_eq!(s.tree[live].children().len(), 2);
        assert_eq!(s.tree.line_end(GameTree::ROOT), live);
        let record = s.export(false);
        assert_eq!(record.matches("(\n2s ").count(), 2, "{record}");
        assert!(record.ends_with(")\n1-0\n"), "{record}");
    }

    #[test]
    fn timeouts_and_failures() {
        let mut s = Session::new();
        s.start_match([Player::Human, engine("b")], [Some("1s/0".parse().unwrap()); 2], false);
        let v = s.view();
        let clock = v.clock.unwrap();
        assert_eq!((clock.running, clock.turn_allowance_ms), (Some(Color::Gold), 1000));
        assert!(!s.check_timeout(Instant::now()));
        assert!(s.check_timeout(Instant::now() + Duration::from_secs(2)));
        assert_eq!(
            s.view().result.unwrap(),
            GameResult { winner: Color::Silver, reason: WinReason::Timeout }
        );
        assert!(s.view().clock.unwrap().running.is_none());

        let mut s = Session::new();
        s.start_match([engine("a"), engine("b")], [None, None], false);
        let g = s.generation();
        s.engine_failed(g, Color::Gold, "crashed".into());
        assert_eq!(s.view().result.unwrap().reason, WinReason::Forfeit);
    }

    #[test]
    fn separate_time_controls_per_side() {
        let mut s = Session::new();
        let gold: TimeControl = "1s/0".parse().unwrap();
        s.start_match([engine("a"), engine("b")], [Some(gold), None], false);
        let clock = s.view().clock.unwrap();
        assert_eq!(clock.gold.unwrap().move_time_ms, 1000);
        assert!(clock.silver.is_none());
        assert_eq!(s.engine_turn().unwrap().time_control, Some(gold));
        s.apply_engine_move(s.generation(), Color::Gold, 0, &setup_text(Color::Gold)).unwrap();
        // Silver is untimed: no deadline, no running clock.
        assert_eq!(s.turn_deadline(), None);
        assert_eq!(s.engine_turn().unwrap().time_control, None);
        assert!(s.view().clock.unwrap().running.is_none());
        assert!(!s.check_timeout(Instant::now() + Duration::from_secs(3600)));
    }

    #[test]
    fn game_time_limit_ends_by_score() {
        let mut s = Session::new();
        // 1 h per move, but a 1 s game limit.
        let tc: TimeControl = "1h/0/100/0/1s".parse().unwrap();
        s.start_match([engine("a"), engine("b")], [Some(tc); 2], false);
        assert!(s.view().clock.unwrap().game_remaining_ms.unwrap() <= 1000);
        assert!(s.check_timeout(Instant::now() + Duration::from_secs(2)));
        let v = s.view();
        assert_eq!(v.result.unwrap().reason, WinReason::Score);
        assert_eq!(v.end_detail.as_deref(), Some("game time limit reached"));
    }

    #[test]
    fn moves_are_timed_without_a_clock() {
        let mut s = Session::new();
        s.start_match([engine("a"), engine("b")], [None, None], false);
        assert_eq!(s.last_move_time(), None);
        std::thread::sleep(Duration::from_millis(20));
        s.apply_engine_move(s.generation(), Color::Gold, 0, &setup_text(Color::Gold)).unwrap();
        assert!(s.last_move_time().unwrap() >= Duration::from_millis(20));
        assert!(s.view().clock.is_none());
    }

    #[test]
    fn end_match_returns_to_free_play() {
        let mut s = Session::new();
        s.start_match([engine("a"), engine("b")], [None, None], false);
        let g = s.generation();
        s.end_match();
        assert!(s.generation() > g);
        let v = s.view();
        assert!(v.players.is_none() && v.can_input);
        assert_eq!(v.position.pieces.len(), 16, "gold's draft is back");
    }

    #[test]
    fn load_error_has_line() {
        let mut s = Session::new();
        let err = s.load("1g Ra1\n").unwrap_err();
        assert_eq!(err.line, Some(1));
    }

    fn analysing(s: &mut Session) {
        s.set_analysis(Some(AnalysisEngine { id: "e".into(), name: "E".into() }));
    }

    fn strings(moves: &[&str]) -> Vec<String> {
        moves.iter().map(|m| m.to_string()).collect()
    }

    #[test]
    fn analysis_targets_the_shown_node() {
        let mut s = Session::new();
        s.load(SAMPLE).unwrap();
        assert!(s.analysis_target().is_none(), "off");
        analysing(&mut s);
        s.goto(4).unwrap();
        let t = s.analysis_target().unwrap();
        assert_eq!((t.node, t.label.as_str(), t.moves.len()), (s.cursor_node(), "3g", 4));
        assert_eq!(t.moves[2], "Ee2n Ee3n Ee4n Ee5e");
        // A turn being entered doesn't change the target.
        s.try_step(sq("a2"), sq("a3")).unwrap();
        assert!(s.analysis_target().unwrap().same(&t));
        // Analysis stays on for a new game; the target is new.
        s.new_game();
        let fresh = s.analysis_target().unwrap();
        assert!(!fresh.same(&t));
        assert_eq!(fresh.label, "1g");
    }

    #[test]
    fn analysis_goes_on_after_a_resignation_but_not_a_goal() {
        let mut s = Session::new();
        s.load(SAMPLE).unwrap();
        analysing(&mut s);
        s.finish(GameResult { winner: Color::Silver, reason: WinReason::Resignation }, None);
        let t = s.analysis_target().expect("the position is still open");
        assert_eq!(t.game.result(), None);
        // A result on the board ends the search. (Set directly: the sample
        // has no goal.)
        s.load(SAMPLE).unwrap();
        s.finish(GameResult { winner: Color::Gold, reason: WinReason::Goal }, None);
        assert!(s.analysis_target().is_none());
        s.goto(9).unwrap();
        assert!(s.analysis_target().is_some());
    }

    #[test]
    fn stored_analysis_keeps_the_deepest_line() {
        let mut s = Session::new();
        s.load(SAMPLE).unwrap();
        let node = s.cursor_node();
        let line = |depth: &str| AnalysisLine {
            node,
            depth: Some(depth.into()),
            eval: None,
            pv: Vec::new(),
            nodes: None,
            time_ms: None,
        };
        let g = s.generation();
        s.store_analysis(g, &line("8"));
        s.store_analysis(g, &line("12"));
        s.store_analysis(g, &line("9+"));
        assert_eq!(s.stored_analysis(g, node).unwrap().depth.as_deref(), Some("12"));
        s.store_analysis(g, &line("12+"));
        assert_eq!(s.view().stored_analysis.unwrap().depth.as_deref(), Some("12+"));
        s.store_analysis(g + 1, &line("30"));
        assert_eq!(s.stored_analysis(g, node).unwrap().depth.as_deref(), Some("12+"), "old game");
        s.goto(3).unwrap();
        let earlier = s.cursor_node();
        s.delete_from(s.line[4]).unwrap();
        assert!(s.stored_analysis(g, node).is_none(), "dropped with its node");
        s.load(SAMPLE).unwrap();
        assert!(s.stored_analysis(s.generation(), earlier).is_none(), "cleared with a new game");
        assert_eq!(depth_key(None), (-1.0, false));
        assert!(depth_key(Some("12.4+")) > depth_key(Some("12")));
    }

    #[test]
    fn add_line_adds_and_shows_a_variation() {
        let mut s = Session::new();
        s.load(SAMPLE).unwrap();
        s.goto(2).unwrap();
        let from = s.cursor_node();
        let anim = s.add_line(from, &strings(&["Ee2n", "ee7s", "Ee3n"])).unwrap();
        assert_eq!(anim.len(), 3, "the moves from the shown node animate");
        let v = s.view();
        assert_eq!((v.ply, v.moves[2].notation.as_str()), (5, "Ee2n"));
        assert_eq!(s.tree[from].children().len(), 2);
        // The game's own moves are reused, and an illegal move adds nothing.
        let nodes = s.tree.len();
        s.add_line(from, &strings(&["Ee2n Ee3n Ee4n Ee5e", "hh7s hh6s hh5w"])).unwrap();
        assert_eq!(s.tree.len(), nodes);
        assert_eq!(s.view().moves_after_cursor, 6, "on the game's line");
        let err = s.add_line(from, &strings(&["Ee2n", "Ra1n"])).unwrap_err();
        assert!(err.message.starts_with("Ra1n"), "{}", err.message);
        assert_eq!(s.tree.len(), nodes);
        assert!(s.add_line(from, &[]).is_err());
        // From a node other than the shown one: no animation.
        s.goto(0).unwrap();
        assert!(s.add_line(from, &strings(&["Ee2n", "ee7s"])).unwrap().is_empty());
        assert_eq!(s.view().ply, 4);
    }

    #[test]
    fn add_line_in_a_match_is_a_plan() {
        let mut s = human_vs_engine_started();
        let live = s.matchup.as_ref().unwrap().live;
        s.add_line(live, &strings(&["Ee2n", "ee7s"])).unwrap();
        assert_eq!(s.matchup.as_ref().unwrap().live, live, "nothing played");
        assert_eq!(s.engine_turn(), None, "still the human's turn");
        assert_eq!(s.view().ply, 4);
        let mut s = Session::new();
        s.start_match([Player::Human, engine("bot")], [None, None], false);
        assert!(s.add_line(GameTree::ROOT, &[setup_text(Color::Gold)]).is_err(), "setups aren't planned");
    }

    #[test]
    fn preview_line_shows_the_position_after_it() {
        let mut s = Session::new();
        s.load(SAMPLE).unwrap();
        s.goto(2).unwrap();
        let from = s.cursor_node();
        let p = s.preview_line(from, &strings(&["Ee2n Ee3n", "ee7s"])).unwrap();
        let at = |q: &str| p.pieces.iter().find(|x| x.square == sq(q)).map(|x| x.piece);
        assert!(at("e4").is_some() && at("e2").is_none() && at("e6").is_some());
        assert_eq!(p.side_to_move, Color::Gold);
        assert!(s.preview_line(from, &strings(&["Ra1n"])).is_err());
        assert_eq!(s.view().ply, 2, "nothing changes");
    }
}
