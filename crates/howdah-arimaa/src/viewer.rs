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
use crate::notation;
use crate::position::Position;
use crate::record::GameRecord;
use crate::setup::{Placement, validate_setup};
use crate::start::StartPosition;
use crate::tree::{GameTree, NodeId};
use crate::types::{Color, Piece, Square};

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
        let (text, labels) = normalize(movelist)?;
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
                // The label the move list's own label stood for.
                let label = labels.iter().find(|(outer, _)| outer == label).map_or(label, |(_, inner)| inner);
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

/// A move list line: its label (`2w`) and the rest.
struct Line {
    label: String,
    number: u32,
    color: Color,
    body: String,
}

fn split_line(l: &str) -> Option<Line> {
    let (label, rest) = l.split_once(char::is_whitespace).unwrap_or((l, ""));
    let (number, color) = notation::parse_move_number(label).ok()?;
    Some(Line { label: label.to_string(), number, color, body: rest.trim().to_string() })
}

/// Turns a viewer's move list into an ordinary record, as leniently as
/// arimaa.com's viewer read it. Returns the record and, for each label the
/// list wrote, the label it stood for.
///
/// - A line may carry a second label after its own (`1w 1g Ha2 ...`); the
///   second one is the move's.
/// - The leading move-1 lines are the setups: a legal pair is an ordinary
///   game's; otherwise they place a puzzle's position, any pieces of
///   either color anywhere (tokens without a piece letter are skipped),
///   and either may be `pass`. `2w pass` after them means silver to move.
/// - Lines with nothing after the label are dropped (the list ends with
///   the next move's label), and so are passes later on (not a move in
///   Arimaa).
/// - Steps name pushed and pulled pieces in the mover's case at times
///   (`Mg1e` for a silver camel on g1): each step takes the piece on its
///   square, and capture tokens are left for the reader to work out.
fn normalize(movelist: &str) -> Result<(String, Vec<(String, String)>), RecordError> {
    let mut labels = Vec::new();
    let mut lines: Vec<Line> = Vec::new();
    for (i, raw) in movelist.split(['\r', '\n']).map(str::trim).filter(|l| !l.is_empty()).enumerate() {
        let mut line = split_line(raw).ok_or_else(|| RecordError {
            line: i + 1,
            error: ParseError::new(format!("can't read the move {raw:?}")).into(),
        })?;
        if let Some(inner) = split_line(&line.body)
            && line
                .body
                .split_whitespace()
                .next()
                .is_some_and(|t| t.starts_with(|c: char| c.is_ascii_digit()))
        {
            labels.push((line.label.clone(), inner.label.clone()));
            line = inner;
        }
        lines.push(line);
    }

    // The setups.
    let placements = |body: &str| -> Option<Vec<(Piece, Square)>> {
        if body.eq_ignore_ascii_case("pass") {
            return Some(Vec::new());
        }
        let mut out = Vec::new();
        for tok in body.split_whitespace() {
            let piece = tok.chars().next().and_then(Piece::from_letter);
            let square = tok.get(1..).and_then(|sq| sq.parse::<Square>().ok());
            match (piece, square) {
                (Some(p), Some(sq)) => out.push((p, sq)),
                // A step: not a setup line.
                (Some(_), None) if tok.len() == 4 => return None,
                _ => {} // unreadable (`a1`): skipped
            }
        }
        Some(out)
    };
    let setups: Vec<(Color, Vec<(Piece, Square)>)> = lines
        .iter()
        .take(2)
        .map_while(|l| (l.number == 1).then(|| placements(&l.body).map(|p| (l.color, p))).flatten())
        .collect();
    let legal = setups.len() == 2
        && setups[0].0 == Color::Gold
        && setups[1].0 == Color::Silver
        && setups.iter().all(|(c, p)| {
            let placed: Vec<Placement> =
                p.iter().map(|&(piece, square)| Placement { piece, square }).collect();
            validate_setup(*c, &placed).is_ok()
        });
    let mut position = Position::empty(Color::Gold);
    for (piece, sq) in setups.iter().flat_map(|(_, p)| p) {
        if position.piece_at(*sq).is_some() {
            return Err(RecordError {
                line: 1,
                error: ParseError::new(format!("{sq} is occupied twice")).into(),
            });
        }
        position.set(*sq, Some(*piece));
    }
    let mut text = String::new();
    let rest: &[Line];
    if !legal && !setups.is_empty() {
        let mut after = &lines[setups.len()..];
        if let Some(first) = after.first()
            && first.body.eq_ignore_ascii_case("pass")
        {
            if (first.number, first.color) != (2, Color::Gold) {
                return Err(error(format!("unexpected pass in {} {}", first.label, first.body)));
            }
            position.set_side_to_move(Color::Silver);
            after = &after[1..];
        }
        rest = after;
        let start = StartPosition::new(position.clone());
        text.push_str(&format!("[Position \"{}\"]\n\n", start.to_short_string()));
    } else {
        // An ordinary game: its setups as written.
        for l in &lines[..setups.len().min(2)] {
            text.push_str(&format!("{} {}\n", l.label, l.body));
        }
        rest = &lines[setups.len().min(2)..];
    }

    for l in rest {
        if l.body.is_empty() || l.body.eq_ignore_ascii_case("pass") {
            continue;
        }
        let mut tokens = Vec::new();
        for tok in l.body.split_whitespace() {
            let mut chars = tok.chars();
            let (Some(letter), Some(square), Some(dir)) = (
                chars.next(),
                tok.get(1..3).and_then(|s| s.parse::<Square>().ok()),
                tok.get(3..).filter(|d| d.len() == 1).and_then(|d| d.chars().next()),
            ) else {
                tokens.push(tok.to_string());
                continue;
            };
            if dir == 'x' {
                continue; // the reader works the captures out
            }
            let mut letter = letter;
            if let (Some(on), Some(Some(named))) =
                (position.piece_at(square), Some(Piece::from_letter(letter)))
                && on.kind == named.kind
            {
                letter = on.letter();
            }
            if let (Some(piece), Some(d)) = (Piece::from_letter(letter), crate::types::Dir::from_letter(dir))
            {
                let _ = position.apply_step(crate::step::Step::new(piece, square, d));
            }
            tokens.push(format!("{letter}{square}{dir}"));
        }
        text.push_str(&format!("{} {}\n", l.label, tokens.join(" ")));
    }
    Ok((text, labels))
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

    /// The moves after the start, as `label notation`.
    fn solution(g: &ViewerGame) -> Vec<String> {
        let t = &g.record.tree;
        let line = t.line_through(g.start);
        let at = line.iter().position(|&n| n == g.start).unwrap();
        line[at + 1..]
            .iter()
            .map(|&n| format!("{} {}", t.label_of(n).unwrap(), t[n].mv().unwrap().notation()))
            .collect()
    }

    #[test]
    fn quirks_of_arimaa_coms_move_lists() {
        // A placement without a piece letter is skipped.
        let g = ViewerGame::parse("&movelist=1w Ra2 a1 Ee1%0d1b rh7 ee8%0d2w Ra2n%0d2b \n").unwrap();
        assert_eq!(g.record.tree.start_position().unwrap().pieces().count(), 4);

        // Doubled labels, a stray empty line and pass, and a startmove in
        // the outer labels.
        let text = "&movelist=1w 1g Ra2 Ee1%0d1b 1s rh7 ee8%0d2w 2g Ra2n%0d2b 2s rh7s%0d\
                    3w 3g%0d3b pass%0d4w 3g Ra3n%0d4b \n&startmove=4w\n";
        let g = ViewerGame::parse(text).unwrap();
        assert_eq!(g.record.tree.label_after(g.start), "3g");
        assert_eq!(solution(&g), ["3g Ra3n"]);

        // A pushed piece written in the mover's case.
        let g = ViewerGame::parse("&movelist=1w Ee4 Ra2%0d1b re5 rh7%0d2w Re5n Ee4n%0d2b \n&startmove=2w\n")
            .unwrap();
        assert_eq!(solution(&g), ["2g re5n Ee4n"]);
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
