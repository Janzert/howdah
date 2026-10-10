//! Game records: tags plus a game tree, in the record format described in
//! `docs/VARIATIONS.md`.
//!
//! ```text
//! [Gold "alice"]
//! [Silver "bob"]
//!
//! {Game comment.}
//! 1g Ra1 Rb1 ...
//! 1s ra8 rb8 ...
//! 2g Ed2n Ed3n Ed4n Ed5n ?! {Comment.}
//! (
//! 2g Ee2n Ee3n
//! )
//! 2s ed7s ed6s
//! ```
//!
//! The reader is lenient: it accepts PGN tag pairs and arimaa.com's
//! `Name: value` tags, `White`/`Black` tag names, `w`/`b` move labels,
//! several moves on a line, `#` comment lines and arimaa.com `takeback`
//! lines. The writer produces one canonical form.

use crate::error::{GameError, ParseError, RecordError};
use crate::game::build_turn;
use crate::notation::{self, MoveBody};
use crate::outcome::{GameResult, WinReason};
use crate::position::Position;
use crate::start::StartPosition;
use crate::tree::{GameTree, Glyph, NodeId};
use crate::types::Color;

/// A game with its tags (players, event, result, ...).
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct GameRecord {
    /// Tags in the order read or set. Names use Gold and Silver.
    pub tags: Vec<(String, String)>,
    pub tree: GameTree,
}

/// Results that can end the move text.
const RESULT_TOKENS: [&str; 4] = ["1-0", "0-1", "1/2-1/2", "*"];

/// Tags the writer computes from the tree, written last as AEI does.
const COMPUTED_TAGS: [&str; 3] = ["PlyCount", "ResultCode", "Result"];

/// Marks a multi-line value in arimaa.com's tag format.
const MULTILINE: &str = "-=+=-";

impl GameRecord {
    pub fn new() -> GameRecord {
        GameRecord::default()
    }

    pub fn tag(&self, name: &str) -> Option<&str> {
        self.tags.iter().find(|(n, _)| n == name).map(|(_, v)| v.as_str())
    }

    /// Sets a tag, replacing any value it had and keeping its place.
    pub fn set_tag(&mut self, name: impl Into<String>, value: impl Into<String>) {
        let (name, value) = (name.into(), value.into());
        match self.tags.iter_mut().find(|(n, _)| *n == name) {
            Some(slot) => slot.1 = value,
            None => self.tags.push((name, value)),
        }
    }

    /// Parses a record holding one game (or none: empty text gives an
    /// empty game).
    pub fn parse(text: &str) -> Result<GameRecord, RecordError> {
        let mut games = parse_games(text)?;
        if games.len() > 1 {
            let line = games[1].0;
            return Err(RecordError {
                line,
                error: ParseError::new("the record holds more than one game").into(),
            });
        }
        Ok(games.pop().map(|(_, g)| g).unwrap_or_default())
    }

    /// Parses a file of any number of games.
    pub fn parse_all(text: &str) -> Result<Vec<GameRecord>, RecordError> {
        Ok(parse_games(text)?.into_iter().map(|(_, g)| g).collect())
    }

    /// Writes the full record: tags, comments, glyphs and variations.
    pub fn to_record(&self) -> String {
        let tree = &self.tree;
        let end = tree.line_end(GameTree::ROOT);
        let mut tags: Vec<(String, String)> =
            self.tags.iter().filter(|(n, _)| !COMPUTED_TAGS.contains(&n.as_str())).cloned().collect();
        // PlyCount only goes with other tags, so an untagged record stays
        // free of a header.
        if let Some(p) = tree.start_position() {
            tags.retain(|(n, _)| n != "Position");
            tags.push(("Position".into(), position_tag(p)));
        }
        if tree[end].ply() > 0 && !tags.is_empty() {
            tags.push(("PlyCount".into(), tree[end].ply().to_string()));
        }
        let (code, result) = match tree[end].result() {
            Some(r) => (Some(r.reason.letter().to_string()), Some(result_token(r.winner).to_string())),
            None => (self.tag("ResultCode").map(str::to_string), self.tag("Result").map(str::to_string)),
        };
        tags.extend(code.map(|c| ("ResultCode".to_string(), c)));
        tags.extend(result.clone().map(|r| ("Result".to_string(), r)));

        let mut out = String::new();
        for (name, value) in &tags {
            out.push_str(&format!("[{name} \"{}\"]\n", escape_tag(value)));
        }
        if !tags.is_empty() {
            out.push('\n');
        }
        if let Some(c) = &tree[GameTree::ROOT].annotation().comment {
            out.push_str(&format!("{{{}}}\n", escape_comment(c)));
        }
        self.write_from(&mut out, GameTree::ROOT, false);
        if let Some(r) = result {
            out.push_str(&r);
            out.push('\n');
        }
        out
    }

    /// Writes the main continuation from `node`, with each move's
    /// variations after it. `in_variation` is false for the game's main
    /// line, whose result goes in the tags and closing token instead.
    fn write_from(&self, out: &mut String, mut node: NodeId, in_variation: bool) {
        let tree = &self.tree;
        loop {
            let children = tree[node].children();
            if tree[node].result().is_some() {
                // The game ended here, so moves after it are analysis (a
                // likely finish after a resignation). Each is a variation
                // starting with the next ply's label, which marks it as a
                // continuation of this move rather than a replacement.
                for &c in children {
                    out.push_str("(\n");
                    if let Some(intro) = &tree[c].annotation().intro {
                        out.push_str(&format!("{{{}}}\n", escape_comment(intro)));
                    }
                    self.write_move(out, c);
                    self.write_from(out, c, true);
                    out.push_str(")\n");
                }
                break;
            }
            let Some(&main) = children.first() else { break };
            self.write_move(out, main);
            for &v in &children[1..] {
                out.push_str("(\n");
                if let Some(intro) = &tree[v].annotation().intro {
                    out.push_str(&format!("{{{}}}\n", escape_comment(intro)));
                }
                self.write_move(out, v);
                self.write_from(out, v, true);
                out.push_str(")\n");
            }
            node = main;
        }
        if let Some(marker) = tree[node].end_marker() {
            out.push_str(&format!("{} {marker}\n", tree.label_after(node)));
        }
        // A variation's outside result (the rules find theirs again) goes on
        // a line of its own, unless its end word already says it.
        if in_variation
            && let Some(r) = tree[node].result()
            && !r.reason.is_on_board()
            && marker_result(tree, node) != Some(r)
        {
            out.push_str(&format!("{} {}\n", result_token(r.winner), r.reason.letter()));
        }
    }

    fn write_move(&self, out: &mut String, id: NodeId) {
        let node = &self.tree[id];
        let mv = node.mv().expect("only the root has no move");
        out.push_str(&self.tree.label_of(id).expect("only the root has no move"));
        out.push(' ');
        out.push_str(&mv.notation());
        for g in &node.annotation().glyphs {
            out.push_str(&format!(" {g}"));
        }
        if let Some(c) = &node.annotation().comment {
            out.push_str(&format!(" {{{}}}", escape_comment(c)));
        }
        out.push('\n');
    }
}

/// A set position as the `Position` tag holds it: the side to move and the
/// short format, `g [rrrrrrrr...]`.
pub(crate) fn position_tag(position: &Position) -> String {
    format!("{} {}", position.side_to_move().letter(), position.to_short_string())
}

/// The result an end word gives: it names the side to move as the loser
/// (`2s resigns`).
fn marker_result(tree: &GameTree, node: NodeId) -> Option<GameResult> {
    let reason = match tree[node].end_marker()?.to_ascii_lowercase().as_str() {
        "resigns" | "resign" => WinReason::Resignation,
        "timeout" => WinReason::Timeout,
        "forfeit" => WinReason::Forfeit,
        _ => return None,
    };
    Some(GameResult { winner: tree[node].position().side_to_move().opponent(), reason })
}

/// The reason a `ResultCode` letter names.
fn reason_from_code(code: &str) -> Option<WinReason> {
    let mut chars = code.chars();
    chars.next().filter(|_| chars.next().is_none()).and_then(WinReason::from_letter)
}

fn result_token(winner: Color) -> &'static str {
    match winner {
        Color::Gold => "1-0",
        Color::Silver => "0-1",
    }
}

/// Escapes a tag value. PGN has no way to write a line break in a tag, so
/// `\n` is our extension (for multi-line values such as arimaa.com chat).
fn escape_tag(value: &str) -> String {
    value.replace('\\', "\\\\").replace('"', "\\\"").replace('\n', "\\n")
}

fn escape_comment(text: &str) -> String {
    text.replace('\\', "\\\\").replace('}', "\\}")
}

/// Maps chess side names onto Arimaa's: `White` → `Gold`, `BlackElo` →
/// `SilverRating`.
fn normalize_tag_name(name: &str) -> String {
    for (chess, arimaa) in [("White", "Gold"), ("Black", "Silver")] {
        if let Some(rest) = name.strip_prefix(chess)
            && rest.chars().next().is_none_or(|c| c.is_ascii_uppercase())
        {
            let rest = if rest == "Elo" { "Rating" } else { rest };
            return format!("{arimaa}{rest}");
        }
    }
    name.to_string()
}

#[derive(Clone, Debug, PartialEq, Eq)]
enum Tok {
    Tag(String, String),
    Comment(String),
    Open,
    Close,
    Word(String),
}

#[derive(Debug)]
struct Lexed {
    tok: Tok,
    line: usize,
}

fn parse_error(line: usize, message: impl Into<String>) -> RecordError {
    RecordError { line, error: ParseError::new(message).into() }
}

fn is_tag_name(name: &str) -> bool {
    name.chars().next().is_some_and(|c| c.is_ascii_alphabetic())
        && name.chars().all(|c| c.is_ascii_alphanumeric() || c == '_')
}

/// Reads a PGN tag pair, `[Name "value"]`.
fn parse_pgn_tag(text: &str) -> Option<(String, String)> {
    let inner = text.strip_prefix('[')?.strip_suffix(']')?.trim();
    let (name, rest) = inner.split_once(char::is_whitespace)?;
    let quoted = rest.trim().strip_prefix('"')?.strip_suffix('"')?;
    if !is_tag_name(name) {
        return None;
    }
    let mut value = String::new();
    let mut chars = quoted.chars();
    while let Some(c) = chars.next() {
        if c == '\\' {
            match chars.next()? {
                'n' => value.push('\n'),
                c => value.push(c),
            }
        } else {
            value.push(c);
        }
    }
    Some((name.to_string(), value))
}

/// Reads an arimaa.com tag line, `Name: value`.
fn parse_colon_tag(text: &str) -> Option<(&str, &str)> {
    let (name, value) = text.split_once(':')?;
    (is_tag_name(name) && (value.is_empty() || value.starts_with(' '))).then(|| (name, value.trim()))
}

fn lex(text: &str) -> Result<Vec<Lexed>, RecordError> {
    let lines: Vec<&str> = text.lines().collect();
    let mut out = Vec::new();
    // Where arimaa.com tags may appear: before a game's move text.
    let mut header = true;
    // An open comment: its text and the line it started on.
    let mut comment: Option<(String, usize)> = None;
    let mut i = 0;
    while i < lines.len() {
        let line = i + 1;
        let trimmed = lines[i].trim();
        i += 1;
        if comment.is_none() {
            if trimmed.is_empty() || trimmed.starts_with('#') {
                continue;
            }
            if trimmed.starts_with('[') {
                let (name, value) = parse_pgn_tag(trimmed)
                    .ok_or_else(|| parse_error(line, format!("invalid tag {trimmed:?}")))?;
                out.push(Lexed { tok: Tok::Tag(name, value), line });
                header = true;
                continue;
            }
            if header && let Some((name, value)) = parse_colon_tag(trimmed) {
                let value = if value == MULTILINE {
                    let mut body = Vec::new();
                    loop {
                        let Some(next) = lines.get(i) else {
                            return Err(parse_error(line, format!("unterminated multi-line tag {name}")));
                        };
                        i += 1;
                        if next.starts_with(MULTILINE) {
                            break;
                        }
                        body.push(*next);
                    }
                    body.join("\n")
                } else {
                    value.to_string()
                };
                out.push(Lexed { tok: Tok::Tag(name.to_string(), value), line });
                continue;
            }
        } else if let Some((buf, _)) = &mut comment {
            buf.push('\n');
        }

        let mut word = String::new();
        let mut chars = lines[i - 1].chars().peekable();
        let flush = |word: &mut String, out: &mut Vec<Lexed>, header: &mut bool| {
            if !word.is_empty() {
                let w = std::mem::take(word);
                *header = RESULT_TOKENS.contains(&w.as_str());
                out.push(Lexed { tok: Tok::Word(w), line });
            }
        };
        while let Some(c) = chars.next() {
            if let Some((buf, start)) = &mut comment {
                match c {
                    '\\' if matches!(chars.peek(), Some('}' | '\\')) => buf.push(chars.next().unwrap()),
                    '}' => {
                        let (text, start) = (std::mem::take(buf), *start);
                        out.push(Lexed { tok: Tok::Comment(text.trim().to_string()), line: start });
                        comment = None;
                    }
                    _ => buf.push(c),
                }
                continue;
            }
            match c {
                '{' | '(' | ')' => {
                    flush(&mut word, &mut out, &mut header);
                    header = false;
                    match c {
                        '{' => comment = Some((String::new(), line)),
                        '(' => out.push(Lexed { tok: Tok::Open, line }),
                        _ => out.push(Lexed { tok: Tok::Close, line }),
                    }
                }
                c if c.is_whitespace() => flush(&mut word, &mut out, &mut header),
                c => word.push(c),
            }
        }
        flush(&mut word, &mut out, &mut header);
    }
    if let Some((_, start)) = comment {
        return Err(parse_error(start, "unterminated comment"));
    }
    Ok(out)
}

/// One line of play being read: the main line or a variation.
struct Frame {
    /// The last move read on this line.
    current: NodeId,
    /// Whether a move has been read on this line yet.
    moved: bool,
    /// The line has ended (a decided game, an end marker or an empty move).
    ended: bool,
    /// After a `takeback`, the next move replaces the one taken back as the
    /// main continuation.
    took_back: bool,
    /// Line of the `(` that opened it.
    open_line: usize,
    /// For a variation, the move before its `(`. A variation whose first
    /// label is the next ply continues after that move instead of replacing
    /// it.
    after: Option<NodeId>,
}

impl Frame {
    fn new(current: NodeId, open_line: usize, after: Option<NodeId>) -> Frame {
        Frame { current, moved: false, ended: false, took_back: false, open_line, after }
    }
}

/// A move being read: its label and the words after it.
struct PendingMove {
    number: u32,
    color: Color,
    words: Vec<String>,
    line: usize,
}

struct GameParser {
    record: GameRecord,
    start_line: usize,
    stack: Vec<Frame>,
    pending: Option<PendingMove>,
    intro: Option<String>,
    /// Whether any move text has been read, so a tag starts a new game.
    has_moves: bool,
    /// Moves given continuations by a variation, whose own line's next
    /// move must still become their main continuation.
    continued: Vec<NodeId>,
    result_token: Option<String>,
    /// A result token read inside a variation, waiting for its reason
    /// letter on the same line.
    line_result: Option<(Color, usize)>,
    /// A `Position` tag set the start without a move number, so the first
    /// move's label gives it.
    renumber: bool,
}

impl GameParser {
    fn new(start_line: usize) -> GameParser {
        GameParser {
            record: GameRecord::new(),
            start_line,
            stack: vec![Frame::new(GameTree::ROOT, 0, None)],
            continued: Vec::new(),
            pending: None,
            intro: None,
            has_moves: false,
            result_token: None,
            line_result: None,
            renumber: false,
        }
    }

    fn top(&mut self) -> &mut Frame {
        self.stack.last_mut().expect("the main line is always there")
    }

    fn add_tag(&mut self, name: &str, value: String, line: usize) -> Result<(), RecordError> {
        let name = normalize_tag_name(name);
        if name == "Position" {
            // The tree holds it; the writer writes it from there.
            if value.trim().is_empty() {
                return Ok(());
            }
            if self.has_moves || self.record.tree.len() > 1 {
                return Err(parse_error(line, "the Position tag must come before the moves"));
            }
            let start = StartPosition::parse(&value).map_err(|e| parse_error(line, e.to_string()))?;
            let comment = self.record.tree[GameTree::ROOT].annotation().clone();
            let mut tree = GameTree::from_position(start.position, start.move_number)
                .map_err(|error| RecordError { line, error })?;
            *tree.annotation_mut(GameTree::ROOT).expect("root") = comment;
            self.record.tree = tree;
            self.renumber = true;
            return Ok(());
        }
        self.record.set_tag(name, value);
        Ok(())
    }

    /// Adds the move being read to the tree.
    fn flush(&mut self) -> Result<(), RecordError> {
        let Some(PendingMove { number, color, words, line }) = self.pending.take() else { return Ok(()) };
        let at = |error: GameError| RecordError { line, error };
        if words.len() == 1 && words[0].eq_ignore_ascii_case("takeback") {
            let frame = self.top();
            let current = frame.current;
            let parent = self.record.tree[current].parent().ok_or_else(|| at(GameError::IsRoot))?;
            let frame = self.top();
            frame.current = parent;
            frame.took_back = true;
            frame.ended = false;
            return Ok(());
        }
        // A variation labelled with the ply after the move before it
        // continues that move (used after a line's last move); otherwise it
        // replaces that move.
        if std::mem::take(&mut self.renumber)
            && self.record.tree.len() == 1
            && color == self.record.tree[GameTree::ROOT].position().side_to_move()
            && number >= 2
        {
            self.record.tree.renumber(number).map_err(at)?;
        }
        let frame = self.stack.last().expect("main line");
        let tree = &self.record.tree;
        let label_fits = |n: NodeId| {
            number == ((tree.start_ply() + tree[n].ply()) / 2 + 1) as u32
                && color == tree[n].position().side_to_move()
        };
        if !frame.moved
            && let Some(after) = frame.after
            && !label_fits(frame.current)
            && label_fits(after)
        {
            self.continued.push(after);
            self.top().current = after;
        }
        let frame = self.stack.last().expect("main line");
        let (parent, own_line) = (frame.current, frame.moved);
        let tree = &mut self.record.tree;
        let ply = tree.start_ply() + tree[parent].ply();
        if number != (ply / 2 + 1) as u32 || color != tree[parent].position().side_to_move() {
            let found = format!("{number}{}", color.letter());
            return Err(at(GameError::OutOfSequence { expected: notation::move_label(ply), found }));
        }
        let (body, marker) = notation::parse_move_body(&words.join(" ")).map_err(|e| at(e.into()))?;
        if self.top().ended {
            // arimaa.com records end with the next move's empty label, even
            // after a decided game.
            if body == MoveBody::Empty && marker.is_none() {
                return Ok(());
            }
            return Err(at(GameError::GameOver));
        }
        let tree = &mut self.record.tree;
        let before = tree.len();
        let node = match body {
            MoveBody::Empty => {
                tree.set_end_marker(parent, marker).map_err(at)?;
                self.top().ended = true;
                return Ok(());
            }
            MoveBody::Setup(p) => tree.add_setup(parent, p).map_err(at)?,
            MoveBody::Steps(steps) => {
                let turn = build_turn(tree.begin_turn(parent).map_err(at)?, &steps).map_err(at)?;
                tree.add_turn(parent, turn).map_err(at)?
            }
        };
        // An introduction belongs to the variation's first new move: a
        // variation that repeats the game's last move to continue after it
        // passes the introduction on to the move that follows.
        if tree.len() > before
            && let Some(intro) = self.intro.take()
        {
            tree.annotation_mut(node).map_err(at)?.intro = Some(intro);
        }
        let decided = tree[node].is_terminal() || marker.is_some();
        tree.set_end_marker(node, marker).map_err(at)?;
        let frame = self.top();
        let took_back = std::mem::take(&mut frame.took_back);
        if took_back || (own_line && self.continued.contains(&parent)) {
            self.record.tree.make_first(node).map_err(at)?;
        }
        let frame = self.top();
        frame.current = node;
        frame.moved = true;
        frame.ended = decided;
        Ok(())
    }

    fn comment(&mut self, text: String, line: usize) -> Result<(), RecordError> {
        self.flush()?;
        let frame = self.stack.last().expect("main line");
        let target = if frame.moved {
            frame.current
        } else if self.stack.len() == 1 {
            GameTree::ROOT
        } else {
            self.intro = Some(join_comment(self.intro.take(), text));
            return Ok(());
        };
        let a = self.record.tree.annotation_mut(target).map_err(|error| RecordError { line, error })?;
        a.comment = Some(join_comment(a.comment.take(), text));
        Ok(())
    }

    fn word(&mut self, word: String, line: usize) -> Result<(), RecordError> {
        // A move ends at the end of its line.
        if self.pending.as_ref().is_some_and(|p| p.line != line) {
            self.flush()?;
        }
        if let Ok((number, color)) = notation::parse_move_number(&word) {
            self.flush()?;
            self.pending = Some(PendingMove { number, color, words: Vec::new(), line });
        } else if let Some(glyph) = Glyph::parse(&word) {
            self.flush()?;
            let frame = self.stack.last().expect("main line");
            if !frame.moved {
                return Err(parse_error(line, format!("{word} doesn't follow a move")));
            }
            let current = frame.current;
            self.record
                .tree
                .annotation_mut(current)
                .map_err(|error| RecordError { line, error })?
                .set_glyph(glyph);
        } else if let Some(p) = &mut self.pending {
            p.words.push(word);
        } else {
            return Err(parse_error(line, format!("expected a move number before {word:?}")));
        }
        Ok(())
    }

    fn open(&mut self, line: usize) -> Result<(), RecordError> {
        self.flush()?;
        let frame = self.stack.last().expect("main line");
        let parent = self.record.tree[frame.current].parent();
        let (true, Some(parent)) = (frame.moved, parent) else {
            return Err(parse_error(line, "a variation must follow a move"));
        };
        let after = frame.current;
        self.stack.push(Frame::new(parent, line, Some(after)));
        Ok(())
    }

    fn close(&mut self, line: usize) -> Result<(), RecordError> {
        self.flush()?;
        if self.stack.len() == 1 {
            return Err(parse_error(line, "unmatched )"));
        }
        let frame = self.stack.pop().expect("checked above");
        if !frame.moved {
            return Err(parse_error(line, "empty variation"));
        }
        // An end word ends the variation as it ends the game.
        let tree = &mut self.record.tree;
        if tree[frame.current].result().is_none()
            && let Some(result) = marker_result(tree, frame.current)
        {
            tree.end_line(frame.current, result).map_err(|error| RecordError { line, error })?;
        }
        Ok(())
    }

    /// Starts a result inside a variation (`0-1 r`): its winner, with the
    /// reason to follow.
    fn start_line_result(&mut self, token: &str, line: usize) -> Result<(), RecordError> {
        self.flush()?;
        let winner = match token {
            "1-0" => Color::Gold,
            "0-1" => Color::Silver,
            _ => return Err(parse_error(line, format!("a variation can't end in {token}"))),
        };
        if !self.top().moved {
            return Err(parse_error(line, format!("{token} doesn't follow a move")));
        }
        self.line_result = Some((winner, line));
        Ok(())
    }

    /// Ends the variation's line with the result started by
    /// `start_line_result`, given its reason letter.
    fn end_line_result(&mut self, code: Option<&str>) -> Result<(), RecordError> {
        let (winner, line) = self.line_result.take().expect("a result was started");
        let reason = code.and_then(reason_from_code);
        let Some(reason) = reason else {
            return Err(parse_error(line, "a result in a variation needs its ResultCode letter"));
        };
        let result = GameResult { winner, reason };
        let current = self.top().current;
        let tree = &mut self.record.tree;
        match tree[current].result() {
            Some(r) if r == result => {}
            Some(_) => return Err(parse_error(line, "the result doesn't match the line's end")),
            None if reason.is_on_board() => {
                return Err(parse_error(line, "the position doesn't show that result"));
            }
            None => tree.end_line(current, result).map_err(|error| RecordError { line, error })?,
        }
        self.top().ended = true;
        Ok(())
    }

    /// Ends the game: checks the variations are closed and applies the
    /// result from the result token or tags.
    fn finish(mut self, line: usize) -> Result<(usize, GameRecord), RecordError> {
        self.flush()?;
        if self.stack.len() > 1 {
            let open = self.stack.last().expect("checked").open_line;
            return Err(parse_error(open, "unclosed ("));
        }
        if let Some(token) = &self.result_token {
            match self.record.tag("Result") {
                Some(tag) if tag != token => {
                    return Err(parse_error(
                        line,
                        format!("result {token} doesn't match the Result tag {tag}"),
                    ));
                }
                _ => self.record.set_tag("Result", token.clone()),
            }
        }
        let winner = match self.record.tag("Result") {
            Some("1-0") => Some(Color::Gold),
            Some("0-1") => Some(Color::Silver),
            _ => None,
        };
        let reason = self.record.tag("ResultCode").and_then(reason_from_code);
        // The game ends where the main line's moves end; anything after
        // that came from variations.
        let end = self.stack[0].current;
        let tree = &mut self.record.tree;
        let from_marker = marker_result(tree, end);
        let from_tags = winner.zip(reason).map(|(winner, reason)| GameResult { winner, reason });
        // A record can't contradict the board: a result that the rules
        // decide is already set, and these are outside reasons.
        if let (Some(result), None) = (from_tags.or(from_marker), tree[end].result()) {
            tree.end_line(end, result).map_err(|error| RecordError { line, error })?;
        }
        Ok((self.start_line, self.record))
    }
}

fn join_comment(existing: Option<String>, text: String) -> String {
    match existing {
        Some(e) if !e.is_empty() => format!("{e} {text}"),
        _ => text,
    }
}

/// Parses every game in `text`, with the line each one starts on.
fn parse_games(text: &str) -> Result<Vec<(usize, GameRecord)>, RecordError> {
    let mut games = Vec::new();
    let mut game: Option<GameParser> = None;
    let mut last_line = 1;
    for Lexed { tok, line } in lex(text)? {
        last_line = line;
        // A tag after move text, or anything after a result token, starts
        // the next game.
        let new_game = match (&game, &tok) {
            (None, _) => true,
            (Some(g), Tok::Tag(..)) => g.has_moves || g.result_token.is_some(),
            (Some(g), _) => g.result_token.is_some(),
        };
        if new_game {
            if let Some(g) = game.take() {
                games.push(g.finish(line)?);
            }
            game = Some(GameParser::new(line));
        }
        let g = game.as_mut().expect("set above");
        if !matches!(tok, Tok::Tag(..)) {
            g.has_moves = true;
        }
        if let Some((_, result_line)) = g.line_result {
            let code = match &tok {
                Tok::Word(w) if line == result_line => Some(w.as_str()),
                _ => None,
            };
            g.end_line_result(code)?;
            if code.is_some() {
                continue;
            }
        }
        match tok {
            Tok::Tag(name, value) => g.add_tag(&name, value, line)?,
            Tok::Comment(text) => g.comment(text, line)?,
            Tok::Open => g.open(line)?,
            Tok::Close => g.close(line)?,
            Tok::Word(w) if RESULT_TOKENS.contains(&w.as_str()) && g.stack.len() > 1 => {
                g.start_line_result(&w, line)?;
            }
            Tok::Word(w) if RESULT_TOKENS.contains(&w.as_str()) => {
                g.flush()?;
                g.result_token = Some(w);
            }
            Tok::Word(w) => g.word(w, line)?,
        }
    }
    if let Some(g) = game {
        games.push(g.finish(last_line)?);
    }
    Ok(games)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::setup::default_setup;

    fn setups() -> String {
        format!(
            "1g {}\n1s {}\n",
            notation::format_placements(&default_setup(Color::Gold)),
            notation::format_placements(&default_setup(Color::Silver))
        )
    }

    fn main_moves(r: &GameRecord) -> Vec<String> {
        r.tree.main_line()[3..].iter().map(|&id| r.tree[id].mv().unwrap().notation()).collect()
    }

    #[test]
    fn plain_record_round_trips_through_game() {
        let text = format!("{}2g Ee2n Ee3n\n2s ee7s\n", setups());
        let r = GameRecord::parse(&text).unwrap();
        assert_eq!(r.tree.main_game().to_record(), text);
        assert_eq!(main_moves(&r), ["Ee2n Ee3n", "ee7s"]);
    }

    #[test]
    fn full_record_round_trips() {
        let text = format!(
            "[Event \"Test \\\"quoted\\\"\"]\n[Gold \"alice\"]\n[Silver \"bob\"]\n[PlyCount \"4\"]\n\n\
             {{Game comment.}}\n{}2g Ee2n Ee3n ?! {{Early. [%emt 0:00:12]}}\n(\n{{Quieter.}}\n\
             2g Db2n\n2s ee7s\n(\n2s db7s !? $14\n)\n)\n2s ee7s {{Brace \\}} and \\\\.}}\n",
            setups()
        );
        let r = GameRecord::parse(&text).unwrap();
        assert_eq!(r.tag("Event"), Some("Test \"quoted\""));
        assert_eq!(r.tree[GameTree::ROOT].annotation().comment.as_deref(), Some("Game comment."));
        let main = r.tree.main_line();
        let e2 = main[3];
        assert_eq!(r.tree[e2].annotation().glyphs, [Glyph::DUBIOUS]);
        assert_eq!(r.tree[main[4]].annotation().comment.as_deref(), Some("Brace } and \\."));
        let alt = r.tree[main[2]].children()[1];
        assert_eq!(r.tree[alt].annotation().intro.as_deref(), Some("Quieter."));
        let inner = r.tree[alt].children()[1];
        assert_eq!(r.tree[inner].annotation().glyphs, [Glyph::INTERESTING, Glyph(14)]);
        assert_eq!(r.to_record(), text);
    }

    #[test]
    fn results_from_tags_and_tokens() {
        let text = format!("[ResultCode \"r\"]\n[Result \"1-0\"]\n\n{}2g Ee2n\n1-0\n", setups());
        let r = GameRecord::parse(&text).unwrap();
        let end = r.tree.line_end(GameTree::ROOT);
        assert_eq!(
            r.tree[end].result(),
            Some(GameResult { winner: Color::Gold, reason: WinReason::Resignation })
        );
        assert_eq!(r.to_record(), text);

        // A draw from an old game stays in the tags.
        let old = format!("[ResultCode \"n\"]\n\n{}1/2-1/2\n", setups());
        let r = GameRecord::parse(&old).unwrap();
        assert_eq!(r.tag("Result"), Some("1/2-1/2"));
        assert_eq!(r.tree[r.tree.line_end(GameTree::ROOT)].result(), None);
        assert!(r.to_record().ends_with("1/2-1/2\n"));

        let clash = format!("[Result \"0-1\"]\n\n{}1-0\n", setups());
        assert!(GameRecord::parse(&clash).is_err());
    }

    #[test]
    fn arimaa_com_tags_and_labels() {
        let text = format!(
            "Event: Casual game\nWhite: alice\nBlack: bob\nChat: -=+=-\nhi\nthere\n-=+=-\n\n{}",
            setups().replace("1g", "1w").replace("1s", "1b")
        );
        let r = GameRecord::parse(&text).unwrap();
        assert_eq!(r.tag("Gold"), Some("alice"));
        assert_eq!(r.tag("Silver"), Some("bob"));
        assert_eq!(r.tag("Chat"), Some("hi\nthere"));
        let out = r.to_record();
        assert!(out.contains("\n1g H") && !out.contains("1w"), "labels are written with g/s");
        assert!(out.contains("[Chat \"hi\\nthere\"]"));
        let again = GameRecord::parse(&out).unwrap();
        assert_eq!((&again.tree, again.tag("Chat")), (&r.tree, Some("hi\nthere")));
    }

    #[test]
    fn chess_tag_names_are_mapped() {
        assert_eq!(normalize_tag_name("White"), "Gold");
        assert_eq!(normalize_tag_name("BlackElo"), "SilverRating");
        assert_eq!(normalize_tag_name("WhiteTitle"), "GoldTitle");
        assert_eq!(normalize_tag_name("Whitespace"), "Whitespace");
        // AEI's round-robin output.
        let text = format!(
            "[White \"bot_a\"]\n[Black \"bot_b\"]\n[TimeControl \"2/2/100/2/0\"]\n[PlyCount \"3\"]\n\
             [ResultCode \"t\"]\n[Result \"1-0\"]\n\n{}2g Ee2n\n1-0\n",
            setups()
        );
        let r = GameRecord::parse(&text).unwrap();
        assert_eq!(r.tag("Gold"), Some("bot_a"));
        let end = r.tree.line_end(GameTree::ROOT);
        assert_eq!(r.tree[end].result().unwrap().reason, WinReason::Timeout);
    }

    #[test]
    fn several_games() {
        let one = format!("[Gold \"a\"]\n\n{}2g Ee2n\n1-0\n", setups());
        let two = format!("[Gold \"b\"]\n\n{}2g Db2n\n", setups());
        let both = format!("{one}\n{two}");
        let games = GameRecord::parse_all(&both).unwrap();
        assert_eq!(games.len(), 2);
        assert_eq!(games[1].tag("Gold"), Some("b"));
        assert_eq!(main_moves(&games[1]), ["Db2n"]);
        let err = GameRecord::parse(&both).unwrap_err();
        assert_eq!(err.line, 8);
        // Without a result token, a tag after moves still starts a new game.
        let games = GameRecord::parse_all(&format!("{two}{two}")).unwrap();
        assert_eq!(games.len(), 2);
    }

    #[test]
    fn takeback_keeps_the_old_move() {
        let text = format!("{}2g Ee2n\n2s ee7s\n2s takeback\n2s db7s\n", setups());
        let r = GameRecord::parse(&text).unwrap();
        assert_eq!(main_moves(&r), ["Ee2n", "db7s"]);
        let e2 = r.tree.main_line()[3];
        assert_eq!(r.tree[e2].children().len(), 2, "the taken-back move is a variation");
    }

    #[test]
    fn lenient_layout() {
        let text = format!("{}2g Ee2n 2s ee7s (2s db7s) 3g Ee3n", setups());
        let r = GameRecord::parse(&text).unwrap();
        assert_eq!(main_moves(&r), ["Ee2n", "ee7s", "Ee3n"]);
        assert_eq!(r.tree[r.tree.main_line()[3]].children().len(), 2);
    }

    /// Gold to move: a rabbit on a2, a dog held on c3 by a cat, silver's
    /// rabbit h7 and elephant h8, as the `Position` tag holds it.
    fn set_position() -> String {
        position_tag(&crate::position::tests::pos(Color::Gold, "Ra2 Dc3 Cc4 rh7 eh8"))
    }

    #[test]
    fn games_from_a_set_position() {
        let set = set_position();
        let text = format!(
            "[Gold \"alice\"]\n[Position \"{set}\"]\n\n{{Puzzle.}}\n2g Ra2n\n(\n2g Cc4n\n)\n2s rh7s\n"
        );
        let r = GameRecord::parse(&text).unwrap();
        assert_eq!(r.tree.start_ply(), 2);
        assert_eq!(r.tag("Position"), None, "the tree holds it");
        let moves: Vec<String> =
            r.tree.main_line()[1..].iter().map(|&id| r.tree[id].mv().unwrap().notation()).collect();
        assert_eq!(moves, ["Ra2n", "rh7s"]);
        assert_eq!(r.tree[GameTree::ROOT].annotation().comment.as_deref(), Some("Puzzle."));
        let written = r.to_record();
        assert!(written.starts_with(&format!("[Gold \"alice\"]\n[Position \"{set}\"]\n[PlyCount \"2\"]")));
        assert!(written.contains("\n2g Ra2n\n(\n2g Cc4n Dc3x\n)\n2s rh7s\n"), "{written}");
        assert_eq!(GameRecord::parse(&written).unwrap().to_record(), written);

        // The first label gives the move number.
        let late =
            GameRecord::parse(&format!("[Position \"{set}\"]\n\n31g Ra2n\n31s rh7s\n32g Ra3n\n")).unwrap();
        assert_eq!(late.tree.start_ply(), 60);
        assert!(late.to_record().contains("31g Ra2n\n31s rh7s\n32g Ra3n\n"));
        assert_eq!(late.tree.main_game().to_record().lines().nth(2), Some("31g Ra2n"));
        // A label for the other side doesn't fit.
        let err = GameRecord::parse(&format!("[Position \"{set}\"]\n\n2s rh7s\n")).unwrap_err();
        assert!(matches!(err.error, GameError::OutOfSequence { .. }), "{err}");
        // With no moves, it's move 2.
        let bare = GameRecord::parse(&format!("[Position \"{set}\"]\n")).unwrap();
        assert_eq!(bare.tree.start_ply(), 2);
        assert_eq!(bare.to_record(), format!("[Position \"{set}\"]\n\n"));
    }

    #[test]
    fn the_main_game_keeps_the_set_position() {
        let set = set_position();
        let r = GameRecord::parse(&format!("[Position \"{set}\"]\n\n2g Ra2n\n")).unwrap();
        let game = r.tree.main_game();
        assert_eq!(game.start_ply(), 2);
        assert_eq!(game.to_record(), format!("[Position \"{set}\"]\n\n2g Ra2n\n"));
        assert_eq!(crate::Game::parse(&game.to_record()).unwrap(), game);
        let (tree, end) = GameTree::from_game(&game);
        assert_eq!((tree.start_ply(), tree.label_of(end).as_deref()), (2, Some("2g")));
    }

    #[test]
    fn errors_have_lines() {
        let set = set_position();
        let base = setups();
        let cases = [
            (format!("{base}2g Ee2n\n(\n3s ee7s\n)\n"), 5, "wrong label in a variation"),
            (format!("{base}2g Ee2n\n(\n2g Db2n\n"), 4, "unclosed"),
            (format!("{base})\n"), 3, "unmatched"),
            (format!("(\n{base})\n"), 1, "variation before any move"),
            (format!("{base}2g Ee2n {{open\nstill\n"), 3, "unterminated comment"),
            (format!("{base}Ee2n\n"), 3, "missing label"),
            ("[Position \"g [x]\"]\n\n2g Ee2n\n".to_string(), 1, "a bad position"),
            (format!("[Position \"{set}\"]\n\n{base}"), 3, "setups after a set position"),
            (format!("{base}2g Ee2n\n2s resigns\n3g Ee3n\n"), 5, "move after the end"),
        ];
        for (text, line, what) in cases {
            let err = GameRecord::parse(&text).err().unwrap_or_else(|| panic!("{what}: parsed"));
            assert_eq!(err.line, line, "{what}: {err}");
        }
    }

    #[test]
    fn analysis_after_a_resignation() {
        // Silver resigned after 2g. A likely finish follows as a variation
        // labelled 2s: the next ply, so it continues 2g.
        let text = format!(
            "[ResultCode \"r\"]\n[Result \"1-0\"]\n\n{}2g Ee2n\n(\n{{Likely:}}\n2s ee7s\n3g Ee3n\n)\n\
             (\n2s db7s\n)\n1-0\n",
            setups()
        );
        // The chess way, repeating the last move, reads the same.
        let repeated = text
            .replace("(\n{Likely:}\n2s", "(\n{Likely:}\n2g Ee2n\n2s")
            .replace("(\n2s db7s", "(\n2g Ee2n\n2s db7s");
        assert_eq!(GameRecord::parse(&repeated).unwrap(), GameRecord::parse(&text).unwrap());
        let r = GameRecord::parse(&text).unwrap();
        let end = r.tree.line_end(GameTree::ROOT);
        assert_eq!(r.tree[end].result().unwrap().reason, WinReason::Resignation);
        assert_eq!(r.tree[end].mv().unwrap().notation(), "Ee2n");
        let after = r.tree[end].children();
        assert_eq!(after.len(), 2, "both continuations hang off the last move");
        assert_eq!(r.tree[after[0]].annotation().intro.as_deref(), Some("Likely:"));
        assert_eq!(r.tree.len(), 7);
        assert_eq!(r.to_record(), text);
    }

    #[test]
    fn a_continuation_before_the_next_move_keeps_the_main_line() {
        // Hand-written: a continuation of 2g given before the game's 2s.
        let text = format!("{}2g Ee2n\n(\n2s db7s\n)\n2s ee7s\n3g Ee3n\n", setups());
        let r = GameRecord::parse(&text).unwrap();
        assert_eq!(main_moves(&r), ["Ee2n", "ee7s", "Ee3n"]);
        let e2 = r.tree.main_line()[3];
        assert_eq!(r.tree[e2].children().len(), 2);
        // A block starting with neither label is an error.
        let bad = format!("{}2g Ee2n\n(\n3g Ee3n\n)\n", setups());
        assert!(GameRecord::parse(&bad).is_err());
    }

    #[test]
    fn outside_results_in_variations() {
        // The main line went on, but in the variation silver resigned on
        // gold's turn, which no end word can say.
        let text = format!("{}2g Ee2n\n(\n2g Db2n\n2s ee7s\n0-1 r\n)\n2s ee7s\n", setups());
        let r = GameRecord::parse(&text).unwrap();
        let alt = r.tree[r.tree.main_line()[2]].children()[1];
        let end = r.tree.line_end(alt);
        let resigned = GameResult { winner: Color::Silver, reason: WinReason::Resignation };
        assert_eq!(r.tree[end].result(), Some(resigned));
        assert_eq!(r.tree[r.tree.line_end(GameTree::ROOT)].result(), None);
        assert_eq!(r.to_record(), text);

        // A timeout on a played line that's no longer the main line, with
        // analysis after it.
        let text = format!("{}2g Ee2n\n(\n2g Db2n\n1-0 t\n(\n2s ee7s\n)\n)\n2s ee7s\n", setups());
        let r = GameRecord::parse(&text).unwrap();
        let alt = r.tree[r.tree.main_line()[2]].children()[1];
        assert_eq!(r.tree[alt].result().unwrap().reason, WinReason::Timeout);
        assert_eq!(r.tree[alt].children().len(), 1);
        assert_eq!(r.to_record(), text.replace("1-0 t\n(\n2s ee7s\n)\n", "(\n2s ee7s\n)\n1-0 t\n"));

        // An end word in a variation gives its result too, and is written
        // alone.
        let text = format!("{}2g Ee2n\n(\n2g Db2n\n2s resigns\n)\n2s ee7s\n", setups());
        let r = GameRecord::parse(&text).unwrap();
        let alt = r.tree[r.tree.main_line()[2]].children()[1];
        assert_eq!(r.tree[alt].result().unwrap().reason, WinReason::Resignation);
        assert_eq!(r.to_record(), text);

        // The main line's closing token still ends the game.
        let text = format!("{}2g Ee2n\n(\n2g Db2n\n0-1 r\n)\n1-0\n", setups());
        assert_eq!(GameRecord::parse_all(&format!("{text}{text}")).unwrap().len(), 2);
    }

    #[test]
    fn variation_result_errors() {
        let cases = [
            ("2g Db2n\n0-1\n)", "no reason"),
            ("2g Db2n\n0-1\nr\n)", "reason on the next line"),
            ("2g Db2n\n0-1 x\n)", "unknown reason"),
            ("2g Db2n\n0-1 g\n)", "a goal the board doesn't show"),
            ("2g Db2n\n1/2-1/2 r\n)", "a draw"),
            ("0-1 r\n)", "before any move"),
            ("2g Db2n\n0-1 r\n2s ee7s\n)", "a move after the result"),
        ];
        for (block, what) in cases {
            let text = format!("{}2g Ee2n\n(\n{block}\n", setups());
            assert!(GameRecord::parse(&text).is_err(), "{what}: parsed");
        }
    }

    #[test]
    fn end_markers_round_trip() {
        let text = format!("{}2g Ee2n\n2s resigns\n", setups());
        let r = GameRecord::parse(&text).unwrap();
        let end = r.tree.line_end(GameTree::ROOT);
        assert_eq!(r.tree[end].end_marker(), Some("resigns"));
        let resigned = GameResult { winner: Color::Gold, reason: WinReason::Resignation };
        assert_eq!(r.tree[end].result(), Some(resigned), "silver resigned");
        assert!(r.to_record().ends_with("2g Ee2n\n2s resigns\n1-0\n"));
        // arimaa.com adds an empty label after the end; it's ignored.
        assert_eq!(GameRecord::parse(&format!("{text}2s\n")).unwrap(), r);
    }
}
