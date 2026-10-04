//! End-to-end check of a short hand-built game containing a push, a pull and
//! a trap capture. The record was also replayed through the reference Python
//! implementation (AEI's pyrimaa) with identical notation.

use howdah_arimaa::{Color, Game, Move, Piece, Position, StepKind, TurnBuilder};

const RECORD: &str = include_str!("data/sample_game.txt");

#[test]
fn sample_game_replays_and_roundtrips() {
    let game = Game::parse(RECORD).expect("sample game is legal");
    assert_eq!(game.ply_count(), 10);
    assert_eq!(game.result(), None);
    assert_eq!(game.to_record(), RECORD);
}

#[test]
fn sample_game_contains_push_pull_and_capture() {
    let game = Game::parse(RECORD).unwrap();
    let mut kinds = Vec::new();
    let mut captures = Vec::new();
    for ply in 2..game.ply_count() {
        let Move::Steps(effects) = &game.moves()[ply] else { panic!("ply {ply} is a setup") };
        // Replay through a TurnBuilder to recover each step's role.
        let mut tb = TurnBuilder::new(game.position_at(ply).unwrap());
        for e in effects {
            kinds.push(tb.try_step(e.step).unwrap().kind);
            captures.extend(e.capture);
        }
    }
    assert!(kinds.contains(&StepKind::PushStart));
    assert!(kinds.contains(&StepKind::PushFinish));
    assert!(kinds.contains(&StepKind::PullFinish));
    assert_eq!(captures.len(), 1);
    assert_eq!(captures[0].to_string(), "hf3x");
}

#[test]
fn sample_game_final_position() {
    let game = Game::parse(RECORD).unwrap();
    // Ranks 8 down to 1, as in the pyrimaa board diagram.
    let rows =
        ["rrrrrrr ", "hdcm cd ", "        ", "    e E ", "      r ", "        ", "HDCM CDH", "RRRRRRRR"];
    let expected = Position::from_short_string(Color::Gold, &format!("[{}]", rows.concat())).unwrap();
    assert_eq!(game.current_position(), &expected);
    assert_eq!(game.current_position().count(Piece::from_letter('h').unwrap()), 1);
}
