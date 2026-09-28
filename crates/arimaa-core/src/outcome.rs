//! Game-ending conditions.

use crate::position::Position;
use crate::types::{Color, Piece, PieceKind, data_type};

data_type! {
    #[cfg_attr(feature = "serde", serde(rename_all = "camelCase"))]
    pub enum WinReason {
        /// A rabbit reached its goal rank.
        Goal,
        /// The other side lost all its rabbits.
        Elimination,
    }
}

data_type! {
    pub struct GameResult {
        pub winner: Color,
        pub reason: WinReason,
    }
}

fn rabbit_on_goal(pos: &Position, color: Color) -> bool {
    let goal_rank = 0xFFu64 << (8 * color.goal_rank());
    pos.bitboard(Piece::new(color, PieceKind::Rabbit)) & goal_rank != 0
}

fn has_rabbits(pos: &Position, color: Color) -> bool {
    pos.count(Piece::new(color, PieceKind::Rabbit)) > 0
}

/// Checks for the end of the game after `mover` completes a turn, in the
/// official order: mover's goal, opponent's goal, opponent eliminated, mover
/// eliminated.
pub fn outcome_after_turn(pos: &Position, mover: Color) -> Option<GameResult> {
    let opp = mover.opponent();
    let win = |winner, reason| Some(GameResult { winner, reason });
    if rabbit_on_goal(pos, mover) {
        return win(mover, WinReason::Goal);
    }
    if rabbit_on_goal(pos, opp) {
        return win(opp, WinReason::Goal);
    }
    if !has_rabbits(pos, opp) {
        return win(mover, WinReason::Elimination);
    }
    if !has_rabbits(pos, mover) {
        return win(opp, WinReason::Elimination);
    }
    // TODO(rules): immobilization. If the opponent has no legal move, the
    // mover wins. Needs full move generation (not just steps).
    // TODO(rules): repetition. A turn may not produce the same position with
    // the same side to move for the third time. Needs the game's hash history.
    None
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::position::tests::pos;

    #[test]
    fn no_result_midgame() {
        assert_eq!(outcome_after_turn(&pos(Color::Silver, "Rd4 rd5"), Color::Gold), None);
    }

    #[test]
    fn gold_goal() {
        let r = outcome_after_turn(&pos(Color::Silver, "Rd8 rd5"), Color::Gold);
        assert_eq!(r, Some(GameResult { winner: Color::Gold, reason: WinReason::Goal }));
    }

    #[test]
    fn silver_goal() {
        let r = outcome_after_turn(&pos(Color::Gold, "Rd4 rd1"), Color::Silver);
        assert_eq!(r, Some(GameResult { winner: Color::Silver, reason: WinReason::Goal }));
    }

    #[test]
    fn opponent_goal_counts_for_opponent() {
        // Gold pushes a silver rabbit onto rank 1: silver wins.
        let r = outcome_after_turn(&pos(Color::Silver, "Ed2 Rd4 rd1"), Color::Gold);
        assert_eq!(r, Some(GameResult { winner: Color::Silver, reason: WinReason::Goal }));
    }

    #[test]
    fn mover_goal_takes_priority() {
        let r = outcome_after_turn(&pos(Color::Silver, "Ra8 ra1"), Color::Gold);
        assert_eq!(r.unwrap().winner, Color::Gold);
    }

    #[test]
    fn elimination() {
        let r = outcome_after_turn(&pos(Color::Silver, "Rd4 ed5"), Color::Gold);
        assert_eq!(r, Some(GameResult { winner: Color::Gold, reason: WinReason::Elimination }));
        // Mover losing their own last rabbit loses.
        let r = outcome_after_turn(&pos(Color::Silver, "Ed4 rd5"), Color::Gold);
        assert_eq!(r, Some(GameResult { winner: Color::Silver, reason: WinReason::Elimination }));
    }

    #[test]
    fn goal_beats_elimination() {
        let r = outcome_after_turn(&pos(Color::Silver, "Rd8 ed5"), Color::Gold);
        assert_eq!(r.unwrap().reason, WinReason::Goal);
    }
}
