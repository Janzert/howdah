//! The game being viewed and edited in a window: pure state logic with no
//! Tauri types, so it can be unit tested (and reused by other front ends).

use arimaa_core::{
    Color, Game, Move, Placement, Position, Square, StepEffect, TurnBuilder, default_setup, notation,
};

use crate::dto::{
    AnimPiece, AnimStep, ApiError, MoveView, Phase, PieceId, PieceView, PositionView, SessionView,
    StepTarget, TurnStepView, TurnView,
};

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
        let mut s = Session { game, cursor, turn: None, setup_draft: None, ids: Vec::new() };
        s.refresh();
        s
    }

    fn refresh(&mut self) {
        self.ids = compute_ids(&self.game);
        let setup_due = Game::is_setup_ply(self.cursor) && self.cursor == self.game.ply_count();
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
        *self = Session::new();
    }

    pub fn load(&mut self, record: &str) -> Result<(), ApiError> {
        *self = Session::with_game(Game::parse(record)?);
        Ok(())
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
        let mut tb = self.turn_builder()?;
        tb.try_move(from, to).map_err(ApiError::illegal)?;
        self.turn = Some(tb);
        let (anim, _) = self.turn_animation();
        Ok(anim.last().copied().into_iter().collect())
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
        let turn = tb.finish().expect("checked above");
        self.game.truncate(self.cursor);
        self.game.play_turn(turn).map_err(ApiError::illegal)?;
        self.cursor = self.game.ply_count();
        self.refresh();
        Ok(())
    }

    /// Swaps two pieces in the setup draft.
    pub fn setup_swap(&mut self, a: Square, b: Square) -> Result<Vec<AnimStep>, ApiError> {
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
        let Some(draft) = self.setup_draft.take() else {
            return Err(ApiError::state("no setup is being arranged"));
        };
        self.game.play_setup(draft).map_err(ApiError::illegal)?;
        self.cursor = self.game.ply_count();
        self.refresh();
        Ok(())
    }

    /// Where the piece on `from` may go next, for drag hints. The core still
    /// validates the actual step.
    pub fn legal_targets(&self, from: Square) -> Vec<StepTarget> {
        if let Some(draft) = &self.setup_draft {
            if !draft.iter().any(|p| p.square == from) {
                return Vec::new();
            }
            return draft
                .iter()
                .filter(|p| p.square != from)
                .map(|p| StepTarget { to: p.square, kind: None })
                .collect();
        }
        let Ok(tb) = self.turn_builder() else { return Vec::new() };
        tb.legal_steps_from(from)
            .into_iter()
            .filter_map(|(step, kind)| step.to().map(|to| StepTarget { to, kind: Some(kind) }))
            .collect()
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
            commit_blocker: tb.can_finish().err().map(|e| e.to_string()),
        });

        SessionView {
            moves,
            ply: self.cursor,
            phase: self.phase(),
            position: position_view(&position, &ids),
            turn,
            moves_after_cursor: self.game.ply_count() - self.cursor,
            result: if self.cursor == self.game.ply_count() { self.game.result() } else { None },
            end_marker: self.game.end_marker().map(str::to_string),
        }
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
    fn legal_targets_include_pushes() {
        let mut s = Session::new();
        s.load(SAMPLE).unwrap();
        s.goto(6).unwrap(); // gold to move: E on g4, silver horse on g3
        let targets: Vec<Square> = s.legal_targets(sq("g3")).into_iter().map(|t| t.to).collect();
        assert!(targets.contains(&sq("f3")));
        assert!(targets.contains(&sq("h3")));
    }

    #[test]
    fn load_error_has_line() {
        let mut s = Session::new();
        let err = s.load("1g Ra1\n").unwrap_err();
        assert_eq!(err.line, Some(1));
    }
}
