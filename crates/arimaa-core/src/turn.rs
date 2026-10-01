//! Building a turn step by step, with full legality checking.

use crate::error::{CommitError, StepError};
use crate::position::{Position, neighbors};
use crate::step::{Step, StepEffect};
use crate::types::{Dir, PieceKind, Square, data_type};

/// Maximum steps in one turn.
pub const MAX_STEPS: usize = 4;

data_type! {
    /// How a step fits into the turn.
    #[cfg_attr(feature = "serde", serde(rename_all = "camelCase"))]
    pub enum StepKind {
        /// A friendly piece moving on its own (possibly starting a pull).
        Simple,
        /// An enemy piece displaced by a stronger friendly piece, which must
        /// step into the vacated square next.
        PushStart,
        /// The pushing piece stepping into the square the enemy vacated.
        PushFinish,
        /// An enemy piece dragged into the square a stronger friendly piece
        /// just vacated.
        PullFinish,
    }
}

/// What the previous step lets or requires the next step do.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Pending {
    None,
    /// A push is under way: one of `pushers` must step into `vacated`.
    /// Recording the eligible pushers at the start is equivalent to checking
    /// the finishing piece then: the pushed piece's step can't change any
    /// friendly piece's frozen status (see `enemy_step_cannot_change_pusher_eligibility`).
    Push {
        vacated: Square,
        pushers: u64,
    },
    /// The previous step was a friendly piece leaving `vacated`; a weaker
    /// enemy next to that square may be pulled into it. This holds even if
    /// the puller was captured on a trap by its own step.
    Pull {
        vacated: Square,
        puller: PieceKind,
    },
}

/// A step taken in the turn, with its role.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TurnStep {
    pub effect: StepEffect,
    pub kind: StepKind,
    after: Pending,
}

/// A completed, legal turn.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Turn {
    pub steps: Vec<TurnStep>,
    /// Position before the turn.
    pub start: Position,
    /// Position after the turn, with the other side to move.
    pub end: Position,
}

impl Turn {
    pub fn effects(&self) -> Vec<StepEffect> {
        self.steps.iter().map(|s| s.effect).collect()
    }
}

/// One piece walking on its own, as found by [`TurnBuilder::shortest_routes`].
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Route {
    pub steps: Vec<Step>,
    /// Friendly pieces lost along the way: left alone on a trap when the
    /// walking piece stepped away.
    pub captures: usize,
}

impl Route {
    /// The square the walk ends on.
    pub fn end(&self) -> Option<Square> {
        self.steps.last().and_then(Step::to)
    }

    /// The squares the piece enters, in order.
    pub fn squares(&self) -> Vec<Square> {
        self.steps.iter().filter_map(Step::to).collect()
    }

    /// The route that enters the most squares of `path` (e.g. the squares a
    /// piece was dragged across), then the one losing the fewest friendly
    /// pieces. Ties go to the earliest route.
    pub fn best_along(routes: Vec<Route>, path: &[Square]) -> Option<Route> {
        let score = |r: &Route| {
            let followed = r.squares().iter().filter(|sq| path.contains(sq)).count();
            (followed, std::cmp::Reverse(r.captures))
        };
        routes.into_iter().fold(None, |best: Option<Route>, r| match best {
            Some(b) if score(&b) >= score(&r) => Some(b),
            _ => Some(r),
        })
    }
}

/// An in-progress turn for the side to move in `start`.
///
/// Push/pull interpretation: when an enemy step could be either the second
/// half of a pull or the first half of a push, it's treated as a pull. That
/// reading imposes no obligation on later steps and anything legal after the
/// push reading is also legal after the pull reading, so a step sequence is
/// accepted exactly when some interpretation of it is legal. A step that
/// finishes a push or a pull can't itself start a pull.
#[derive(Clone, Debug)]
pub struct TurnBuilder {
    start: Position,
    pos: Position,
    steps: Vec<TurnStep>,
}

impl TurnBuilder {
    pub fn new(start: &Position) -> TurnBuilder {
        TurnBuilder { start: start.clone(), pos: start.clone(), steps: Vec::new() }
    }

    pub fn start(&self) -> &Position {
        &self.start
    }

    /// Current position. The side to move stays the mover until the turn is finished.
    pub fn position(&self) -> &Position {
        &self.pos
    }

    pub fn steps(&self) -> &[TurnStep] {
        &self.steps
    }

    pub fn steps_left(&self) -> usize {
        MAX_STEPS - self.steps.len()
    }

    fn pending(&self) -> Pending {
        self.steps.last().map_or(Pending::None, |s| s.after)
    }

    /// The square a pushing piece must step into, if a push is under way.
    pub fn push_pending(&self) -> Option<Square> {
        match self.pending() {
            Pending::Push { vacated, .. } => Some(vacated),
            _ => None,
        }
    }

    /// Checks whether `step` is legal now, without applying it.
    pub fn classify(&self, step: Step) -> Result<StepKind, StepError> {
        self.check(step).map(|(kind, _)| kind)
    }

    fn check(&self, step: Step) -> Result<(StepKind, Pending), StepError> {
        let pos = &self.pos;
        let mover = pos.side_to_move();
        if self.steps.len() >= MAX_STEPS {
            return Err(StepError::TurnFull);
        }
        let piece = pos.piece_at(step.from).ok_or(StepError::NoPiece(step.from))?;
        if piece != step.piece {
            return Err(StepError::PieceMismatch { square: step.from, expected: step.piece, found: piece });
        }
        let to = step.to().ok_or(StepError::OffBoard)?;
        if pos.occupied() & to.bit() != 0 {
            return Err(StepError::Occupied(to));
        }
        let pending = self.pending();

        if piece.color == mover {
            if let Pending::Push { vacated, pushers } = pending {
                if to != vacated || pushers & step.from.bit() == 0 {
                    return Err(StepError::PushIncomplete(vacated));
                }
                return Ok((StepKind::PushFinish, Pending::None));
            }
            if pos.is_frozen(step.from) {
                return Err(StepError::Frozen(step.from));
            }
            if piece.kind == PieceKind::Rabbit && step.dir == mover.backward() {
                return Err(StepError::RabbitBackward);
            }
            let after = Pending::Pull { vacated: step.from, puller: piece.kind };
            return Ok((StepKind::Simple, after));
        }

        // Enemy piece: must be completing a pull or starting a push.
        if let Pending::Push { vacated, .. } = pending {
            return Err(StepError::PushIncomplete(vacated));
        }
        if let Pending::Pull { vacated, puller } = pending
            && to == vacated
            && puller > piece.kind
        {
            return Ok((StepKind::PullFinish, Pending::None));
        }
        let pushers = self.eligible_pushers(step.from);
        if pushers == 0 {
            return Err(StepError::CannotMoveEnemy(step.from));
        }
        if self.steps_left() < 2 {
            return Err(StepError::NoStepsForPush);
        }
        Ok((StepKind::PushStart, Pending::Push { vacated: step.from, pushers }))
    }

    /// Unfrozen friendly pieces next to `enemy_sq` that are stronger than the enemy there.
    fn eligible_pushers(&self, enemy_sq: Square) -> u64 {
        let pos = &self.pos;
        let Some(enemy) = pos.piece_at(enemy_sq) else { return 0 };
        let candidates = neighbors(enemy_sq.bit()) & pos.stronger_than(pos.side_to_move(), enemy.kind);
        crate::position::squares(candidates)
            .filter(|&sq| !pos.is_frozen(sq))
            .fold(0, |acc, sq| acc | sq.bit())
    }

    /// Applies `step` if it's legal.
    pub fn try_step(&mut self, step: Step) -> Result<&TurnStep, StepError> {
        let (kind, after) = self.check(step)?;
        let effect = self.pos.apply_step(step)?;
        self.steps.push(TurnStep { effect, kind, after });
        Ok(self.steps.last().unwrap())
    }

    /// Convenience for UIs: step the piece on `from` to the adjacent `to`.
    pub fn try_move(&mut self, from: Square, to: Square) -> Result<&TurnStep, StepError> {
        let piece = self.pos.piece_at(from).ok_or(StepError::NoPiece(from))?;
        let dir = from.direction_to(to).ok_or(StepError::NotAdjacent { from, to })?;
        self.try_step(Step::new(piece, from, dir))
    }

    /// Takes back the last step.
    pub fn undo(&mut self) -> Option<TurnStep> {
        let last = self.steps.pop()?;
        self.pos.undo_step(&last.effect);
        Some(last)
    }

    /// Every legal next step.
    pub fn legal_steps(&self) -> Vec<(Step, StepKind)> {
        self.pos.pieces().flat_map(|(sq, _)| self.legal_steps_from(sq)).collect()
    }

    /// Legal next steps for the piece on `from`.
    pub fn legal_steps_from(&self, from: Square) -> Vec<(Step, StepKind)> {
        let Some(piece) = self.pos.piece_at(from) else { return Vec::new() };
        Dir::ALL
            .into_iter()
            .map(|dir| Step::new(piece, from, dir))
            .filter_map(|step| self.classify(step).ok().map(|kind| (step, kind)))
            .collect()
    }

    /// Every way the friendly piece on `from` can walk to `to` on its own, with
    /// the fewest steps that the steps left allow. Routes are made of simple
    /// steps only (no pushes or pulls), each legal at the time it's taken, and
    /// the piece must survive every step. Empty if `to` can't be reached.
    pub fn shortest_routes(&self, from: Square, to: Square) -> Vec<Route> {
        let walks = self.walks(from);
        let shortest = walks.iter().filter(|r| r.end() == Some(to)).map(|r| r.steps.len()).min();
        walks.into_iter().filter(|r| r.end() == Some(to) && Some(r.steps.len()) == shortest).collect()
    }

    /// Squares the friendly piece on `from` can walk to on its own (as in
    /// [`TurnBuilder::shortest_routes`]), each with the fewest steps needed.
    pub fn reachable(&self, from: Square) -> Vec<(Square, usize)> {
        let mut best: Vec<(Square, usize)> = Vec::new();
        for route in self.walks(from) {
            let (end, len) = (route.end().expect("walks are non-empty"), route.steps.len());
            match best.iter_mut().find(|(sq, _)| *sq == end) {
                Some(entry) => entry.1 = entry.1.min(len),
                None => best.push((end, len)),
            }
        }
        best
    }

    /// Every walk of the friendly piece on `from` that never revisits a square
    /// (a shortest route never does), up to the steps left.
    fn walks(&self, from: Square) -> Vec<Route> {
        let mut out = Vec::new();
        let mut route = Route { steps: Vec::new(), captures: 0 };
        self.extend_walk(from, &mut route, &mut out);
        out
    }

    fn extend_walk(&self, at: Square, route: &mut Route, out: &mut Vec<Route>) {
        let Some(piece) = self.pos.piece_at(at) else { return };
        if piece.color != self.pos.side_to_move() {
            return;
        }
        for dir in Dir::ALL {
            let step = Step::new(piece, at, dir);
            let Some(next) = step.to() else { continue };
            let revisit = route.steps.iter().any(|s| s.from == next);
            if revisit || !matches!(self.classify(step), Ok(StepKind::Simple)) {
                continue;
            }
            let mut tb = self.clone();
            let captured = tb.try_step(step).expect("classified as legal").effect.capture.is_some();
            if tb.pos.piece_at(next) != Some(piece) {
                continue; // the piece itself was captured on a trap
            }
            route.steps.push(step);
            route.captures += usize::from(captured);
            out.push(route.clone());
            tb.extend_walk(next, route, out);
            route.captures -= usize::from(captured);
            route.steps.pop();
        }
    }

    /// Checks whether the turn could end now.
    pub fn can_finish(&self) -> Result<(), CommitError> {
        if self.steps.is_empty() {
            return Err(CommitError::NoSteps);
        }
        if self.push_pending().is_some() {
            return Err(CommitError::PushIncomplete);
        }
        if self.pos.same_board(&self.start) {
            return Err(CommitError::NoChange);
        }
        // TODO(rules): a turn that repeats a position for the third time is
        // illegal. Needs the game's position history, so it belongs in Game.
        Ok(())
    }

    /// Ends the turn, handing the move to the other side.
    pub fn finish(self) -> Result<Turn, CommitError> {
        self.can_finish()?;
        let mut end = self.pos;
        end.set_side_to_move(end.side_to_move().opponent());
        Ok(Turn { steps: self.steps, start: self.start, end })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::position::tests::{pos, sq, step};
    use crate::types::Color;

    fn builder(side: Color, spec: &str) -> TurnBuilder {
        TurnBuilder::new(&pos(side, spec))
    }

    fn kinds(tb: &TurnBuilder) -> Vec<StepKind> {
        tb.steps().iter().map(|s| s.kind).collect()
    }

    // --- simple steps and freezing ---

    #[test]
    fn simple_step() {
        let mut tb = builder(Color::Gold, "Ed4");
        assert_eq!(tb.try_step(step("Ed4n")).unwrap().kind, StepKind::Simple);
        assert_eq!(tb.position(), &pos(Color::Gold, "Ed5"));
    }

    #[test]
    fn frozen_piece_cannot_move() {
        let mut tb = builder(Color::Gold, "Dd4 hd5");
        assert_eq!(tb.try_step(step("Dd4s")), Err(StepError::Frozen(sq("d4"))));
    }

    #[test]
    fn friend_unfreezes_then_moves() {
        let mut tb = builder(Color::Gold, "Dd4 hd5 Rb4");
        assert!(tb.try_step(step("Dd4s")).is_err());
        tb.try_step(step("Rb4e")).unwrap();
        tb.try_step(step("Dd4s")).unwrap();
    }

    #[test]
    fn friend_leaving_refreezes() {
        let mut tb = builder(Color::Gold, "Dd4 hd5 Rc4");
        tb.try_step(step("Rc4n")).unwrap();
        assert_eq!(tb.try_step(step("Dd4s")), Err(StepError::Frozen(sq("d4"))));
    }

    #[test]
    fn friend_lost_on_trap_refreezes() {
        // The dog's only friend steps onto an unguarded trap and is captured.
        let mut tb = builder(Color::Gold, "Dd4 he4 Cc4");
        assert!(!tb.position().is_frozen(sq("d4")));
        let s = tb.try_step(step("Cc4s")).unwrap();
        assert_eq!(s.effect.capture.map(|c| c.square), Some(sq("c3")));
        assert_eq!(tb.try_step(step("Dd4n")), Err(StepError::Frozen(sq("d4"))));
    }

    #[test]
    fn capturing_the_freezer_unfreezes() {
        // The silver horse on c3 freezes the gold dog and is guarded only by
        // the rabbit on b3. Pushing the rabbit away captures the horse.
        let mut tb = builder(Color::Gold, "Dd3 hc3 rb3 Ea3");
        assert!(tb.position().is_frozen(sq("d3")));
        let s = tb.try_step(step("rb3n")).unwrap();
        assert_eq!(s.effect.capture.map(|c| c.piece.letter()), Some('h'));
        tb.try_step(step("Ea3e")).unwrap();
        tb.try_step(step("Dd3n")).unwrap();
    }

    #[test]
    fn rabbits_cannot_step_backward() {
        let mut tb = builder(Color::Gold, "Rd4");
        assert_eq!(tb.try_step(step("Rd4s")), Err(StepError::RabbitBackward));
        tb.try_step(step("Rd4e")).unwrap();
        let mut tb = builder(Color::Silver, "rd4");
        assert_eq!(tb.try_step(step("rd4n")), Err(StepError::RabbitBackward));
        tb.try_step(step("rd4s")).unwrap();
    }

    #[test]
    fn other_pieces_can_step_backward() {
        let mut tb = builder(Color::Gold, "Cd4");
        tb.try_step(step("Cd4s")).unwrap();
    }

    #[test]
    fn at_most_four_steps() {
        let mut tb = builder(Color::Gold, "Ea1");
        for s in ["Ea1n", "Ea2n", "Ea3n", "Ea4n"] {
            tb.try_step(step(s)).unwrap();
        }
        assert_eq!(tb.try_step(step("Ea5n")), Err(StepError::TurnFull));
    }

    #[test]
    fn cannot_move_enemy_without_pusher_or_puller() {
        let mut tb = builder(Color::Gold, "Rd4 cd5");
        assert_eq!(tb.try_step(step("cd5n")), Err(StepError::CannotMoveEnemy(sq("d5"))));
    }

    // --- pushes ---

    #[test]
    fn push() {
        let mut tb = builder(Color::Gold, "Ed4 rd5");
        assert_eq!(tb.try_step(step("rd5n")).unwrap().kind, StepKind::PushStart);
        assert_eq!(tb.push_pending(), Some(sq("d5")));
        assert_eq!(tb.can_finish(), Err(CommitError::PushIncomplete));
        assert_eq!(tb.try_step(step("Ed4n")).unwrap().kind, StepKind::PushFinish);
        assert_eq!(tb.push_pending(), None);
        let turn = tb.finish().unwrap();
        assert_eq!(turn.end, pos(Color::Silver, "Ed5 rd6"));
    }

    #[test]
    fn enemy_rabbits_can_be_moved_backward() {
        // North is backward for silver rabbits, but pushes and pulls may move
        // them in any direction.
        let mut tb = builder(Color::Gold, "Ed4 rd5");
        assert_eq!(tb.try_step(step("rd5n")).unwrap().kind, StepKind::PushStart);
        let mut tb = builder(Color::Gold, "Ed5 rd4");
        tb.try_step(step("Ed5n")).unwrap();
        assert_eq!(tb.try_step(step("rd4n")).unwrap().kind, StepKind::PullFinish);
    }

    #[test]
    fn push_must_be_finished_by_the_pusher_into_the_vacated_square() {
        let mut tb = builder(Color::Gold, "Ed4 rd5 Ra1");
        tb.try_step(step("rd5e")).unwrap();
        // Another piece can't move instead.
        assert_eq!(tb.try_step(step("Ra1n")), Err(StepError::PushIncomplete(sq("d5"))));
        // The pusher can't go elsewhere.
        assert_eq!(tb.try_step(step("Ed4w")), Err(StepError::PushIncomplete(sq("d5"))));
        tb.try_step(step("Ed4n")).unwrap();
    }

    #[test]
    fn push_needs_stronger_piece() {
        let mut tb = builder(Color::Gold, "Dd4 dd5");
        assert_eq!(tb.try_step(step("dd5n")), Err(StepError::CannotMoveEnemy(sq("d5"))));
    }

    #[test]
    fn frozen_piece_cannot_push() {
        // Gold dog is frozen by the silver camel and can't push the cat.
        let mut tb = builder(Color::Gold, "Dd4 cd5 mc4");
        assert!(tb.position().is_frozen(sq("d4")));
        assert_eq!(tb.try_step(step("cd5n")), Err(StepError::CannotMoveEnemy(sq("d5"))));
    }

    #[test]
    fn frozen_enemy_can_be_pushed() {
        let mut tb = builder(Color::Gold, "Ed4 rd5");
        assert!(tb.position().is_frozen(sq("d5")));
        tb.try_step(step("rd5w")).unwrap();
        tb.try_step(step("Ed4n")).unwrap();
    }

    #[test]
    fn push_cannot_start_on_last_step() {
        let mut tb = builder(Color::Gold, "Ea1 Hd4 rd5");
        for s in ["Ea1n", "Ea2n", "Ea3n"] {
            tb.try_step(step(s)).unwrap();
        }
        assert_eq!(tb.try_step(step("rd5n")), Err(StepError::NoStepsForPush));
    }

    #[test]
    fn push_into_trap_captures() {
        let mut tb = builder(Color::Gold, "Ec5 dc4");
        let s = tb.try_step(step("dc4s")).unwrap().clone();
        assert_eq!(s.kind, StepKind::PushStart);
        assert_eq!(s.effect.capture.map(|c| c.piece.letter()), Some('d'));
        tb.try_step(step("Ec5s")).unwrap();
        assert_eq!(tb.finish().unwrap().end, pos(Color::Silver, "Ec4"));
    }

    #[test]
    fn push_away_trap_guard_captures_enemy() {
        // Pushing the silver dog off d3 leaves the silver cat on c3 alone.
        let mut tb = builder(Color::Gold, "Ed2 dd3 cc3");
        let s = tb.try_step(step("dd3e")).unwrap();
        assert_eq!(s.effect.capture.map(|c| c.square), Some(sq("c3")));
        tb.try_step(step("Ed2n")).unwrap();
    }

    #[test]
    fn frozen_piece_cannot_finish_a_push() {
        // Gold horse on e4 is frozen by the silver camel on e5. The push is
        // made by the elephant; the horse can't finish it.
        let mut tb = builder(Color::Gold, "Ec4 He4 dd4 me5");
        assert!(tb.position().is_frozen(sq("e4")));
        tb.try_step(step("dd4s")).unwrap();
        assert_eq!(tb.try_step(step("He4w")), Err(StepError::PushIncomplete(sq("d4"))));
        tb.try_step(step("Ec4e")).unwrap();
    }

    /// A push's first step moves an enemy piece from `s` to a neighbour `t`,
    /// possibly capturing an enemy on a trap next to `s`. Friendly pieces
    /// next to `s` are never orthogonally adjacent to `t` or to other
    /// neighbours of `s` (the grid is bipartite), so neither arrival nor
    /// capture touches a candidate pusher. The pushed piece is weaker than
    /// any pusher, so it wasn't freezing one and its leaving changes nothing.
    /// (Other friendly pieces can change status: weaker ones the pushed piece
    /// froze, or ones next to a captured piece as in
    /// `capturing_the_freezer_unfreezes`. None of those can push.)
    /// Check every push start in a few positions: each candidate pusher
    /// (friendly, adjacent, stronger) keeps its frozen status.
    #[test]
    fn enemy_step_cannot_change_pusher_eligibility() {
        let positions = [
            "Dd3 hc3 rb3 Ea3",     // pushing the trap guard captures the horse
            "Ed2 dd3 cc3 Hc4 me4", // pushing a guard away from c3
            "Ec5 dc4 Hb4 mb5 Rd4", // pushing onto a trap
            "Ee4 Mf3 rf4 hg3 cf2 De2",
        ];
        for spec in positions {
            let tb = builder(Color::Gold, spec);
            let before = tb.position().clone();
            for (step, kind) in tb.legal_steps() {
                if kind != StepKind::PushStart {
                    continue;
                }
                let mut after = tb.clone();
                after.try_step(step).unwrap();
                for (sq, piece) in before.pieces() {
                    let candidate = piece.color == Color::Gold
                        && piece.stronger_than(step.piece)
                        && step.from.neighbors().any(|n| n == sq);
                    if candidate {
                        assert_eq!(
                            before.is_frozen(sq),
                            after.position().is_frozen(sq),
                            "{spec}: {step} changed frozen status on {sq}"
                        );
                    }
                }
            }
        }
    }

    #[test]
    fn push_finish_cannot_start_a_pull() {
        // The elephant pushes rd5 north, vacating d4 as it finishes the push.
        let mut tb = builder(Color::Gold, "Ed4 rd5 re4");
        tb.try_step(step("rd5n")).unwrap();
        tb.try_step(step("Ed4n")).unwrap();
        // re4 is next to d4, but the push finish can't double as a pull,
        // and the elephant on d5 isn't adjacent to push it.
        assert_eq!(tb.try_step(step("re4w")), Err(StepError::CannotMoveEnemy(sq("e4"))));
    }

    // --- pulls ---

    #[test]
    fn pull() {
        let mut tb = builder(Color::Gold, "Ed4 rd5");
        tb.try_step(step("Ed4s")).unwrap();
        assert_eq!(tb.try_step(step("rd5s")).unwrap().kind, StepKind::PullFinish);
        assert_eq!(tb.finish().unwrap().end, pos(Color::Silver, "Ed3 rd4"));
    }

    #[test]
    fn pull_is_optional() {
        let mut tb = builder(Color::Gold, "Ed4 rd5");
        tb.try_step(step("Ed4s")).unwrap();
        tb.can_finish().unwrap();
        tb.try_step(step("Ed3s")).unwrap();
        tb.can_finish().unwrap();
    }

    #[test]
    fn pull_needs_stronger_puller() {
        let mut tb = builder(Color::Gold, "Dd4 dd5 Ra1");
        tb.try_step(step("Dd4s")).unwrap();
        assert_eq!(tb.try_step(step("dd5s")), Err(StepError::CannotMoveEnemy(sq("d5"))));
    }

    #[test]
    fn pull_only_into_vacated_square() {
        let mut tb = builder(Color::Gold, "Ed4 rc4");
        tb.try_step(step("Ed4s")).unwrap();
        // c4 is next to d4 (vacated) so pulling east works...
        let mut tb2 = tb.clone();
        tb2.try_step(step("rc4e")).unwrap();
        // ...but moving it north isn't a pull, and there's no pusher.
        assert_eq!(tb.try_step(step("rc4n")), Err(StepError::CannotMoveEnemy(sq("c4"))));
    }

    #[test]
    fn pull_must_come_immediately() {
        let mut tb = builder(Color::Gold, "Ed4 rd5 Ra1");
        tb.try_step(step("Ed4s")).unwrap();
        tb.try_step(step("Ra1n")).unwrap();
        assert_eq!(tb.try_step(step("rd5s")), Err(StepError::CannotMoveEnemy(sq("d5"))));
    }

    #[test]
    fn pull_out_of_trap() {
        // Silver cat on c3, guarded by the dog on b3. The elephant steps
        // north and drags the cat off the trap into c4.
        let mut tb = builder(Color::Gold, "Ec4 cc3 db3");
        tb.try_step(step("Ec4n")).unwrap();
        assert_eq!(tb.try_step(step("cc3n")).unwrap().kind, StepKind::PullFinish);
        assert_eq!(tb.position(), &pos(Color::Gold, "Ec5 cc4 db3"));
    }

    #[test]
    fn pull_onto_trap_captures() {
        let mut tb = builder(Color::Gold, "Ec3 Rb3 rc4");
        // Elephant on guarded trap steps south; rabbit is pulled onto the
        // trap and captured (no silver neighbors).
        tb.try_step(step("Ec3s")).unwrap();
        let s = tb.try_step(step("rc4s")).unwrap();
        assert_eq!(s.kind, StepKind::PullFinish);
        assert_eq!(s.effect.capture.map(|c| c.piece.letter()), Some('r'));
    }

    #[test]
    fn captured_puller_can_still_pull() {
        // Gold horse steps onto an unguarded trap and dies, but still pulls.
        let mut tb = builder(Color::Gold, "Hc4 rc5");
        let s = tb.try_step(step("Hc4s")).unwrap();
        assert!(s.effect.capture.is_some());
        assert_eq!(tb.try_step(step("rc5s")).unwrap().kind, StepKind::PullFinish);
    }

    #[test]
    fn pull_finish_cannot_start_another_pull() {
        let mut tb = builder(Color::Gold, "Ed4 rd5 rc5");
        tb.try_step(step("Ed4s")).unwrap();
        tb.try_step(step("rd5s")).unwrap();
        // d5 was vacated by an enemy step, so rc5 can't be "pulled" into it.
        assert_eq!(tb.try_step(step("rc5e")), Err(StepError::CannotMoveEnemy(sq("c5"))));
    }

    #[test]
    fn frozen_piece_cannot_pull() {
        let mut tb = builder(Color::Gold, "Hd4 rd5 ed3");
        assert!(tb.position().is_frozen(sq("d4")));
        assert_eq!(tb.try_step(step("Hd4e")), Err(StepError::Frozen(sq("d4"))));
    }

    // --- ambiguity ---

    #[test]
    fn enemy_step_that_could_be_pull_or_push_is_a_pull() {
        // E leaves d4 for c4. rd5 can then move into d4 either as a pull by
        // E or as the start of a push by the camel on e5.
        let mut tb = builder(Color::Gold, "Ed4 rd5 Me5");
        tb.try_step(step("Ed4w")).unwrap();
        assert_eq!(tb.try_step(step("rd5s")).unwrap().kind, StepKind::PullFinish);
        // No push obligation, so the turn can end and any piece can move.
        tb.can_finish().unwrap();
        assert_eq!(tb.push_pending(), None);
        // ...and the camel may still step into d5 voluntarily.
        tb.try_step(step("Me5w")).unwrap();
    }

    #[test]
    fn enemy_push_after_simple_step_elsewhere() {
        let mut tb = builder(Color::Gold, "Ed4 rd5 Ra1");
        tb.try_step(step("Ra1n")).unwrap();
        assert_eq!(tb.try_step(step("rd5e")).unwrap().kind, StepKind::PushStart);
    }

    // --- turn completion ---

    #[test]
    fn empty_turn_rejected() {
        let tb = builder(Color::Gold, "Ed4");
        assert_eq!(tb.can_finish(), Err(CommitError::NoSteps));
    }

    #[test]
    fn net_null_turn_rejected() {
        let mut tb = builder(Color::Gold, "Ed4");
        tb.try_step(step("Ed4n")).unwrap();
        tb.try_step(step("Ed5s")).unwrap();
        assert_eq!(tb.can_finish(), Err(CommitError::NoChange));
        tb.try_step(step("Ed4e")).unwrap();
        tb.try_step(step("Ee4w")).unwrap();
        assert_eq!(tb.finish(), Err(CommitError::NoChange));
    }

    #[test]
    fn undo_restores_position_and_pending_state() {
        let mut tb = builder(Color::Gold, "Ed4 rd5");
        tb.try_step(step("rd5n")).unwrap();
        tb.try_step(step("Ed4n")).unwrap();
        tb.undo().unwrap();
        assert_eq!(tb.push_pending(), Some(sq("d5")));
        tb.undo().unwrap();
        assert_eq!(tb.position(), tb.start());
        assert!(tb.undo().is_none());
        assert_eq!(kinds(&tb), vec![]);
    }

    #[test]
    fn legal_steps_lists_pushes_and_pulls() {
        let mut tb = builder(Color::Gold, "Ed4 rd5");
        let from_d5: Vec<_> = tb.legal_steps_from(sq("d5")).into_iter().map(|(s, _)| s.to_string()).collect();
        assert_eq!(from_d5.len(), 3); // n, e, w pushes (d4 occupied)
        tb.try_step(step("Ed4w")).unwrap();
        let pulls: Vec<_> = tb
            .legal_steps()
            .into_iter()
            .filter(|(_, k)| *k == StepKind::PullFinish)
            .map(|(s, _)| s.to_string())
            .collect();
        assert_eq!(pulls, vec!["rd5s".to_string()]);
    }

    // --- routes ---

    fn route_squares(r: &Route) -> String {
        r.squares().iter().map(|s| s.to_string()).collect::<Vec<_>>().join(" ")
    }

    #[test]
    fn shortest_routes_are_all_minimal_walks() {
        let tb = builder(Color::Gold, "Ed4");
        let mut routes: Vec<_> = tb.shortest_routes(sq("d4"), sq("e6")).iter().map(route_squares).collect();
        routes.sort();
        assert_eq!(routes, vec!["d5 d6 e6", "d5 e5 e6", "e4 e5 e6"]);
    }

    #[test]
    fn routes_avoid_losing_the_piece_on_a_trap() {
        // Straight through c3 the lone rabbit would be captured.
        let tb = builder(Color::Gold, "Rc2");
        let routes = tb.shortest_routes(sq("c2"), sq("c4"));
        assert!(!routes.is_empty());
        assert!(routes.iter().all(|r| r.steps.len() == 4 && !r.squares().contains(&sq("c3"))));
    }

    #[test]
    fn routes_stop_where_the_piece_freezes() {
        // The dog freezes on d6 next to the horse, so it can't walk on to d7.
        let tb = builder(Color::Gold, "Dd4 he6");
        assert_eq!(tb.shortest_routes(sq("d4"), sq("d6")).len(), 1);
        assert!(tb.shortest_routes(sq("d4"), sq("d7")).is_empty());
    }

    #[test]
    fn routes_use_only_the_steps_left() {
        let mut tb = builder(Color::Gold, "Ed4 Ra1");
        tb.try_step(step("Ra1n")).unwrap();
        tb.try_step(step("Ra2n")).unwrap();
        assert_eq!(tb.shortest_routes(sq("d4"), sq("d6")).len(), 1);
        assert!(tb.shortest_routes(sq("d4"), sq("d7")).is_empty());
    }

    #[test]
    fn routes_count_friends_left_on_traps() {
        let tb = builder(Color::Gold, "Dd3 Cc3");
        let routes = tb.shortest_routes(sq("d3"), sq("e4"));
        assert_eq!(routes.len(), 2);
        assert!(routes.iter().all(|r| r.captures == 1));
    }

    #[test]
    fn no_routes_for_enemy_pieces_or_during_a_push() {
        let tb = builder(Color::Gold, "Ed4 rd5");
        assert!(tb.shortest_routes(sq("d5"), sq("d7")).is_empty());
        let mut tb = tb;
        tb.try_step(step("rd5n")).unwrap();
        assert!(tb.shortest_routes(sq("d4"), sq("c5")).is_empty());
    }

    #[test]
    fn reachable_gives_fewest_steps() {
        let tb = builder(Color::Gold, "Ed4");
        let reach = tb.reachable(sq("d4"));
        let dist = |s: &str| reach.iter().find(|(q, _)| *q == sq(s)).map(|(_, d)| *d);
        assert_eq!(dist("d5"), Some(1));
        assert_eq!(dist("e6"), Some(3));
        assert_eq!(dist("h4"), Some(4));
        assert_eq!(dist("d4"), None);
        assert_eq!(dist("c3"), None); // unguarded trap
        assert_eq!(dist("h5"), None); // 5 steps away
    }

    #[test]
    fn best_route_follows_the_most_dragged_squares() {
        let tb = builder(Color::Gold, "Ed4");
        let routes = || tb.shortest_routes(sq("d4"), sq("e6"));
        let along = |path: &str| {
            let path: Vec<Square> = path.split_whitespace().map(sq).collect();
            route_squares(&Route::best_along(routes(), &path).unwrap())
        };
        assert_eq!(along("e4 e5 e6"), "e4 e5 e6");
        assert_eq!(along("d5 e5 e6"), "d5 e5 e6");
        // A longer dragged path: the shortest route sharing the most squares.
        assert_eq!(along("c4 c5 d5 d6 e6"), "d5 d6 e6");
        assert_eq!(along("e4 f4 f5 e5 e6"), "e4 e5 e6");
    }

    #[test]
    fn best_route_prefers_fewer_losses_on_ties() {
        let walk =
            |steps: &[&str], captures| Route { steps: steps.iter().map(|s| step(s)).collect(), captures };
        let lossy = walk(&["Dd3n", "Dd4e"], 1);
        let safe = walk(&["Dd3e", "De3n"], 0);
        assert_eq!(Route::best_along(vec![lossy.clone(), safe.clone()], &[]), Some(safe));
        assert_eq!(Route::best_along(vec![lossy.clone()], &[]), Some(lossy));
        assert_eq!(Route::best_along(vec![], &[]), None);
    }
}
