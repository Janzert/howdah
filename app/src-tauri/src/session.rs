//! The game being viewed and edited in a window: pure state logic with no
//! Tauri types, so it can be unit tested (and reused by other front ends).
//!
//! A session is either free play (anyone may enter moves for either side,
//! at any ply) or a match: each side has a human or engine player, and
//! optionally a clock. In a match, humans can only move on their own turn at
//! the live end of the game; engine moves arrive via `apply_engine_move`
//! from the controller.

use std::time::{Duration, Instant};

use arimaa_core::{
    Color, Game, GameError, GameResult, Move, Placement, Position, Route, Square, Step, StepEffect, StepKind,
    TimeControl, TurnBuilder, WinReason, default_setup, limit_score_winner, notation,
};

use crate::dto::{
    AnimPiece, AnimStep, ApiError, CapturedView, ClockView, LastMoveView, LastStepView, MoveView, Phase,
    PieceAt, PieceId, PieceView, PlayerKind, PlayerView, PlayersView, PositionView, SessionView,
    SideClockView, StepTarget, TurnStepView, TurnView,
};

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Player {
    Human,
    Engine { id: String, name: String },
}

impl Player {
    fn view(&self) -> PlayerView {
        match self {
            Player::Human => PlayerView { kind: PlayerKind::Human, name: "Human".into() },
            Player::Engine { name, .. } => PlayerView { kind: PlayerKind::Engine, name: name.clone() },
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
    clock: Option<Clock>,
    thinking: Option<Color>,
    /// When the current turn (at the live end of the game) started. Kept
    /// even without a clock, to time moves.
    turn_started: Instant,
    /// How long the most recent move took.
    last_move_time: Option<Duration>,
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
    game: Game,
    /// Ply being shown.
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
}

impl Default for Session {
    fn default() -> Self {
        Session::new()
    }
}

impl Session {
    pub fn new() -> Session {
        Session::with_game(Game::new())
    }

    fn with_game(game: Game) -> Session {
        let cursor = game.ply_count();
        let mut s = Session {
            game,
            cursor,
            turn: None,
            setup_draft: None,
            ids: Vec::new(),
            matchup: None,
            end_detail: None,
            generation: 0,
        };
        s.refresh();
        s
    }

    /// Replaces the game, keeping the generation counter moving forward.
    fn replace(&mut self, game: Game, matchup: Option<Match>) {
        let generation = self.generation + 1;
        *self = Session::with_game(game);
        self.generation = generation;
        self.matchup = matchup;
        self.refresh();
    }

    pub fn generation(&self) -> u64 {
        self.generation
    }

    fn player(&self, side: Color) -> &Player {
        self.matchup.as_ref().map_or(&Player::Human, |m| &m.players[side.index()])
    }

    /// Side to move at the live end of the game.
    fn live_side(&self) -> Color {
        self.game.current_position().side_to_move()
    }

    /// Whether board input is accepted now.
    pub fn can_input(&self) -> bool {
        match &self.matchup {
            None => true,
            Some(_) => {
                self.game.result().is_none()
                    && self.cursor == self.game.ply_count()
                    && *self.player(self.live_side()) == Player::Human
            }
        }
    }

    fn require_input(&self) -> Result<(), ApiError> {
        if self.can_input() {
            Ok(())
        } else if self.cursor != self.game.ply_count() {
            Err(ApiError::state("the match is live; go to the latest move to play"))
        } else {
            Err(ApiError::state("it's not your turn"))
        }
    }

    fn refresh(&mut self) {
        self.ids = compute_ids(&self.game);
        let setup_due = Game::is_setup_ply(self.cursor)
            && self.cursor == self.game.ply_count()
            && self.game.result().is_none()
            && *self.player(self.live_side()) == Player::Human;
        if !setup_due {
            self.setup_draft = None;
        } else if self.setup_draft.is_none() {
            let side = self.game.current_position().side_to_move();
            self.setup_draft = Some(default_setup(side));
        }
    }

    fn cursor_position(&self) -> &Position {
        self.game.position_at(self.cursor).expect("cursor is within the game")
    }

    pub fn new_game(&mut self) {
        self.replace(Game::new(), None);
    }

    pub fn load(&mut self, record: &str) -> Result<(), ApiError> {
        let game = Game::parse(record)?;
        self.replace(game, None);
        Ok(())
    }

    /// Starts a new game between the given players, with a time control per
    /// side (gold, silver); `None` leaves that side untimed.
    pub fn start_match(&mut self, players: [Player; 2], time_controls: [Option<TimeControl>; 2]) {
        let now = Instant::now();
        let clock = time_controls.iter().any(Option::is_some).then(|| Clock {
            tcs: time_controls,
            reserves: time_controls.map(|tc| tc.map_or(Duration::ZERO, |t| t.starting_reserve())),
            game_started: now,
        });
        let matchup =
            Match { players, clock, thinking: None, turn_started: Instant::now(), last_move_time: None };
        self.replace(Game::new(), Some(matchup));
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
        let m = self.matchup.as_ref()?;
        if self.game.result().is_some() {
            return None;
        }
        let side = self.live_side();
        let Player::Engine { id, .. } = &m.players[side.index()] else { return None };
        Some(EngineTurn {
            generation: self.generation,
            side,
            engine_id: id.clone(),
            ply: self.game.ply_count(),
            moves: self.game.moves().iter().map(Move::notation).collect(),
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
        if self.game.result().is_some() {
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
    pub fn check_timeout(&mut self, now: Instant) -> bool {
        if !self.turn_deadline().is_some_and(|d| now >= d) {
            return false;
        }
        let game_limit = self.matchup.as_ref().and_then(|m| m.clock.as_ref()).and_then(Clock::game_deadline);
        if game_limit.is_some_and(|g| now >= g) {
            let winner = limit_score_winner(self.game.current_position());
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

    /// Ends the game with a result decided outside the board.
    fn finish(&mut self, result: GameResult, detail: Option<String>) {
        if self.game.end_game(result).is_ok() {
            self.end_detail = detail;
            self.turn = None;
            self.setup_draft = None;
            if let Some(m) = &mut self.matchup {
                m.thinking = None;
            }
        }
    }

    /// Engine ids playing (gold, silver) in the current match.
    pub fn engine_players(&self) -> [Option<String>; 2] {
        let id = |p: &Player| match p {
            Player::Engine { id, .. } => Some(id.clone()),
            Player::Human => None,
        };
        match &self.matchup {
            Some(m) => [id(&m.players[0]), id(&m.players[1])],
            None => [None, None],
        }
    }

    /// Ends the game because the engine for `side` failed (crashed, didn't
    /// start, ...), at any point of the game.
    pub fn engine_failed(&mut self, generation: u64, side: Color, detail: String) {
        if generation == self.generation && self.game.result().is_none() {
            self.finish(GameResult { winner: side.opponent(), reason: WinReason::Forfeit }, Some(detail));
        }
    }

    /// Timing and clock bookkeeping for a move by the side to move, before
    /// it's added. Returns false (and ends the game) if the move came too late.
    fn clock_move(&mut self, now: Instant) -> bool {
        let setup = Game::is_setup_ply(self.game.ply_count());
        let side = self.live_side();
        let Some(m) = self.matchup.as_mut() else { return true };
        let used = now.saturating_duration_since(m.turn_started);
        m.last_move_time = Some(used);
        m.turn_started = now;
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

    /// End-of-turn checks that depend on the match (the turn limit).
    fn after_move(&mut self, mover: Color, ply: usize) {
        let limit = self.matchup.as_ref().and_then(|m| m.clock.as_ref()).and_then(Clock::turn_limit);
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
        if generation != self.generation
            || ply != self.game.ply_count()
            || side != self.live_side()
            || self.game.result().is_some()
        {
            return Err(ApiError::state("stale engine move"));
        }
        if let Some(m) = &mut self.matchup {
            m.thinking = None;
        }
        if !self.clock_move(Instant::now()) {
            return Ok(Vec::new());
        }
        let following = self.cursor == ply;
        if text.trim().eq_ignore_ascii_case("resign") {
            self.finish(GameResult { winner: side.opponent(), reason: WinReason::Resignation }, None);
            return Ok(Vec::new());
        }
        if let Err(e) = self.game.play_notation(text) {
            let detail = format!("{side:?} engine played {text:?}: {e}");
            self.finish(GameResult { winner: side.opponent(), reason: WinReason::IllegalMove }, Some(detail));
            return Ok(Vec::new());
        }
        self.after_move(side, ply);
        let mut anim = Vec::new();
        if following {
            self.turn = None;
            self.cursor = self.game.ply_count();
            self.ids = compute_ids(&self.game);
            anim = self.move_animation(ply);
        }
        self.refresh();
        Ok(anim)
    }

    pub fn export(&self) -> String {
        self.game.to_record()
    }

    /// Shows `ply`, discarding any turn in progress. Moving one ply forward or
    /// back animates that move.
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
        if self.cursor == self.game.ply_count() && self.game.result().is_some() {
            return Err(ApiError::state("the game is over"));
        }
        Ok(TurnBuilder::new(self.cursor_position()))
    }

    pub fn try_step(&mut self, from: Square, to: Square) -> Result<Vec<AnimStep>, ApiError> {
        self.require_input()?;
        let mut tb = self.turn_builder()?;
        tb.try_move(from, to).map_err(ApiError::illegal)?;
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
        let tb = self.turn_builder().ok()?;
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
        let mut tb = self.turn_builder()?;
        let Some(steps) = self.route(&tb, from, to, path) else {
            return Err(ApiError::illegal("that piece can't get there this turn"));
        };
        for &step in &steps {
            tb.try_step(step).map_err(ApiError::illegal)?;
        }
        self.turn = Some(tb);
        let (anim, _) = self.turn_animation();
        Ok(anim[anim.len() - steps.len()..].to_vec())
    }

    pub fn undo_step(&mut self) -> Result<Vec<AnimStep>, ApiError> {
        if self.turn.is_none() {
            return Err(ApiError::state("no step to undo"));
        }
        let (anim, _) = self.turn_animation();
        let tb = self.turn.as_mut().unwrap();
        tb.undo();
        if tb.steps().is_empty() {
            self.turn = None;
        }
        Ok(anim.last().map(|a| a.reversed()).into_iter().collect())
    }

    pub fn cancel_turn(&mut self) -> Vec<AnimStep> {
        let (anim, _) = self.turn_animation();
        self.turn = None;
        anim.into_iter().rev().map(AnimStep::reversed).collect()
    }

    /// Adds the in-progress turn to the game, replacing any later moves.
    pub fn commit_turn(&mut self) -> Result<(), ApiError> {
        let Some(tb) = self.turn.take() else { return Err(ApiError::state("no turn in progress")) };
        if let Err(e) = tb.can_finish() {
            self.turn = Some(tb);
            return Err(ApiError::illegal(e));
        }
        let turn = tb.clone().finish().expect("checked above");
        // Check before truncating, so a rejected turn doesn't lose later moves.
        if self.game.is_third_repetition(self.cursor, &turn.end) {
            self.turn = Some(tb);
            return Err(ApiError::illegal(GameError::Repetition));
        }
        if let Err(e) = self.require_input() {
            self.turn = Some(tb);
            return Err(e);
        }
        let (mover, ply) = (turn.start.side_to_move(), self.cursor);
        self.game.truncate(self.cursor);
        if self.clock_move(Instant::now()) {
            self.game.play_turn(turn).map_err(ApiError::illegal)?;
            self.after_move(mover, ply);
        }
        self.cursor = self.game.ply_count();
        self.refresh();
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
        if self.clock_move(Instant::now()) {
            self.game.play_setup(draft).map_err(ApiError::illegal)?;
        }
        self.cursor = self.game.ply_count();
        self.refresh();
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
        let Ok(tb) = self.turn_builder() else { return Vec::new() };
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
            .then(|| arimaa_core::GameError::Repetition.to_string())
    }

    fn phase(&self) -> Phase {
        if Game::is_setup_ply(self.cursor) {
            Phase::Setup
        } else if self.cursor == self.game.ply_count() && self.game.result().is_some() {
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
            ply: self.cursor,
            phase: self.phase(),
            position: position_view(&position, &ids),
            turn,
            last_move: self.last_move_view(),
            captured: self.captured_view(),
            moves_after_cursor: self.game.ply_count() - self.cursor,
            result: if self.cursor == self.game.ply_count() { self.game.result() } else { None },
            end_marker: self.game.end_marker().map(str::to_string),
            end_detail: self.end_detail.clone(),
            players: self
                .matchup
                .as_ref()
                .map(|m| PlayersView { gold: m.players[0].view(), silver: m.players[1].view() }),
            clock: self.clock_view(),
            thinking: self.matchup.as_ref().and_then(|m| m.thinking),
            can_input: self.can_input(),
        }
    }

    fn last_move_view(&self) -> Option<LastMoveView> {
        let ply = self.cursor.checked_sub(1)?;
        let Move::Steps(steps) = &self.game.moves()[ply] else { return None };
        let color = self.game.position_at(ply)?.side_to_move();
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
            self.game.result().is_none().then(|| self.live_side()).filter(|s| clock.tc(*s).is_some());
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

    const SAMPLE: &str = include_str!("../../../crates/arimaa-core/tests/data/sample_game.txt");

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
        s.commit_turn().unwrap();
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
        assert!(s.commit_turn().is_err());
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
    fn move_from_earlier_ply_replaces_later_moves() {
        let mut s = Session::new();
        s.load(SAMPLE).unwrap();
        s.goto(2).unwrap();
        assert_eq!(s.view().moves_after_cursor, 8);
        s.try_step(sq("a2"), sq("a3")).unwrap();
        s.commit_turn().unwrap();
        let v = s.view();
        assert_eq!(v.moves.len(), 3);
        assert_eq!(v.moves[2].notation, "Ha2n");
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
        use arimaa_core::PieceKind;
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
        s.start_match([Player::Human, engine("bot")], [None, None]);
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
        s.commit_turn().unwrap();
        assert_eq!(s.engine_turn().unwrap().ply, 3);
    }

    #[test]
    fn engine_vs_engine_has_no_draft() {
        let mut s = Session::new();
        s.start_match([engine("a"), engine("b")], [None, None]);
        assert_eq!(s.view().position.pieces.len(), 0);
        assert!(!s.can_input());
        let t = s.engine_turn().unwrap();
        assert_eq!((t.side, t.engine_id.as_str()), (Color::Gold, "a"));
    }

    #[test]
    fn stale_and_illegal_engine_moves() {
        let mut s = Session::new();
        s.start_match([engine("a"), engine("b")], [None, None]);
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
        s.start_match([engine("a"), engine("b")], [None, None]);
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

    #[test]
    fn timeouts_and_failures() {
        let mut s = Session::new();
        s.start_match([Player::Human, engine("b")], [Some("1s/0".parse().unwrap()); 2]);
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
        s.start_match([engine("a"), engine("b")], [None, None]);
        let g = s.generation();
        s.engine_failed(g, Color::Gold, "crashed".into());
        assert_eq!(s.view().result.unwrap().reason, WinReason::Forfeit);
    }

    #[test]
    fn separate_time_controls_per_side() {
        let mut s = Session::new();
        let gold: TimeControl = "1s/0".parse().unwrap();
        s.start_match([engine("a"), engine("b")], [Some(gold), None]);
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
        s.start_match([engine("a"), engine("b")], [Some(tc); 2]);
        assert!(s.view().clock.unwrap().game_remaining_ms.unwrap() <= 1000);
        assert!(s.check_timeout(Instant::now() + Duration::from_secs(2)));
        let v = s.view();
        assert_eq!(v.result.unwrap().reason, WinReason::Score);
        assert_eq!(v.end_detail.as_deref(), Some("game time limit reached"));
    }

    #[test]
    fn moves_are_timed_without_a_clock() {
        let mut s = Session::new();
        s.start_match([engine("a"), engine("b")], [None, None]);
        assert_eq!(s.last_move_time(), None);
        std::thread::sleep(Duration::from_millis(20));
        s.apply_engine_move(s.generation(), Color::Gold, 0, &setup_text(Color::Gold)).unwrap();
        assert!(s.last_move_time().unwrap() >= Duration::from_millis(20));
        assert!(s.view().clock.is_none());
    }

    #[test]
    fn end_match_returns_to_free_play() {
        let mut s = Session::new();
        s.start_match([engine("a"), engine("b")], [None, None]);
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
}
