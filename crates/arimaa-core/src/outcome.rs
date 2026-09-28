//! Game-ending conditions.

use crate::position::Position;
use crate::turn::TurnBuilder;
use crate::types::{Color, Piece, PieceKind, data_type};

data_type! {
    /// How a game ended. The first three come from the position; the rest
    /// are decided outside the rules (clocks, players, controllers).
    #[cfg_attr(feature = "serde", serde(rename_all = "camelCase"))]
    pub enum WinReason {
        /// A rabbit reached its goal rank.
        Goal,
        /// The other side lost all its rabbits.
        Elimination,
        /// The other side had no legal move.
        Immobilization,
        /// The other side ran out of time.
        Timeout,
        /// The other side resigned.
        Resignation,
        /// The other side tried an illegal move (including a third repetition).
        IllegalMove,
        /// A game or turn limit was reached and the winner was decided by score.
        Score,
        /// The other side forfeited (e.g. left the game).
        Forfeit,
    }
}

impl WinReason {
    /// The letter arimaa.com and AEI tools use for this end condition.
    pub fn letter(self) -> char {
        match self {
            WinReason::Goal => 'g',
            WinReason::Elimination => 'e',
            WinReason::Immobilization => 'm',
            WinReason::Timeout => 't',
            WinReason::Resignation => 'r',
            WinReason::IllegalMove => 'i',
            WinReason::Score => 's',
            WinReason::Forfeit => 'f',
        }
    }

    pub fn from_letter(c: char) -> Option<WinReason> {
        [
            WinReason::Goal,
            WinReason::Elimination,
            WinReason::Immobilization,
            WinReason::Timeout,
            WinReason::Resignation,
            WinReason::IllegalMove,
            WinReason::Score,
            WinReason::Forfeit,
        ]
        .into_iter()
        .find(|r| r.letter() == c.to_ascii_lowercase())
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

/// True if the side to move in `pos` has no legal step. Any legal first
/// step can be extended into a legal turn (a lone step changes the position,
/// and a push can always be finished), so this is "has no legal move",
/// ignoring the repetition rule.
pub fn is_immobilized(pos: &Position) -> bool {
    TurnBuilder::new(pos).legal_steps().is_empty()
}

/// Checks for the end of the game after `mover` completes a turn, in the
/// official order: mover's goal, opponent's goal, opponent eliminated, mover
/// eliminated, opponent immobilized. `pos` has the opponent to move.
///
/// Repetition is enforced by [`crate::Game`], which has the position history.
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
    if is_immobilized(pos) {
        // TODO(rules): strictly, a player whose only legal moves would all be
        // third repetitions is also immobilized. That needs full move
        // generation plus the game history; pyrimaa doesn't check it either.
        return win(mover, WinReason::Immobilization);
    }
    None
}

/// Winner when a game or turn limit is reached: the side with more pieces,
/// silver on a tie.
// TODO(rules): arimaa.com's scoring for limit games is more detailed. This
// is pyrimaa's piece count.
pub fn limit_score_winner(pos: &Position) -> Color {
    let gold = pos.occupied_by(Color::Gold).count_ones();
    let silver = pos.occupied_by(Color::Silver).count_ones();
    if gold > silver { Color::Gold } else { Color::Silver }
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
    fn double_goal_goes_to_mover() {
        // Both sides have a rabbit on goal (pyrimaa's DOUBLE_GOAL_POS idea).
        let p = pos(Color::Gold, "Rb8 rg1");
        assert_eq!(outcome_after_turn(&p, Color::Gold).unwrap().winner, Color::Gold);
        assert_eq!(outcome_after_turn(&p, Color::Silver).unwrap().winner, Color::Silver);
    }

    #[test]
    fn double_elimination_goes_to_mover() {
        // Neither side has rabbits left (pyrimaa's DOUBLE_RABBIT_LOSS).
        let p = pos(Color::Gold, "Db6 cd6 mf5 dc7 df2");
        for mover in Color::ALL {
            let r = outcome_after_turn(&p, mover).unwrap();
            assert_eq!(r, GameResult { winner: mover, reason: WinReason::Elimination });
        }
    }

    #[test]
    fn immobilization() {
        // Silver's only piece is a rabbit on a8, frozen by the gold cats next
        // to it, so silver has no legal step.
        let p = pos(Color::Silver, "ra8 Ca7 Cb8 Rd4");
        assert!(is_immobilized(&p));
        let r = outcome_after_turn(&p, Color::Gold);
        assert_eq!(r, Some(GameResult { winner: Color::Gold, reason: WinReason::Immobilization }));
        // One cat is enough to freeze it.
        assert!(is_immobilized(&pos(Color::Silver, "ra8 Ca7 Rd4")));
        // Unfrozen, it can step east.
        assert!(!is_immobilized(&pos(Color::Silver, "ra8 Rd4")));
        // Any other silver piece that can move is enough.
        assert!(!is_immobilized(&pos(Color::Silver, "ra8 Ca7 Cb8 rh8 Rd4")));
    }

    #[test]
    fn reason_letters_round_trip() {
        for c in "gemtrisf".chars() {
            assert_eq!(WinReason::from_letter(c).unwrap().letter(), c);
        }
        assert_eq!(WinReason::from_letter('x'), None);
    }

    #[test]
    fn goal_beats_elimination() {
        let r = outcome_after_turn(&pos(Color::Silver, "Rd8 ed5"), Color::Gold);
        assert_eq!(r.unwrap().reason, WinReason::Goal);
    }
}
