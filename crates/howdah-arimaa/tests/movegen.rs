//! Cross-checks step legality against AEI's pyrimaa by counting every
//! distinct position reachable in one turn. Expected counts come from
//! `pyrimaa.board.Position.get_moves()`. Positions are the opening, three
//! from seeded random play, and the hand-built positions in pyrimaa's
//! `tests/test_board.py`, each with both sides to move.

use std::collections::HashSet;

use howdah_arimaa::{Color, Position, TurnBuilder};

fn collect(tb: &mut TurnBuilder, out: &mut HashSet<Position>) {
    if tb.can_finish().is_ok() {
        out.insert(tb.clone().finish().unwrap().end);
    }
    for (step, _) in tb.legal_steps() {
        tb.try_step(step).unwrap();
        collect(tb, out);
        tb.undo();
    }
}

fn count_moves(side: Color, short: &str) -> usize {
    let pos = Position::from_short_string(side, short).unwrap();
    let mut out = HashSet::new();
    collect(&mut TurnBuilder::new(&pos), &mut out);
    out.len()
}

fn check(cases: &[(&str, &str, usize, usize)]) {
    for &(name, pos, gold, silver) in cases {
        assert_eq!(count_moves(Color::Gold, pos), gold, "{name}, gold to move");
        assert_eq!(count_moves(Color::Silver, pos), silver, "{name}, silver to move");
    }
}

#[test]
fn opening_position() {
    let pos = "[rrrrrrrrhdcmecdh                                HDCMECDHRRRRRRRR]";
    assert_eq!(count_moves(Color::Gold, pos), 3353);
}

#[test]
fn random_play_positions() {
    check(&[
        ("random 15", "[rrr r r hm c  r     r  d M r  h D  d            HRRRR RR RC R   ]", 12724, 46401),
        ("random 25", "[rrr r   h c  r   R  r hdm M     D  drr   R RR R  H     RR C R   ]", 23405, 29660),
        ("random 35", "[r       hR r r  m   r  hDRM    d   r rR    RR    RH  RR C       ]", 14166, 8476),
    ]);
}

#[test]
fn pyrimaa_test_positions() {
    check(&[
        ("CHECK_STEP_POS", "[rrrrrr rdhce h d C   c      mCr     ED         RDH M  H RRRRRRR ]", 15312, 5826),
        (
            "CHECK_TRAP_STEP",
            "[rrrrrc rdhce hMd          C mCr     ED   H     RD     H RRRRRRR ]",
            46370,
            3251,
        ),
        // Almost every step sequence for gold ends net-null.
        (
            "INDUCE_NULL_MOVE_POS",
            "[rrrr rrr  hreh   dcC d     cRm                                  ]",
            2,
            17329,
        ),
        (
            "DOUBLE_IMMOBILIZATION_POS",
            "[rEmH    RdH      C                            c      hDr    hMeR]",
            0,
            0,
        ),
        ("GOLD_GOAL_POS", "[rRrr rrr  d      D c        Rm                       d  R RRRR  ]", 1202, 12362),
    ]);
}
