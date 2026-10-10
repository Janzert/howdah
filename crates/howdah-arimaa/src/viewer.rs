//! arimaa.com's viewer variables: the `key=value` files its old Flash
//! client (`ArimaaClient.swf?vf=...`) loaded, as its puzzle pages still
//! serve them (`puzzles/show.cgi?v=p4` for a puzzle, `?w=p4` for its
//! answer). Read-only: Howdah writes ordinary records.
//!
//! ```text
//! mode=selfPlay
//! &movelist=1w Dc7 Rd7%0d1b rd8 ee7 dd6%0d2w
//! &chat=Get the rabbit to goal.
//!
//! &title=Arimaa Puzzle p4
//! &side=w
//! &startmove=2w
//! &wplayer=Gold
//! &bplayer=Silver
//! &timecontrol=0/0/0/0/0
//! ```
//!
//! Each variable starts a line with `&` (the first needs none); values are
//! percent-encoded, and `movelist` holds one move per line (`%0d`, a
//! carriage return, between them) with `w`/`b` labels. A puzzle puts its
//! position in the two "setups", which may place any pieces of either
//! color anywhere (or all of them in `1w`, then `1b pass`), and has silver
//! to move with `2w pass`. A puzzle can also be a whole game
//! that ends at the puzzle. `side` is the side the solver plays,
//! `startmove` the move the puzzle starts at (the answer file goes on with
//! the solution, both sides' moves), and `chat` the puzzle's question.

use crate::error::{GameError, ParseError, RecordError};
use crate::notation::{self, MoveBody};
use crate::position::Position;
use crate::record::GameRecord;
use crate::setup::validate_setup;
use crate::start::StartPosition;
use crate::tree::{GameTree, NodeId};
use crate::types::Color;

/// A game or puzzle read from viewer variables.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ViewerGame {
    /// The moves, from a set position when the setups don't make a legal
    /// start; `Event` from `title`, `Gold`/`Silver` from the players (but
    /// not the placeholders "Gold" and "Silver"), `TimeControl` unless it's
    /// `0/0/0/0/0` (no limit), and `chat` as the game comment.
    pub record: GameRecord,
    /// Where the game should be shown from: the `startmove` (a puzzle's
    /// position, before its solution), or else the end of the moves.
    pub start: NodeId,
    /// The side a puzzle's solver plays (`side`).
    pub solver: Option<Color>,
    /// Every variable, decoded, in file order.
    pub vars: Vec<(String, String)>,
}

/// Whether `text` looks like viewer variables rather than a record.
pub fn is_viewer_vars(text: &str) -> bool {
    text.lines().any(|l| l.trim_start().trim_start_matches('&').starts_with("movelist="))
}

/// Splits viewer variables into decoded `(name, value)` pairs. A line
/// starting with `&` (or the first line) starts a variable; other lines
/// continue the value before.
pub fn parse_viewer_vars(text: &str) -> Vec<(String, String)> {
    let mut raw: Vec<(String, String)> = Vec::new();
    for (i, line) in text.lines().enumerate() {
        let line = line.trim_end_matches('\r');
        let starts = i == 0 || line.starts_with('&');
        match line.trim_start_matches('&').split_once('=') {
            Some((name, value)) if starts && !name.is_empty() && !name.contains(' ') => {
                raw.push((name.to_string(), value.to_string()));
            }
            _ => {
                if let Some(last) = raw.last_mut() {
                    last.1.push('\n');
                    last.1.push_str(line);
                }
            }
        }
    }
    raw.into_iter().map(|(n, v)| (n, percent_decode(&v).trim().to_string())).collect()
}

/// Decodes `%XX` escapes (as UTF-8, or Latin-1 when that fails); anything
/// else stays as it is.
fn percent_decode(s: &str) -> String {
    let bytes = s.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        let hex = |b: u8| (b as char).to_digit(16);
        if bytes[i] == b'%'
            && i + 2 < bytes.len()
            && let (Some(h), Some(l)) = (hex(bytes[i + 1]), hex(bytes[i + 2]))
        {
            out.push((h * 16 + l) as u8);
            i += 3;
        } else {
            out.push(bytes[i]);
            i += 1;
        }
    }
    String::from_utf8(out.clone()).unwrap_or_else(|_| out.iter().map(|&b| b as char).collect())
}

fn error(message: impl Into<String>) -> RecordError {
    RecordError { line: 0, error: ParseError::new(message).into() }
}

impl ViewerGame {
    pub fn parse(text: &str) -> Result<ViewerGame, RecordError> {
        let vars = parse_viewer_vars(text);
        let var = |name: &str| vars.iter().find(|(n, _)| n == name).map(|(_, v)| v.as_str());
        let movelist = var("movelist").ok_or_else(|| error("no movelist in the viewer variables"))?;
        let mut lines: Vec<&str> =
            movelist.split(['\r', '\n']).map(str::trim).filter(|l| !l.is_empty()).collect();

        // The setups: a legal pair starts an ordinary game; anything else
        // is a puzzle's position. A puzzle may put every piece in one of
        // them and pass in the other (`1b pass`).
        let setup_line = |l: &str| -> Option<(Color, Vec<_>)> {
            let (label, rest) = l.split_once(char::is_whitespace).unwrap_or((l, ""));
            let (number, color) = notation::parse_move_number(label).ok()?;
            if number != 1 {
                return None;
            }
            if rest.trim().eq_ignore_ascii_case("pass") {
                return Some((color, Vec::new()));
            }
            match notation::parse_move_body(rest).ok()?.0 {
                MoveBody::Setup(p) => Some((color, p)),
                MoveBody::Empty => Some((color, Vec::new())),
                MoveBody::Steps(_) => None,
            }
        };
        let setups: Vec<(Color, Vec<_>)> = lines.iter().take(2).map_while(|l| setup_line(l)).collect();
        let legal = setups.len() == 2
            && setups[0].0 == Color::Gold
            && setups[1].0 == Color::Silver
            && setups.iter().all(|(c, p)| validate_setup(*c, p).is_ok());
        let mut text = String::new();
        if !legal && !setups.is_empty() {
            let pieces = setups.iter().flat_map(|(_, p)| p.iter().map(|p| (p.piece, p.square)));
            let mut position = Position::from_pieces(Color::Gold, pieces)
                .map_err(|e| RecordError { line: 1, error: e.into() })?;
            lines.drain(..setups.len());
            // `2w pass`: gold passes, so silver moves first.
            if let Some(first) = lines.first()
                && let Some((label, rest)) = first.split_once(char::is_whitespace)
                && rest.trim().eq_ignore_ascii_case("pass")
            {
                let (number, color) = notation::parse_move_number(label)
                    .map_err(|e| RecordError { line: setups.len() + 1, error: e.into() })?;
                if (number, color) != (2, Color::Gold) {
                    return Err(error(format!("unexpected pass in {first:?}")));
                }
                position.set_side_to_move(Color::Silver);
                lines.remove(0);
            }
            let start = StartPosition::new(position);
            text.push_str(&format!("[Position \"{}\"]\n\n", start.to_short_string()));
        }
        for l in &lines {
            text.push_str(l);
            text.push('\n');
        }
        let mut record = GameRecord::parse(&text)?;

        if let Some(title) = var("title").filter(|t| !t.is_empty()) {
            record.set_tag("Event", title);
        }
        for (name, tag, placeholder) in [("wplayer", "Gold", "Gold"), ("bplayer", "Silver", "Silver")] {
            if let Some(p) = var(name).filter(|p| !p.is_empty() && *p != placeholder) {
                record.set_tag(tag, p);
            }
        }
        if let Some(tc) = var("timecontrol").filter(|t| !t.is_empty() && *t != "0/0/0/0/0") {
            record.set_tag("TimeControl", tc);
        }
        if let Some(chat) = var("chat").filter(|c| !c.is_empty()) {
            record.tree.annotation_mut(GameTree::ROOT).expect("root").comment = Some(chat.to_string());
        }

        let main = record.tree.main_line();
        let start = match var("startmove").filter(|s| !s.is_empty()) {
            Some(label) => {
                let (number, color) = notation::parse_move_number(label)
                    .map_err(|e| RecordError { line: 0, error: e.into() })?;
                let wanted = notation::move_label(2 * (number as usize - 1) + color.index());
                *main
                    .iter()
                    .find(|&&n| record.tree.label_after(n) == wanted)
                    .ok_or(RecordError { line: 0, error: GameError::NoSuchNode })?
            }
            None => *main.last().expect("the main line has the root"),
        };
        let solver = var("side").and_then(|s| s.chars().next()).and_then(Color::from_letter);
        Ok(ViewerGame { record, start, solver, vars })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A made-up puzzle in the puzzle pages' layout: gold's rabbit b6 and
    /// dog b2, silver's rabbit h3 and elephant a3, silver to move (and
    /// goal).
    const SILVER_PUZZLE: &str = "mode=selfPlay\n\
        &movelist=1w Rb6 Db2%0d1b rh3 ea3%0d2w pass%0d2b rh3s rh2s%0d3w \n\
        &chat=Silver to move & stop the goal.\n\n\
        &title=Made-up puzzle\n\
        &side=b\n\n\
        &startmove=2b\n\
        &wplayer=Gold\n\
        &bplayer=Silver\n\
        &timecontrol=0/0/0/0/0\n";

    #[test]
    fn reads_variables() {
        let vars = parse_viewer_vars(SILVER_PUZZLE);
        let names: Vec<&str> = vars.iter().map(|(n, _)| n.as_str()).collect();
        assert_eq!(
            names,
            ["mode", "movelist", "chat", "title", "side", "startmove", "wplayer", "bplayer", "timecontrol"]
        );
        assert_eq!(vars[1].1, "1w Rb6 Db2\r1b rh3 ea3\r2w pass\r2b rh3s rh2s\r3w");
        assert_eq!(vars[2].1, "Silver to move & stop the goal.", "an & inside a line stays");
        assert!(is_viewer_vars(SILVER_PUZZLE));
        assert!(!is_viewer_vars("1g Ra1\n"));
    }

    #[test]
    fn a_puzzle_position_with_silver_to_move() {
        let g = ViewerGame::parse(SILVER_PUZZLE).unwrap();
        let tree = &g.record.tree;
        let start = tree.start_position().expect("a set position");
        assert_eq!(start.side_to_move(), Color::Silver);
        assert_eq!(start.pieces().count(), 4);
        assert_eq!(g.start, GameTree::ROOT, "the puzzle starts at 2b, the root");
        assert_eq!(g.solver, Some(Color::Silver));
        let main = tree.main_line();
        assert_eq!(tree.label_of(main[1]).as_deref(), Some("2s"));
        assert_eq!(g.record.tag("Event"), Some("Made-up puzzle"));
        assert_eq!(g.record.tag("Gold"), None, "a placeholder name");
        assert_eq!(g.record.tag("TimeControl"), None, "no limit");
        assert_eq!(
            tree[GameTree::ROOT].annotation().comment.as_deref(),
            Some("Silver to move & stop the goal.")
        );
        // It writes as an ordinary record that reads back the same.
        let written = g.record.to_record();
        assert!(written.contains("[Position \"s ["), "{written}");
        assert_eq!(GameRecord::parse(&written).unwrap().tree, g.record.tree);
    }

    #[test]
    fn a_whole_game_ending_at_the_puzzle() {
        let setups = "1w Ra1 Rb1 Rc1 Rd1 Re1 Rf1 Rg1 Rh1 Ha2 Db2 Cc2 Md2 Ee2 Cf2 Dg2 Hh2%0d\
                      1b ra8 rb8 rc8 rd8 re8 rf8 rg8 rh8 ha7 db7 cc7 ed7 me7 cf7 dg7 hh7";
        let text = format!(
            "mode=selfPlay\n&movelist={setups}%0d2w Ee2n%0d2b ed7s%0d3w Ee3n%0d3b \n&side=b\n\
             &wplayer=alice\n&bplayer=bob\n&timecontrol=30s/5m\n"
        );
        let g = ViewerGame::parse(&text).unwrap();
        assert!(g.record.tree.start_position().is_none(), "legal setups: an ordinary game");
        assert_eq!(g.record.tree.label_after(g.start), "3s", "no startmove: the end");
        assert_eq!((g.record.tag("Gold"), g.record.tag("Silver")), (Some("alice"), Some("bob")));
        assert_eq!(g.record.tag("TimeControl"), Some("30s/5m"));

        // An answer file's startmove.
        let answer = format!("{text}&startmove=3w\n");
        let g = ViewerGame::parse(&answer).unwrap();
        assert_eq!(g.record.tree.label_after(g.start), "3g");
    }

    #[test]
    fn every_piece_in_one_setup_and_a_pass_in_the_other() {
        let g = ViewerGame::parse("&movelist=1w Ra2 rh7 Ee1 ee8%0d1b pass%0d2w Ra2n%0d2b \n&startmove=2w\n")
            .unwrap();
        let start = g.record.tree.start_position().unwrap();
        assert_eq!((start.side_to_move(), start.pieces().count()), (Color::Gold, 4));
        assert_eq!(g.record.tree.main_line().len(), 2, "the root, then the solution's move");
        let silver =
            ViewerGame::parse("&movelist=1w Ra2 rh7 Ee1 ee8%0d1b pass%0d2w pass%0d2b rh7s\n").unwrap();
        assert_eq!(silver.record.tree.start_position().unwrap().side_to_move(), Color::Silver);
    }

    #[test]
    fn gold_to_move_and_errors() {
        let g = ViewerGame::parse("&movelist=1w Ra2%0d1b rh7 ee5%0d2w \n&side=w\n").unwrap();
        let start = g.record.tree.start_position().unwrap();
        assert_eq!((start.side_to_move(), g.start), (Color::Gold, GameTree::ROOT));
        assert!(ViewerGame::parse("&title=no moves\n").is_err());
        assert!(ViewerGame::parse("&movelist=1w Ra2 Rb2%0d1b ra2\n").is_err(), "a square used twice");
        assert!(ViewerGame::parse("&movelist=1w Ra2%0d1b rh7%0d2b pass\n").is_err());
        assert!(ViewerGame::parse("&movelist=1w Ra2%0d1b rh7%0d2w \n&startmove=5w\n").is_err());
    }
}
