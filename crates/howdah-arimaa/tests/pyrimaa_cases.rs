//! Cases ported from AEI's `pyrimaa/tests/test_board.py`. Squares are
//! indices (a1 = 0, h8 = 63), which match pyrimaa's step tuples.

use howdah_arimaa::notation::format_steps;
use howdah_arimaa::{Color, CommitError, Position, Square, StepError, TurnBuilder};

const BASIC_SETUP: &str = "[rrrrrrrrdhcemchd                                DHCMECHDRRRRRRRR]";
const CHECK_STEP_POS: &str = "[rrrrrr rdhce h d C   c      mCr     ED         RDH M  H RRRRRRR ]";
const CHECK_TRAP_STEP: &str = "[rrrrrc rdhce hMd          C mCr     ED   H     RD     H RRRRRRR ]";

fn gold(short: &str) -> TurnBuilder {
    TurnBuilder::new(&Position::from_short_string(Color::Gold, short).unwrap())
}

fn sq(i: u8) -> Square {
    Square::new(i).unwrap()
}

fn mv(tb: &mut TurnBuilder, from: u8, to: u8) -> Result<(), StepError> {
    tb.try_move(sq(from), sq(to)).map(|_| ())
}

/// All legal next steps as sorted (from, to) index pairs.
fn step_tuples(tb: &TurnBuilder) -> Vec<(u8, u8)> {
    let mut v: Vec<_> =
        tb.legal_steps().into_iter().map(|(s, _)| (s.from.index(), s.to().unwrap().index())).collect();
    v.sort();
    v
}

#[test]
fn check_step() {
    let tb = gold(CHECK_STEP_POS);
    let err = |from, to| {
        let mut t = tb.clone();
        mv(&mut t, from, to).unwrap_err()
    };
    assert_eq!(err(16, 24), StepError::NoPiece(sq(16)));
    assert_eq!(err(0, 8), StepError::Occupied(sq(8)));
    assert_eq!(err(8, 17), StepError::NotAdjacent { from: sq(8), to: sq(17) });
    assert_eq!(err(41, 40), StepError::Frozen(sq(41)));
    assert_eq!(err(23, 15), StepError::RabbitBackward);
    // No stronger piece next to the silver cat on f6 (the gold cat is equal).
    assert_eq!(err(45, 46), StepError::CannotMoveEnemy(sq(45)));

    // The elephant on e4 pushes the camel from e5 to d5.
    let mut push = tb.clone();
    mv(&mut push, 36, 35).unwrap();
    let in_push = |from, to| {
        let mut t = push.clone();
        mv(&mut t, from, to)
    };
    assert_eq!(in_push(37, 36), Err(StepError::PushIncomplete(sq(36))), "cat is too weak to finish");
    assert_eq!(in_push(8, 16), Err(StepError::PushIncomplete(sq(36))), "can't skip finishing");
    assert_eq!(in_push(38, 30), Err(StepError::PushIncomplete(sq(36))), "can't start another push");
    assert_eq!(in_push(28, 36), Ok(()));

    // A push can't start on the last step.
    let mut last = tb.clone();
    for (from, to) in [(8, 16), (16, 24), (24, 32)] {
        mv(&mut last, from, to).unwrap();
    }
    assert_eq!(mv(&mut last, 36, 35), Err(StepError::NoStepsForPush));
}

#[test]
fn generate_steps() {
    let tb = gold(CHECK_TRAP_STEP);
    let expected_trap = vec![
        (1, 9),
        (2, 10),
        (3, 11),
        (4, 12),
        (5, 13),
        (6, 7),
        (8, 9),
        (8, 16),
        (14, 13),
        (14, 15),
        (14, 22),
        (17, 9),
        (17, 16),
        (17, 18),
        (17, 25),
        (23, 22),
        (23, 31),
        (28, 20),
        (28, 27),
        (29, 21),
        (29, 30),
        (34, 26),
        (34, 33),
        (34, 35),
        (34, 42),
        (36, 35),
        (36, 44),
        (37, 45),
        (38, 30),
        (38, 39),
        (38, 46),
        (53, 45),
        (53, 52),
        (54, 46),
        (54, 62),
        (55, 47),
    ];
    assert_eq!(step_tuples(&tb), expected_trap);

    // Pushing the silver horse onto the f6 trap: only the camel can finish.
    let mut in_push = tb.clone();
    mv(&mut in_push, 53, 45).unwrap();
    assert_eq!(step_tuples(&in_push), vec![(54, 53)]);

    // The camel steps g7 -> g6; the horse on f7 and dog on h7 can now be pulled into g7.
    let mut pull = tb.clone();
    mv(&mut pull, 54, 46).unwrap();
    let expected_pull = vec![
        (1, 9),
        (2, 10),
        (3, 11),
        (4, 12),
        (5, 13),
        (6, 7),
        (8, 9),
        (8, 16),
        (14, 13),
        (14, 15),
        (14, 22),
        (17, 9),
        (17, 16),
        (17, 18),
        (17, 25),
        (23, 22),
        (23, 31),
        (28, 20),
        (28, 27),
        (29, 21),
        (29, 30),
        (34, 26),
        (34, 33),
        (34, 35),
        (34, 42),
        (36, 35),
        (36, 44),
        (37, 45),
        (38, 30),
        (38, 39),
        (46, 45),
        (46, 47),
        (46, 54),
        (53, 54),
        (55, 54),
    ];
    assert_eq!(step_tuples(&pull), expected_pull);
}

fn notation_after(steps: &[(u8, u8)]) -> String {
    let mut tb = gold(BASIC_SETUP);
    for &(from, to) in steps {
        mv(&mut tb, from, to).unwrap();
    }
    let effects: Vec<_> = tb.steps().iter().map(|s| s.effect).collect();
    format_steps(&effects)
}

#[test]
fn capture_notation() {
    // Cat steps onto an unguarded trap.
    assert_eq!(notation_after(&[(10, 18), (12, 20)]), "Cc2n Cc3x Ee2n");
    // Cat on a trap is captured when its guard (the camel) steps away.
    assert_eq!(notation_after(&[(11, 19), (10, 18), (19, 27)]), "Md2n Cc2n Md3n Cc3x");
    // The horse guarding c3 steps back to b2, leaving the cat alone.
    assert_eq!(notation_after(&[(8, 16), (9, 17), (10, 18), (17, 9)]), "Da2n Hb2n Cc2n Hb3s Cc3x");
}

#[test]
fn net_null_turn() {
    // pyrimaa's do_move accepts this sequence (it doesn't check for null
    // moves; get_moves does). A turn must change the position, so it can't
    // be committed.
    let mut tb = gold(BASIC_SETUP);
    for (from, to) in [(9, 17), (17, 18), (18, 17), (17, 9)] {
        mv(&mut tb, from, to).unwrap();
    }
    assert_eq!(tb.can_finish(), Err(CommitError::NoChange));
}

#[test]
fn five_steps() {
    let mut tb = gold(BASIC_SETUP);
    for (from, to) in [(9, 17), (17, 25), (25, 33), (33, 34)] {
        mv(&mut tb, from, to).unwrap();
    }
    assert_eq!(mv(&mut tb, 34, 35), Err(StepError::TurnFull));
}
