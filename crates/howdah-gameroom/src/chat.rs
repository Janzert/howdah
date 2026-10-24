//! The gameroom's chat room: one room-wide chat, separate from each
//! game's chat (mapped 2026-10-17 from the room's own scripts).
//!
//! The room is `/arimaa/chat/` on plain HTTP (https doesn't serve its CGIs).
//! Its server, `srv.cgi`, takes form posts that each carry the user's
//! name and the `auth` from the lobby's `state` (`me.auth`), and answers
//! in plain text:
//!
//! - `cmd=fetch` (with `data=<log pointer>` after the first): `OK`, the
//!   new pointer, then one event per line; or `Nothing new`. **The first
//!   fetch enters the room**, and comes with a backlog of the last few
//!   days' events.
//! - `cmd=users`: `OK`, then the usernames in the room, tab-separated.
//! - `cmd=msg&color=<rrggbb>&data=<text>`: says something.
//! - `cmd=out`: leaves.
//!
//! The archive, `archsrv.cgi` ([`archive`]), needs no login: `day=<unix
//! time>` gets `OK` and the 24 hours of events from then.
//!
//! Entering and leaving are archived for good and everyone sees who is
//! there, so a visit is public. The server times a user out about a
//! minute after their last request, so a client in the room must ask at
//! least every ~45 s, and should say `out` when it leaves.
//!
//! Messages are shown with the room's formatting ([`format_message`]):
//! links, emoticons, Arimaa piece symbols and `/me` emotes, in the
//! poster's color (any `rrggbb`; white is used for spoilers, which the
//! room's white log hides until selected).

use std::time::Duration;

use howdah_arimaa::{Color, PieceKind};

use crate::client::{Error, Http};
use crate::wire::{Format, encode_request};

/// The chat room's directory on arimaa.com.
pub const DEFAULT_CHAT: &str = "http://arimaa.com/arimaa/chat/";

/// How long a chat request may take.
const REQUEST_TIMEOUT: Duration = Duration::from_secs(20);

/// One event in the room's log.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ChatEvent {
    /// Unix seconds, the server's clock.
    pub time: i64,
    pub user: String,
    pub kind: ChatKind,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ChatKind {
    /// The user came in.
    In,
    /// The user left.
    Out,
    /// The server noticed the user had stopped asking (about a minute).
    Timeout,
    /// The user said something, in a color (`rrggbb`, lowercase; `None`
    /// if the log's isn't one).
    Message { color: Option<String>, text: String },
}

impl ChatEvent {
    /// The event as a line of the room's log, as [`parse_event`] reads it.
    /// Tabs and line breaks in a message become spaces (the log can't hold
    /// them).
    pub fn to_line(&self) -> String {
        let clean = |s: &str| s.replace(['\t', '\n', '\r'], " ");
        let (time, user) = (self.time, clean(&self.user));
        match &self.kind {
            ChatKind::In => format!("{time}\t{user}\tin"),
            ChatKind::Out => format!("{time}\t{user}\tout"),
            ChatKind::Timeout => format!("{time}\t{user}\ttimeout"),
            ChatKind::Message { color, text } => {
                format!("{time}\t{user}\tmsg\t{}\t{}", color.as_deref().unwrap_or(""), clean(text))
            }
        }
    }
}

/// Reads one line of the log: `<time>\t<user>\t<cmd>`, and for `msg`
/// also `\t<color>\t<text>`.
pub fn parse_event(line: &str) -> Option<ChatEvent> {
    let mut fields = line.splitn(5, '\t');
    let time = fields.next()?.trim().parse().ok()?;
    let user = fields.next()?.to_string();
    let kind = match fields.next()? {
        "in" => ChatKind::In,
        "out" => ChatKind::Out,
        "timeout" => ChatKind::Timeout,
        "msg" => {
            let color = fields.next().and_then(parse_color);
            ChatKind::Message { color, text: fields.next().unwrap_or("").to_string() }
        }
        _ => return None,
    };
    Some(ChatEvent { time, user, kind })
}

/// A color as the room writes it (`rrggbb`, with or without `#`), in
/// lowercase.
pub fn parse_color(text: &str) -> Option<String> {
    let hex = text.trim().trim_start_matches('#');
    (hex.len() == 6 && hex.bytes().all(|b| b.is_ascii_hexdigit())).then(|| hex.to_ascii_lowercase())
}

/// What a fetch brought.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Fetched {
    /// `Nothing new`.
    Nothing,
    /// The log's new pointer and the events since the last.
    Events { pointer: String, events: Vec<ChatEvent> },
}

/// Reads a `fetch` reply.
pub fn parse_fetch(reply: &str) -> Result<Fetched, Error> {
    let mut lines = reply.lines();
    match lines.next().map(str::trim) {
        Some("OK") => {
            let pointer = lines.next().map(str::trim).filter(|p| !p.is_empty());
            let pointer =
                pointer.ok_or_else(|| Error::BadReply("a chat reply without its pointer".into()))?;
            let events = lines.filter_map(parse_event).collect();
            Ok(Fetched::Events { pointer: pointer.to_string(), events })
        }
        Some(s) if s.starts_with("Nothing new") => Ok(Fetched::Nothing),
        _ => Err(bad_reply(reply)),
    }
}

/// Reads a `users` reply.
pub fn parse_users(reply: &str) -> Result<Vec<String>, Error> {
    let mut lines = reply.lines();
    if lines.next().map(str::trim) != Some("OK") {
        return Err(bad_reply(reply));
    }
    Ok(lines
        .next()
        .unwrap_or("")
        .split('\t')
        .map(str::trim)
        .filter(|u| !u.is_empty())
        .map(str::to_string)
        .collect())
}

/// Reads an archive reply: `OK`, then the events (no pointer).
pub fn parse_archive(reply: &str) -> Result<Vec<ChatEvent>, Error> {
    let mut lines = reply.lines();
    if lines.next().map(str::trim) != Some("OK") {
        return Err(bad_reply(reply));
    }
    Ok(lines.filter_map(parse_event).collect())
}

/// The room's archive, `archsrv.cgi`: the events of the 24 hours from
/// `start` (Unix seconds; the archive's page asks from local midnight).
/// It needs no login, only the Referer every request carries. Past days
/// don't change, so callers should keep what they get and ask gently.
pub async fn archive(http: &Http, base: &str, start: i64) -> Result<Vec<ChatEvent>, Error> {
    let base = if base.ends_with('/') { base.to_string() } else { format!("{base}/") };
    let body = encode_request(Format::KeyValue, &[("day", start.to_string())]);
    let reply = http.page_reply(&format!("{base}archsrv.cgi"), None, Some(body), REQUEST_TIMEOUT).await?;
    match reply.status {
        404 => Err(Error::Refused),
        s if !(200..300).contains(&s) => Err(Error::Status(s)),
        _ => parse_archive(&reply.body),
    }
}

fn bad_reply(reply: &str) -> Error {
    let start: String = reply.trim().chars().take(80).collect();
    Error::BadReply(format!("the chat room answered {start:?}"))
}

/// A user's way into the chat room.
#[derive(Clone)]
pub struct ChatClient {
    http: Http,
    /// `srv.cgi`'s URL.
    url: String,
    user: String,
    auth: String,
}

impl ChatClient {
    /// The room at `base` (its directory, [`DEFAULT_CHAT`]) as `user`, with
    /// the `auth` the lobby gave them (`state`'s `me.auth`).
    pub fn new(http: Http, base: &str, user: &str, auth: &str) -> ChatClient {
        let base = if base.ends_with('/') { base.to_string() } else { format!("{base}/") };
        ChatClient { http, url: format!("{base}srv.cgi"), user: user.to_string(), auth: auth.to_string() }
    }

    pub fn user(&self) -> &str {
        &self.user
    }

    async fn ask(&self, params: &[(&str, String)]) -> Result<String, Error> {
        let mut form = vec![("user", self.user.clone()), ("auth", self.auth.clone())];
        form.extend(params.iter().cloned());
        let body = encode_request(Format::KeyValue, &form);
        let reply = self.http.page_reply(&self.url, None, Some(body), REQUEST_TIMEOUT).await?;
        match reply.status {
            404 => Err(Error::Refused),
            s if !(200..300).contains(&s) => Err(Error::Status(s)),
            _ => Ok(reply.body),
        }
    }

    /// The events since `pointer` (the last fetch's). Without one, this
    /// enters the room, and the reply has the recent backlog.
    pub async fn fetch(&self, pointer: Option<&str>) -> Result<Fetched, Error> {
        let mut params = vec![("cmd", "fetch".to_string())];
        if let Some(p) = pointer {
            params.push(("data", p.to_string()));
        }
        parse_fetch(&self.ask(&params).await?)
    }

    /// Who is in the room.
    pub async fn users(&self) -> Result<Vec<String>, Error> {
        parse_users(&self.ask(&[("cmd", "users".to_string())]).await?)
    }

    /// Says `text` in `color` (`rrggbb`). The line comes back with the
    /// next fetch.
    pub async fn say(&self, color: &str, text: &str) -> Result<(), Error> {
        let color = parse_color(color).unwrap_or_else(|| "000000".into());
        self.ask(&[("cmd", "msg".to_string()), ("color", color), ("data", text.to_string())]).await.map(drop)
    }

    /// Leaves the room.
    pub async fn leave(&self) -> Result<(), Error> {
        self.ask(&[("cmd", "out".to_string())]).await.map(drop)
    }
}

/// A part of a formatted message.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum MessagePart {
    Text(String),
    /// An http(s) link, shown as its URL.
    Link(String),
    /// A face, by the room's image name (`smiley`, `wink`, …), with the
    /// symbol that made it.
    Face {
        name: &'static str,
        symbol: &'static str,
    },
    /// A piece symbol (`:ge`, gold elephant).
    Piece {
        color: Color,
        kind: PieceKind,
    },
}

/// A message as the room shows it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct FormattedMessage {
    /// A `/me` (or `:`) emote: shown as the user's name followed at once
    /// by the parts, rather than the name, a space and the parts.
    pub emote: bool,
    pub parts: Vec<MessagePart>,
}

/// The room's faces, in the order its script replaces them.
pub const FACES: [(&str, &str); 13] = [
    (":)", "smiley"),
    ("=)", "happy"),
    ("^^", "nice"),
    ("XD", "laugh"),
    (";)", "wink"),
    (":P", "tongue"),
    (":D", "biggrin"),
    (":|", "blank"),
    (":shocked:", "shocked"),
    (":(", "sad"),
    (":frown:", "frown"),
    (":angry:", "angry"),
    (":cry:", "cry"),
];

/// Formats a message as the room does (`format.js`): links first, then
/// each face and then each piece symbol (`:g`/`:s` and a piece letter)
/// replaced wherever it appears in the text, inside words too (`:Done`
/// starts with a big grin), as the room shows them. Then a message that
/// still starts with `:` or `/me` is an emote, without that prefix.
///
/// Where the room's script misfires, this doesn't: it replaced symbols
/// in the HTML it had built, so they could land inside links and escaped
/// characters (`>)` held a wink as `&gt;)`), and current browsers replace
/// only each symbol's first occurrence (it relied on a Firefox-only
/// argument for all of them).
pub fn format_message(text: &str) -> FormattedMessage {
    let mut parts = links(text);
    for (symbol, name) in FACES {
        parts = replace(parts, symbol, &|| MessagePart::Face { name, symbol });
    }
    for color in [Color::Gold, Color::Silver] {
        for kind in PieceKind::ALL {
            let symbol = format!(":{}{}", color.letter(), kind.letter().to_ascii_lowercase());
            parts = replace(parts, &symbol, &|| MessagePart::Piece { color, kind });
        }
    }
    let mut emote = false;
    if let Some(MessagePart::Text(first)) = parts.first_mut() {
        let rest = first.strip_prefix("/me").or_else(|| first.strip_prefix(':'));
        if let Some(rest) = rest {
            emote = true;
            *first = rest.to_string();
            if first.is_empty() {
                parts.remove(0);
            }
        }
    }
    FormattedMessage { emote, parts }
}

/// Splits out `https?://` followed by anything but whitespace or `$`.
fn links(text: &str) -> Vec<MessagePart> {
    let mut parts = Vec::new();
    let mut rest = text;
    while let Some(start) = [rest.find("http://"), rest.find("https://")].into_iter().flatten().min() {
        let len = rest[start..].find(|c: char| c.is_whitespace() || c == '$').unwrap_or(rest.len() - start);
        if start > 0 {
            parts.push(MessagePart::Text(rest[..start].to_string()));
        }
        parts.push(MessagePart::Link(rest[start..start + len].to_string()));
        rest = &rest[start + len..];
    }
    if !rest.is_empty() {
        parts.push(MessagePart::Text(rest.to_string()));
    }
    parts
}

/// Replaces every `symbol` in the text parts with `part()`.
fn replace(parts: Vec<MessagePart>, symbol: &str, part: &dyn Fn() -> MessagePart) -> Vec<MessagePart> {
    let mut out = Vec::with_capacity(parts.len());
    for p in parts {
        let MessagePart::Text(text) = p else {
            out.push(p);
            continue;
        };
        let mut pieces = text.split(symbol);
        if let Some(first) = pieces.next().filter(|f| !f.is_empty()) {
            out.push(MessagePart::Text(first.to_string()));
        }
        for piece in pieces {
            out.push(part());
            if !piece.is_empty() {
                out.push(MessagePart::Text(piece.to_string()));
            }
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn text(s: &str) -> MessagePart {
        MessagePart::Text(s.into())
    }

    #[test]
    fn events() {
        assert_eq!(
            parse_event("1263540466\tHippo\ttimeout"),
            Some(ChatEvent { time: 1263540466, user: "Hippo".into(), kind: ChatKind::Timeout })
        );
        assert_eq!(
            parse_event("1263540470\tKříž\tmsg\tF70077\thi\tthere"),
            Some(ChatEvent {
                time: 1263540470,
                user: "Kříž".into(),
                kind: ChatKind::Message { color: Some("f70077".into()), text: "hi\tthere".into() }
            })
        );
        assert_eq!(
            parse_event("1\tx\tmsg\tnope\t"),
            Some(ChatEvent {
                time: 1,
                user: "x".into(),
                kind: ChatKind::Message { color: None, text: String::new() }
            })
        );
        assert_eq!(parse_event("1\tx\tdance"), None);
        assert_eq!(parse_event(""), None);
    }

    #[test]
    fn replies() {
        assert_eq!(parse_fetch("Nothing new").unwrap(), Fetched::Nothing);
        let f = parse_fetch("OK\n75039111\n1\ta\tin\n2\ta\tmsg\t000000\thello\n\n").unwrap();
        let Fetched::Events { pointer, events } = f else { panic!() };
        assert_eq!(pointer, "75039111");
        assert_eq!(events.len(), 2);
        assert!(parse_fetch("OK\n").is_err());
        assert!(parse_fetch("<html>").is_err());
        assert_eq!(parse_users("OK\nJanzert\tbot_x\t\n").unwrap(), ["Janzert", "bot_x"]);
        assert_eq!(parse_users("OK\n").unwrap(), Vec::<String>::new());
        assert!(parse_users("error").is_err());
    }

    #[test]
    fn formatting() {
        let f = format_message("45g :ge can push through :)");
        assert!(!f.emote);
        assert_eq!(
            f.parts,
            [
                text("45g "),
                MessagePart::Piece { color: Color::Gold, kind: PieceKind::Elephant },
                text(" can push through "),
                MessagePart::Face { name: "smiley", symbol: ":)" },
            ]
        );
        // Every occurrence, and inside words.
        let f = format_message(":Done ;);)");
        assert_eq!(
            f.parts,
            [
                MessagePart::Face { name: "biggrin", symbol: ":D" },
                text("one "),
                MessagePart::Face { name: "wink", symbol: ";)" },
                MessagePart::Face { name: "wink", symbol: ";)" },
            ]
        );
        // Faces before pieces: `:shocked:` isn't a silver horse.
        assert_eq!(
            format_message(":shocked:").parts,
            [MessagePart::Face { name: "shocked", symbol: ":shocked:" }]
        );
        // Links keep their symbols, and end at whitespace or `$`.
        let f = format_message("see http://x.com/:)a$b and https://y.org");
        assert_eq!(
            f.parts,
            [
                text("see "),
                MessagePart::Link("http://x.com/:)a".into()),
                text("$b and "),
                MessagePart::Link("https://y.org".into()),
            ]
        );
        // `>)` holds no wink here.
        assert_eq!(format_message("a >) b").parts, [text("a >) b")]);
    }

    #[test]
    fn emotes() {
        let f = format_message("/me waves");
        assert!(f.emote);
        assert_eq!(f.parts, [text(" waves")]);
        let f = format_message(":x");
        assert!(f.emote);
        assert_eq!(f.parts, [text("x")]);
        // A face at the start is a face, not an emote.
        let f = format_message(":) hi");
        assert!(!f.emote);
        let f = format_message("/me");
        assert!(f.emote);
        assert!(f.parts.is_empty());
    }

    #[test]
    fn archive_and_lines() {
        assert_eq!(parse_archive("OK\n").unwrap(), []);
        let events = parse_archive("OK\n1\ta\tin\n2\ta\tmsg\tF70077\thi\n3\ta\tout\n").unwrap();
        assert_eq!(events.len(), 3);
        let lines: Vec<String> = events.iter().map(ChatEvent::to_line).collect();
        assert_eq!(lines, ["1\ta\tin", "2\ta\tmsg\tf70077\thi", "3\ta\tout"]);
        for e in &events {
            assert_eq!(parse_event(&e.to_line()).as_ref(), Some(e));
        }
        assert!(parse_archive("Nothing new").is_err());
    }

    #[test]
    fn colors() {
        assert_eq!(parse_color("FFFFFF"), Some("ffffff".into()));
        assert_eq!(parse_color("#00c0c0"), Some("00c0c0".into()));
        assert_eq!(parse_color("fff"), None);
        assert_eq!(parse_color("gggggg"), None);
    }
}
