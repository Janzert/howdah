//! Standard Arimaa move notation.
//!
//! - Setup token: piece + square, e.g. `Ra1`.
//! - Step token: piece + square + direction, e.g. `Ed2n`.
//! - Capture token: piece + square + `x`, e.g. `cf3x`, written after the step
//!   that caused it.
//! - Move line: move number + side letter, then tokens: `2g Ed2n Ed3n Cf3x`.
//!   The old `w`/`b` side letters are accepted when parsing.
//!
//! Parsing only checks syntax; [`crate::Game`] checks legality and that
//! capture tokens match what actually happened.

use crate::error::ParseError;
use crate::setup::Placement;
use crate::step::{Capture, Step, StepEffect};
use crate::types::{Color, Dir, Piece, Square};

/// Words that may end a move line to mark how the game ended. They're kept
/// as text and not interpreted.
pub const END_MARKERS: [&str; 6] = ["resigns", "resign", "lost", "lose", "timeout", "forfeit"];

/// A step as written in a record, with the capture token that followed it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct RecordStep {
    pub step: Step,
    pub capture: Option<Capture>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum MoveBody {
    Setup(Vec<Placement>),
    Steps(Vec<RecordStep>),
    /// A move number with nothing after it, as at the end of an unfinished record.
    Empty,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct MoveLine {
    pub number: u32,
    pub color: Color,
    pub body: MoveBody,
    /// A trailing end marker such as `resigns`.
    pub marker: Option<String>,
}

enum Token {
    Placement(Placement),
    Step(Step),
    Capture(Capture),
}

fn parse_token(tok: &str) -> Result<Token, ParseError> {
    let err = || ParseError::new(format!("invalid token {tok:?}"));
    let mut chars = tok.chars();
    let piece = chars.next().and_then(Piece::from_letter).ok_or_else(err)?;
    let square: Square = tok.get(1..3).ok_or_else(err)?.parse().map_err(|_| err())?;
    match tok.get(3..) {
        Some("") => Ok(Token::Placement(Placement { piece, square })),
        Some("x") => Ok(Token::Capture(Capture { piece, square })),
        Some(d) if d.len() == 1 => {
            let dir = Dir::from_letter(d.chars().next().unwrap()).ok_or_else(err)?;
            Ok(Token::Step(Step::new(piece, square, dir)))
        }
        _ => Err(err()),
    }
}

/// Parses the tokens of a move (without the move number).
pub fn parse_move_body(text: &str) -> Result<(MoveBody, Option<String>), ParseError> {
    let mut tokens: Vec<&str> = text.split_whitespace().collect();
    let marker = match tokens.last() {
        Some(t) if END_MARKERS.contains(&t.to_ascii_lowercase().as_str()) => {
            Some(tokens.pop().unwrap().to_string())
        }
        _ => None,
    };
    if tokens.is_empty() {
        return Ok((MoveBody::Empty, marker));
    }
    let mut placements = Vec::new();
    let mut steps: Vec<RecordStep> = Vec::new();
    for tok in tokens {
        match parse_token(tok)? {
            Token::Placement(p) => placements.push(p),
            Token::Step(step) => steps.push(RecordStep { step, capture: None }),
            Token::Capture(c) => match steps.last_mut() {
                Some(last) if last.capture.is_none() => last.capture = Some(c),
                Some(_) => return Err(ParseError::new(format!("unexpected second capture {tok}"))),
                None => return Err(ParseError::new(format!("capture {tok} doesn't follow a step"))),
            },
        }
        if !placements.is_empty() && !steps.is_empty() {
            return Err(ParseError::new("a move can't mix setup placements and steps"));
        }
    }
    let body = if placements.is_empty() { MoveBody::Steps(steps) } else { MoveBody::Setup(placements) };
    Ok((body, marker))
}

/// Parses a move number such as `12g` or `3b`.
pub fn parse_move_number(tok: &str) -> Result<(u32, Color), ParseError> {
    let err = || ParseError::new(format!("invalid move number {tok:?}"));
    let split = tok.find(|c: char| !c.is_ascii_digit()).ok_or_else(err)?;
    let (num, side) = tok.split_at(split);
    let number: u32 = num.parse().map_err(|_| err())?;
    let mut side_chars = side.chars();
    let color = side_chars.next().and_then(Color::from_letter).ok_or_else(err)?;
    if side_chars.next().is_some() || number == 0 {
        return Err(err());
    }
    Ok((number, color))
}

/// Parses a full move line such as `2g Ed2n Ed3n Cf3x`.
pub fn parse_move_line(line: &str) -> Result<MoveLine, ParseError> {
    let line = line.trim();
    let (head, rest) = line.split_once(char::is_whitespace).unwrap_or((line, ""));
    let (number, color) = parse_move_number(head)?;
    let (body, marker) = parse_move_body(rest)?;
    Ok(MoveLine { number, color, body, marker })
}

/// Move number label for a ply, e.g. ply 0 → `1g`, ply 3 → `2s`.
pub fn move_label(ply: usize) -> String {
    let color = if ply % 2 == 0 { Color::Gold } else { Color::Silver };
    format!("{}{}", ply / 2 + 1, color.letter())
}

pub fn format_placements(placements: &[Placement]) -> String {
    placements.iter().map(ToString::to_string).collect::<Vec<_>>().join(" ")
}

/// Formats applied steps with their capture tokens.
pub fn format_steps(effects: &[StepEffect]) -> String {
    effects.iter().map(ToString::to_string).collect::<Vec<_>>().join(" ")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_steps_and_captures() {
        let m = parse_move_line("2g Ed2n Ed3n Rc4s Rc3x").unwrap();
        assert_eq!((m.number, m.color), (2, Color::Gold));
        let MoveBody::Steps(steps) = m.body else { panic!() };
        assert_eq!(steps.len(), 3);
        assert_eq!(steps[0].step.to_string(), "Ed2n");
        assert_eq!(steps[2].capture.unwrap().to_string(), "Rc3x");
        assert_eq!(m.marker, None);
    }

    #[test]
    fn parse_setup_line_with_old_colors() {
        let m = parse_move_line("1b ra7 rb7").unwrap();
        assert_eq!(m.color, Color::Silver);
        let MoveBody::Setup(p) = m.body else { panic!() };
        assert_eq!(format_placements(&p), "ra7 rb7");
    }

    #[test]
    fn parse_markers_and_empty() {
        let m = parse_move_line("20s resigns").unwrap();
        assert_eq!(m.body, MoveBody::Empty);
        assert_eq!(m.marker.as_deref(), Some("resigns"));
        let m = parse_move_line("20s").unwrap();
        assert_eq!(m.body, MoveBody::Empty);
    }

    #[test]
    fn parse_errors() {
        for bad in [
            "g Ed2n",
            "0g Ed2n",
            "2x Ed2n",
            "2g Ed2q",
            "2g Xd2n",
            "2g Ei2n",
            "2g Ed2nn",
            "2g Rc3x",
            "2g Ed2n Ra1",
            "2g Rc4s Rc3x Rc3x",
        ] {
            assert!(parse_move_line(bad).is_err(), "{bad} should fail");
        }
    }

    #[test]
    fn labels() {
        assert_eq!(move_label(0), "1g");
        assert_eq!(move_label(1), "1s");
        assert_eq!(move_label(2), "2g");
        assert_eq!(move_label(5), "3s");
    }
}
