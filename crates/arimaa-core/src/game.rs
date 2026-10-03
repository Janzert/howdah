//! A game: the sequence of moves from the empty board, with positions cached
//! per ply.
//!
//! Ply `n` is the position after `n` moves. Ply 0 is the empty board with
//! gold to set up, and ply 2 is the first position with both sides placed.

use crate::error::{GameError, RecordError};
use crate::notation::{self, MoveBody, RecordStep};
use crate::outcome::{GameResult, outcome_after_turn};
use crate::position::Position;
use crate::setup::{Placement, apply_setup};
use crate::step::StepEffect;
use crate::turn::{Turn, TurnBuilder};
use crate::types::Color;

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Move {
    Setup(Vec<Placement>),
    Steps(Vec<StepEffect>),
}

impl Move {
    /// The move's tokens in standard notation, without the move number.
    pub fn notation(&self) -> String {
        match self {
            Move::Setup(p) => notation::format_placements(p),
            Move::Steps(s) => notation::format_steps(s),
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Game {
    moves: Vec<Move>,
    /// `positions[n]` is the position at ply `n`; always `moves.len() + 1` long.
    positions: Vec<Position>,
    result: Option<GameResult>,
    /// End marker from a loaded record, such as `resigns`. Not interpreted.
    end_marker: Option<String>,
}

impl Default for Game {
    fn default() -> Self {
        Game::new()
    }
}

impl Game {
    pub fn new() -> Game {
        Game {
            moves: Vec::new(),
            positions: vec![Position::empty(Color::Gold)],
            result: None,
            end_marker: None,
        }
    }

    pub fn moves(&self) -> &[Move] {
        &self.moves
    }

    /// Number of moves played, which is also the last ply.
    pub fn ply_count(&self) -> usize {
        self.moves.len()
    }

    pub fn position_at(&self, ply: usize) -> Option<&Position> {
        self.positions.get(ply)
    }

    pub fn current_position(&self) -> &Position {
        self.positions.last().expect("positions is never empty")
    }

    /// True if the move made from `ply` is a setup move.
    pub fn is_setup_ply(ply: usize) -> bool {
        ply < 2
    }

    pub fn result(&self) -> Option<GameResult> {
        self.result
    }

    pub fn end_marker(&self) -> Option<&str> {
        self.end_marker.as_deref()
    }

    /// Drops every move after `ply` (for playing a new line from an earlier position).
    pub fn truncate(&mut self, ply: usize) {
        if ply < self.moves.len() {
            self.moves.truncate(ply);
            self.positions.truncate(ply + 1);
            self.result = None;
            self.end_marker = None;
        }
    }

    fn check_can_move(&self) -> Result<(), GameError> {
        if self.result.is_some() { Err(GameError::GameOver) } else { Ok(()) }
    }

    pub fn play_setup(&mut self, placements: Vec<Placement>) -> Result<(), GameError> {
        self.check_can_move()?;
        if !Game::is_setup_ply(self.ply_count()) {
            return Err(GameError::ExpectedTurn);
        }
        let next = apply_setup(self.current_position(), &placements)?;
        self.moves.push(Move::Setup(placements));
        self.positions.push(next);
        Ok(())
    }

    /// Starts a turn from the current position.
    pub fn begin_turn(&self) -> Result<TurnBuilder, GameError> {
        self.check_can_move()?;
        if Game::is_setup_ply(self.ply_count()) {
            return Err(GameError::ExpectedSetup);
        }
        Ok(TurnBuilder::new(self.current_position()))
    }

    /// True if a turn from the position at `ply` ending in `end` would repeat
    /// a position (same pieces, same side to move) for the third time.
    /// Only positions from `ply` back are counted, so this also answers
    /// "what if I play this instead of the moves after `ply`?".
    pub fn is_third_repetition(&self, ply: usize, end: &Position) -> bool {
        let history = self.positions.get(2..=ply).unwrap_or(&[]);
        history.iter().filter(|p| *p == end).count() >= 2
    }

    /// Adds a turn built with [`Game::begin_turn`].
    pub fn play_turn(&mut self, turn: Turn) -> Result<Option<GameResult>, GameError> {
        self.check_can_move()?;
        if Game::is_setup_ply(self.ply_count()) {
            return Err(GameError::ExpectedSetup);
        }
        if &turn.start != self.current_position() {
            return Err(GameError::StaleTurn);
        }
        if self.is_third_repetition(self.ply_count(), &turn.end) {
            return Err(GameError::Repetition);
        }
        let mover = turn.start.side_to_move();
        self.result = outcome_after_turn(&turn.end, mover);
        self.moves.push(Move::Steps(turn.effects()));
        self.positions.push(turn.end);
        Ok(self.result)
    }

    /// Ends the game for a reason outside the rules (timeout, resignation,
    /// illegal move, ...).
    pub fn end_game(&mut self, result: GameResult) -> Result<(), GameError> {
        self.check_can_move()?;
        self.result = Some(result);
        Ok(())
    }

    /// Plays one move given in notation without a move number: a setup
    /// (`Ra1 Rb1 ...`) or steps with optional capture tokens (`Ed2n Ed3n`).
    /// This is how moves from engines and servers enter the game.
    pub fn play_notation(&mut self, text: &str) -> Result<Option<GameResult>, GameError> {
        let (body, _marker) = notation::parse_move_body(text)?;
        match body {
            MoveBody::Empty => Err(GameError::EmptyMove),
            MoveBody::Setup(p) => self.play_setup(p).map(|_| None),
            MoveBody::Steps(steps) => self.play_steps(&steps),
        }
    }

    fn play_steps(&mut self, steps: &[RecordStep]) -> Result<Option<GameResult>, GameError> {
        let tb = self.begin_turn()?;
        self.play_turn(build_turn(tb, steps)?)
    }

    /// Parses and validates a game record: one move per line, e.g.
    /// `1g Ra1 Rb1 ...`, `2g Ed2n Ed3n`, and keeps its main line. Records
    /// with tags, comments and variations are read too; see
    /// [`crate::GameRecord`].
    pub fn parse(record: &str) -> Result<Game, RecordError> {
        Ok(crate::GameRecord::parse(record)?.tree.main_game())
    }

    /// Builds a game from parts already checked by a [`crate::GameTree`]:
    /// `positions` is one longer than `moves`.
    pub(crate) fn from_parts(
        moves: Vec<Move>,
        positions: Vec<Position>,
        result: Option<GameResult>,
        end_marker: Option<String>,
    ) -> Game {
        debug_assert_eq!(positions.len(), moves.len() + 1);
        Game { moves, positions, result, end_marker }
    }

    /// Formats the game as a record, one move per line, each ending in `\n`.
    pub fn to_record(&self) -> String {
        let mut out = String::new();
        for (ply, m) in self.moves.iter().enumerate() {
            out.push_str(&notation::move_label(ply));
            out.push(' ');
            out.push_str(&m.notation());
            out.push('\n');
        }
        out
    }
}

/// Plays record steps on `tb` and finishes the turn. Missing capture
/// tokens are tolerated; wrong ones are not.
pub(crate) fn build_turn(mut tb: TurnBuilder, steps: &[RecordStep]) -> Result<Turn, GameError> {
    for rs in steps {
        let effect = tb
            .try_step(rs.step)
            .map_err(|source| GameError::Step { step: rs.step.to_string(), source })?
            .effect;
        if let Some(written) = rs.capture
            && effect.capture != Some(written)
        {
            return Err(GameError::CaptureMismatch(written.to_string()));
        }
    }
    Ok(tb.finish()?)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::setup::default_setup;

    fn started() -> Game {
        let mut g = Game::new();
        g.play_setup(default_setup(Color::Gold)).unwrap();
        g.play_setup(default_setup(Color::Silver)).unwrap();
        g
    }

    #[test]
    fn setup_then_turns() {
        let mut g = started();
        assert_eq!(g.ply_count(), 2);
        assert!(matches!(g.play_setup(default_setup(Color::Gold)), Err(GameError::ExpectedTurn)));
        let mut tb = g.begin_turn().unwrap();
        tb.try_move("e2".parse().unwrap(), "e3".parse().unwrap()).unwrap();
        g.play_turn(tb.finish().unwrap()).unwrap();
        assert_eq!(g.current_position().side_to_move(), Color::Silver);
        assert!(g.to_record().ends_with("2g Ee2n\n"));
    }

    #[test]
    fn third_repetition_is_illegal() {
        let mut g = started();
        // A gold horse and a silver horse shuffle back and forth. The
        // position after 2s (gold to move) first occurs at ply 2 (after the
        // setups) and again after each 4 plies.
        for (i, m) in ["Ha2n", "ha7s", "Ha3s", "ha6n"].iter().cycle().take(4).enumerate() {
            assert_eq!(g.play_notation(m).unwrap(), None, "move {i}");
        }
        // Back at the post-setup position for the second time; one more cycle
        // would make it the third.
        for m in ["Ha2n", "ha7s", "Ha3s"] {
            g.play_notation(m).unwrap();
        }
        let before = g.ply_count();
        assert_eq!(g.play_notation("ha6n"), Err(GameError::Repetition));
        assert_eq!(g.ply_count(), before, "rejected move isn't added");
        // A different move is fine.
        g.play_notation("ha6e").unwrap();
    }

    #[test]
    fn play_notation_and_end_game() {
        let mut g = Game::new();
        g.play_notation(&notation::format_placements(&default_setup(Color::Gold))).unwrap();
        g.play_notation(&notation::format_placements(&default_setup(Color::Silver))).unwrap();
        assert!(matches!(g.play_notation("Ee2n Ee3x"), Err(GameError::CaptureMismatch(_))));
        assert!(matches!(g.play_notation("Ra1n"), Err(GameError::Step { .. })));
        assert_eq!(g.play_notation(""), Err(GameError::EmptyMove));
        g.play_notation("Ee2n Ee3n").unwrap();
        let resign = GameResult { winner: Color::Gold, reason: crate::WinReason::Resignation };
        g.end_game(resign).unwrap();
        assert_eq!(g.result(), Some(resign));
        assert_eq!(g.play_notation("ee7s"), Err(GameError::GameOver));
        assert_eq!(g.end_game(resign), Err(GameError::GameOver));
    }

    #[test]
    fn turn_before_setup_rejected() {
        assert!(matches!(Game::new().begin_turn(), Err(GameError::ExpectedSetup)));
    }

    #[test]
    fn stale_turn_rejected() {
        let mut g = started();
        let mut tb = g.begin_turn().unwrap();
        tb.try_move("e2".parse().unwrap(), "e3".parse().unwrap()).unwrap();
        let turn = tb.finish().unwrap();
        g.play_turn(turn.clone()).unwrap();
        assert!(matches!(g.play_turn(turn), Err(GameError::StaleTurn)));
    }

    #[test]
    fn truncate_rewinds() {
        let mut g = started();
        g.truncate(1);
        assert_eq!(g.ply_count(), 1);
        assert_eq!(g.current_position().side_to_move(), Color::Silver);
    }

    #[test]
    fn parse_rejects_out_of_sequence() {
        let rec = "1g Ra1 Rb1 Rc1 Rd1 Re1 Rf1 Rg1 Rh1 Ha2 Db2 Cc2 Md2 Ee2 Cf2 Dg2 Hh2\n1g Ra7\n";
        let err = Game::parse(rec).unwrap_err();
        assert_eq!(err.line, 2);
        assert!(matches!(err.error, GameError::OutOfSequence { .. }));
    }

    #[test]
    fn parse_reports_illegal_step_line() {
        let mut rec = started().to_record();
        rec.push_str("2g Ra1n\n"); // a2 is occupied
        let err = Game::parse(&rec).unwrap_err();
        assert_eq!(err.line, 3);
        assert!(matches!(err.error, GameError::Step { .. }));
    }

    #[test]
    fn parse_rejects_wrong_capture_token() {
        let mut rec = started().to_record();
        rec.push_str("2g Ee2n Ee3x\n");
        let err = Game::parse(&rec).unwrap_err();
        assert!(matches!(err.error, GameError::CaptureMismatch(_)));
    }

    #[test]
    fn parse_accepts_trailing_empty_move_and_marker() {
        let mut rec = started().to_record();
        rec.push_str("2g Ee2n\n2s resigns\n");
        let g = Game::parse(&rec).unwrap();
        assert_eq!(g.ply_count(), 3);
        assert_eq!(g.end_marker(), Some("resigns"));
        rec.push_str("3g Ee3n\n");
        assert!(Game::parse(&rec).is_err());
    }
}
