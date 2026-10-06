//! Game-ending conditions.

use std::collections::{HashMap, HashSet};

use crate::position::Position;
use crate::turn::{Pending, TurnBuilder};
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
    /// True for the reasons the position itself decides (goal, elimination,
    /// immobilization). After these no move is possible; after the others
    /// (a resignation, a timeout, ...) the position can still be played on
    /// in analysis.
    pub fn is_on_board(self) -> bool {
        matches!(self, WinReason::Goal | WinReason::Elimination | WinReason::Immobilization)
    }

    /// The letter arimaa.com and AEI tools use for this end condition
    /// (the full arimaa.com list is in `docs/RESULT-CODES.md`).
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
        // A player whose only moves would all be third repetitions is
        // immobilized too: see `outcome_with_history`.
        return win(mover, WinReason::Immobilization);
    }
    None
}

/// [`outcome_after_turn`], plus the repetition rule's part in
/// immobilization: a player whose only legal moves would all repeat a
/// position for the third time can't move, and loses (decided for Howdah;
/// pyrimaa doesn't check it, and arimaa.com probably catches only the
/// simple case of a single otherwise legal move). `history` is the
/// positions that count for repetition, `pos` included: every position
/// after the setups, up to and including `pos`.
pub fn outcome_with_history<'a>(
    pos: &Position,
    mover: Color,
    history: impl IntoIterator<Item = &'a Position>,
) -> Option<GameResult> {
    outcome_after_turn(pos, mover).or_else(|| {
        only_repetitions(pos, history)
            .then_some(GameResult { winner: mover, reason: WinReason::Immobilization })
    })
}

/// True if the side to move in `pos` has legal moves but every one would
/// be a third repetition: its end is already in `history` twice.
pub(crate) fn only_repetitions<'a>(pos: &Position, history: impl IntoIterator<Item = &'a Position>) -> bool {
    // Turn ends have the other side to move.
    let mut counts: HashMap<&Position, u32> = HashMap::new();
    for p in history.into_iter().filter(|p| p.side_to_move() != pos.side_to_move()) {
        *counts.entry(p).or_default() += 1;
    }
    let twice: HashSet<&Position> = counts.into_iter().filter(|&(_, n)| n >= 2).map(|(p, _)| p).collect();
    // Usually nothing has been seen twice, and every move is fine.
    if twice.is_empty() || is_immobilized(pos) {
        return false;
    }
    !has_move_avoiding(&mut TurnBuilder::new(pos), &twice, &mut HashSet::new())
}

/// Whether some turn from `tb` on ends outside `banned`: a depth-first
/// search that stops at the first such turn, skipping states already
/// searched.
fn has_move_avoiding(
    tb: &mut TurnBuilder,
    banned: &HashSet<&Position>,
    searched: &mut HashSet<(Position, usize, Pending)>,
) -> bool {
    if let Ok(turn) = tb.clone().finish()
        && !banned.contains(&turn.end)
    {
        return true;
    }
    if tb.steps_left() == 0 || !searched.insert(tb.search_key()) {
        return false;
    }
    for (step, _) in tb.legal_steps() {
        tb.try_step(step).expect("a legal step");
        let found = has_move_avoiding(tb, banned, searched);
        tb.undo();
        if found {
            return true;
        }
    }
    false
}

/// Winner when a game or turn limit is reached: the side with more pieces,
/// silver on a tie.
// TODO(rules): the official rule (docs/RESULT-CODES.md) looks back: if the
// counts are equal now, the side with more pieces after the most recent
// turn where they differed wins, and silver only if they never differed.
// This is pyrimaa's piece count on the final position.
pub fn limit_score_winner(pos: &Position) -> Color {
    let gold = pos.occupied_by(Color::Gold).count_ones();
    let silver = pos.occupied_by(Color::Silver).count_ones();
    if gold > silver { Color::Gold } else { Color::Silver }
}

#[cfg(test)]
pub(crate) mod tests {
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

    /// Every position a turn from `p` can end in.
    pub(crate) fn turn_ends(p: &Position) -> HashSet<Position> {
        fn collect(tb: &mut TurnBuilder, out: &mut HashSet<Position>) {
            if let Ok(turn) = tb.clone().finish() {
                out.insert(turn.end);
            }
            for (step, _) in tb.legal_steps() {
                tb.try_step(step).unwrap();
                collect(tb, out);
                tb.undo();
            }
        }
        let mut out = HashSet::new();
        collect(&mut TurnBuilder::new(p), &mut out);
        out
    }

    // Silver's cat can wander; its rabbit is frozen by the elephant.
    pub(crate) const WANDERING_CAT: &str = "ch8 Cg8 rb2 Eb1 Ra1";

    #[test]
    fn only_third_repetitions_left_is_immobilization() {
        let p = pos(Color::Silver, WANDERING_CAT);
        let ends = turn_ends(&p);
        assert!(ends.len() > 10, "silver has moves: {}", ends.len());
        let history = ends.iter().chain(ends.iter()).chain([&p]);
        let r = outcome_with_history(&p, Color::Gold, history);
        assert_eq!(r, Some(GameResult { winner: Color::Gold, reason: WinReason::Immobilization }));
    }

    #[test]
    fn one_move_that_isnt_a_third_repetition_is_enough() {
        let p = pos(Color::Silver, WANDERING_CAT);
        let ends = turn_ends(&p);
        for spare in &ends {
            let history = ends.iter().chain(ends.iter().filter(|e| *e != spare)).chain([&p]);
            assert_eq!(outcome_with_history(&p, Color::Gold, history), None, "{}", spare.to_short_string());
        }
        assert_eq!(outcome_with_history(&p, Color::Gold, [&p]), None);
    }

    #[test]
    fn repetitions_count_only_positions_with_the_other_side_to_move() {
        // The same boards with silver to move are other positions.
        let p = pos(Color::Silver, WANDERING_CAT);
        let flipped: Vec<Position> = turn_ends(&p)
            .into_iter()
            .map(|mut e| {
                e.set_side_to_move(Color::Silver);
                e
            })
            .collect();
        let history = flipped.iter().chain(flipped.iter()).chain([&p]);
        assert_eq!(outcome_with_history(&p, Color::Gold, history), None);
    }

    #[test]
    fn plain_immobilization_needs_no_history() {
        // Silver has no move at all: plain immobilization, history or not.
        let p = pos(Color::Silver, "ra8 Ca7 Rd4");
        let r = outcome_with_history(&p, Color::Gold, [&p]);
        assert_eq!(r, Some(GameResult { winner: Color::Gold, reason: WinReason::Immobilization }));
    }
}
