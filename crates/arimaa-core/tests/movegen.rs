//! Cross-checks step legality against AEI's pyrimaa by counting every
//! distinct position reachable in one turn. The expected counts come from
//! `pyrimaa.board.Position.get_moves()` (positions from seeded random play).

use std::collections::HashSet;

use arimaa_core::{Color, Position, TurnBuilder};

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

#[test]
fn opening_position() {
    let pos = "[rrrrrrrrhdcmecdh                                HDCMECDHRRRRRRRR]";
    assert_eq!(count_moves(Color::Gold, pos), 3353);
}

#[test]
fn midgame_positions() {
    let cases = [
        ("[rrr r r hm c  r     r  d M r  h D  d            HRRRR RR RC R   ]", 12724),
        ("[rrr r   h c  r   R  r hdm M     D  drr   R RR R  H     RR C R   ]", 23405),
        ("[r       hR r r  m   r  hDRM    d   r rR    RR    RH  RR C       ]", 14166),
    ];
    for (pos, expected) in cases {
        assert_eq!(count_moves(Color::Gold, pos), expected, "{pos}");
    }
}
