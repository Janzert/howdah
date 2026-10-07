//! The game server's game state, read from a `gamestate` reply.
//!
//! Field meanings follow the ASIP 1.0 spec and the browser client
//! (`jsClient/pro/arimaa.js`), which is the most definitive source for how
//! the server's fields are used.

use std::time::{Duration, Instant};

use howdah_arimaa::{Color, GameResult, WinReason};

use crate::clock_sync::ClockSync;
use crate::wire::Record;

/// A player's role at the table.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Role {
    Player(Color),
    Viewer,
}

impl Role {
    pub fn letter(self) -> char {
        match self {
            Role::Player(Color::Gold) => 'w',
            Role::Player(Color::Silver) => 'b',
            Role::Viewer => 'v',
        }
    }

    pub fn from_letter(s: &str) -> Option<Role> {
        match s {
            "w" | "g" => Some(Role::Player(Color::Gold)),
            "b" | "s" => Some(Role::Player(Color::Silver)),
            "v" => Some(Role::Viewer),
            _ => None,
        }
    }
}

/// A side as the server writes it (`w`/`b`).
pub fn side_from_letter(s: &str) -> Option<Color> {
    match s {
        "w" | "g" => Some(Color::Gold),
        "b" | "s" => Some(Color::Silver),
        _ => None,
    }
}

/// A line of a game's chat.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ChatLine {
    /// Who wrote it, when the line says.
    pub side: Option<Color>,
    /// The move it was written at (`6g`); the server leaves it out for
    /// some lines (an empty message).
    pub label: Option<String>,
    pub text: String,
}

/// Splits a game's `chat` field into lines. The server writes each as
/// `<side> <move label>: <text>` (`g 6g: hello`, seen 2026-10-06), as
/// sent: not escaped. A line that doesn't fit is kept whole as text.
pub fn parse_chat(chat: &str) -> Vec<ChatLine> {
    chat.lines()
        .filter(|l| !l.trim().is_empty())
        .map(|line| {
            let whole = || ChatLine { side: None, label: None, text: line.to_string() };
            let Some((head, text)) = line.split_once(':') else { return whole() };
            let mut words = head.split_whitespace();
            let (Some(side), label, None) =
                (words.next().and_then(side_from_letter), words.next(), words.next())
            else {
                return whole();
            };
            let text = text.strip_prefix(' ').unwrap_or(text).to_string();
            ChatLine { side: Some(side), label: label.map(str::to_string), text }
        })
        .collect()
}

/// The clocks as the server reports them, measured at the reply.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ServerClock {
    /// Reserves (gold, silver).
    pub reserves: [Duration; 2],
    /// Time the side to move has used on its turn so far.
    pub turn_elapsed: Duration,
    /// Time since the game started (`tcgamenow`).
    pub game_elapsed: Option<Duration>,
    /// Whether the side to move's clock runs: the server has started its
    /// turn (`{w,b}startmove`), which it hasn't before the game starts.
    pub running: bool,
    /// The server's clock when the turn started (`{w,b}startmove`) and when
    /// it replied (`timeonserver`), in whole seconds.
    pub turn_start: Option<i64>,
    pub server_time: Option<i64>,
    /// When the turn and the game started, as local instants, from the
    /// estimated server clock ([`ServerClock::estimate`]). Without them,
    /// `turn_elapsed` and `game_elapsed` count from the reply's arrival.
    pub turn_started_at: Option<Instant>,
    pub game_started_at: Option<Instant>,
    /// The estimated time for a request to reach the server.
    pub one_way: Duration,
}

impl ServerClock {
    /// Places the turn's and the game's start on the local clock, through
    /// the estimated server clock.
    pub fn estimate(&mut self, sync: &ClockSync) {
        self.turn_started_at =
            self.turn_start.filter(|_| self.running).and_then(|s| sync.local_instant(s as f64));
        self.game_started_at = match (self.server_time, self.game_elapsed) {
            (Some(now), Some(e)) => sync.local_instant(now as f64 - e.as_secs_f64()),
            _ => None,
        };
        self.one_way = sync.one_way_delay();
    }
}

/// What a game server reply says about the game.
#[derive(Clone, Debug)]
pub struct GameState {
    pub role: Option<Role>,
    /// Gold's and silver's usernames, without the `* ` annotation.
    pub players: [Option<String>; 2],
    /// Whether each player is at the table (`wpresent`/`bpresent`: when
    /// they sat, `0` once they've left), where the state says. A seat is
    /// present while it has a long poll open or makes requests: `leave`
    /// or a closed poll connection clears it within seconds, and any
    /// request from the seat brings it back.
    pub present: [Option<bool>; 2],
    pub time_control: Option<String>,
    pub rated: bool,
    pub postal: bool,
    /// Whether the game has started (`starttime`).
    pub started: bool,
    /// Whether both players are seated, so the game can start
    /// (`canstart`).
    pub can_start: bool,
    /// Side to move.
    pub turn: Option<Color>,
    /// Every move, without move numbers (`moves` split by line).
    pub moves: Vec<String>,
    pub chat: String,
    /// The side asking for a takeback, while the request waits for an
    /// answer (`takeback=w 7w`: gold, at move 7w).
    pub takeback: Option<Color>,
    /// The result, once the game is over.
    pub result: Option<GameResult>,
    /// The raw two-letter result, for results [`GameResult`] can't hold.
    pub result_code: Option<String>,
    /// The permanent game id of a finished game.
    pub finished_id: Option<String>,
    /// How long the last move took (`lastmoveused`, whole seconds; `0`
    /// before the first). Earlier moves' times aren't sent.
    pub last_move_used: Option<Duration>,
    pub clock: Option<ServerClock>,
    /// The reply itself.
    pub raw: Record,
}

impl GameState {
    /// Reads a reply whose `moves` and `chat` are complete (a `gamestate`
    /// reply, or an update with the earlier parts added back).
    pub fn from_record(raw: Record) -> GameState {
        let player = |key: &str| raw.nonempty(key).map(|p| p.trim_start_matches("* ").trim().to_string());
        // ASIP sends `result=wg`; the browser client's server sends
        // `result=w` and `reason=g`.
        let result_code = raw.nonempty("result").map(|r| match raw.nonempty("reason") {
            Some(reason) if r.len() == 1 => format!("{r}{reason}"),
            _ => r,
        });
        let result = result_code.as_deref().and_then(parse_result);
        // A rated game's time control starts with `R ` (`R 2m/5m/100/0/30m`).
        let tc = raw.nonempty("timecontrol");
        let rated_tc = tc.as_deref().is_some_and(|t| t.starts_with("R "));
        let time_control = tc.map(|t| t.strip_prefix("R ").unwrap_or(&t).trim().to_string());
        GameState {
            role: raw.str("role").as_deref().and_then(Role::from_letter),
            players: [player("wplayer"), player("bplayer")],
            present: ["wpresent", "bpresent"].map(|k| raw.int(k).map(|t| t > 0)),
            time_control,
            rated: raw.flag("rated") || rated_tc,
            postal: raw.flag("postal"),
            started: raw.int("starttime").is_some_and(|t| t > 0),
            can_start: raw.flag("canstart"),
            turn: raw.str("turn").as_deref().and_then(side_from_letter),
            moves: split_moves(&raw.str("moves").unwrap_or_default()),
            chat: raw.str("chat").unwrap_or_default(),
            takeback: raw.nonempty("takeback").and_then(|t| side_from_letter(t.split_whitespace().next()?)),
            result,
            result_code,
            finished_id: raw.nonempty("finishedId"),
            last_move_used: raw
                .int("lastmoveused")
                .and_then(|t| u64::try_from(t).ok())
                .map(Duration::from_secs),
            clock: server_clock(&raw),
            raw,
        }
    }
}

/// Splits a `moves` field into the moves played, without their numbers
/// (`2w Ee2n` is `Ee2n`). A move number with no steps yet (the move being
/// played) is left out.
///
/// A takeback doesn't shorten the field: the server appends a
/// `<label> takeback` line for each ply taken back, then the label of the
/// move to play (`…\n6b cf7s cf6x\n7w takeback\n6b takeback\n6w`). Each
/// such line undoes the move before it, as in the record format.
pub fn split_moves(text: &str) -> Vec<String> {
    let mut moves = Vec::new();
    for line in text.lines() {
        let line = line.trim();
        let (label, body) = line.split_once(char::is_whitespace).unwrap_or((line, ""));
        let numbered = label.len() > 1
            && label[..label.len() - 1].chars().all(|c| c.is_ascii_digit())
            && "wbgs".contains(&label[label.len() - 1..]);
        let body = if numbered { body.trim() } else { line };
        if body.eq_ignore_ascii_case("takeback") {
            moves.pop();
        } else if !body.is_empty() {
            moves.push(body.to_string());
        }
    }
    moves
}

/// Reads a result such as `wg` (gold won by goal). The winner can also be
/// written `g`/`s`.
pub fn parse_result(code: &str) -> Option<GameResult> {
    let mut chars = code.chars();
    let winner = side_from_letter(&chars.next()?.to_string())?;
    let reason = WinReason::from_letter(chars.next()?)?;
    Some(GameResult { winner, reason })
}

/// The clocks, as the browser client works them out: the turn has run
/// for `timeonserver - {w,b}startmove` seconds.
fn server_clock(raw: &Record) -> Option<ServerClock> {
    let secs = |key: &str| raw.int(key).map(|s| Duration::from_secs(s.max(0) as u64));
    let reserves = [secs("tcwreserve")?, secs("tcbreserve")?];
    let now = raw.int("timeonserver");
    let started = match raw.str("turn").as_deref().and_then(side_from_letter) {
        Some(Color::Gold) => raw.int("wstartmove"),
        Some(Color::Silver) => raw.int("bstartmove"),
        None => None,
    };
    let running = started.is_some_and(|s| s > 0);
    let turn_elapsed = match (now, started) {
        (Some(now), Some(start)) if start > 0 => Duration::from_secs((now - start).max(0) as u64),
        _ => Duration::ZERO,
    };
    Some(ServerClock {
        reserves,
        turn_elapsed,
        game_elapsed: secs("tcgamenow"),
        running,
        turn_start: started.filter(|&s| s > 0),
        server_time: now,
        turn_started_at: None,
        game_started_at: None,
        one_way: Duration::ZERO,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn moves_lose_their_numbers() {
        let text = "1w Ra1 Rb1\n1b ra8 rb8\n2w Ee2n Ee3n\n2b\n";
        assert_eq!(split_moves(text), ["Ra1 Rb1", "ra8 rb8", "Ee2n Ee3n"]);
        assert_eq!(split_moves("1g Ra1\n1s ra8"), ["Ra1", "ra8"]);
        assert!(split_moves("").is_empty());
    }

    #[test]
    fn presence() {
        let s = GameState::from_record(Record::decode("wpresent=1791308328\nbpresent=0\n").unwrap());
        assert_eq!(s.present, [Some(true), Some(false)]);
        let none = GameState::from_record(Record::decode("turn=w\n").unwrap());
        assert_eq!(none.present, [None, None]);
    }

    #[test]
    fn chat_lines() {
        // As game 539482 had them.
        let chat =
            "g 1g: alone before anyone sits\ns 1g: hello <b>bold</b> & \"q\": é\ng: \nno label at all\n";
        let lines = parse_chat(chat);
        assert_eq!(lines.len(), 4);
        assert_eq!(
            lines[1],
            ChatLine {
                side: Some(Color::Silver),
                label: Some("1g".into()),
                text: "hello <b>bold</b> & \"q\": é".into()
            }
        );
        assert_eq!(lines[2], ChatLine { side: Some(Color::Gold), label: None, text: String::new() });
        assert_eq!(lines[3].side, None);
        assert_eq!(lines[3].text, "no label at all");
    }

    #[test]
    fn takeback_lines_undo_moves() {
        // As the server wrote them in game 539467: a takeback of two plies
        // asked on gold's turn, gold's move again, then one of one ply.
        let text = "5w Re1n\n5b ra7s\n6w Rh1n\n6b cf7s cf6x\n7w takeback\n6b takeback\n6w";
        assert_eq!(split_moves(text), ["Re1n", "ra7s"]);
        let text = format!("{text} Rh1n\n6b takeback\n6w");
        assert_eq!(split_moves(&text), ["Re1n", "ra7s"]);
        assert_eq!(split_moves(&format!("{text} Hh3n\n6b")), ["Re1n", "ra7s", "Hh3n"]);
        assert!(split_moves("1w takeback").is_empty(), "nothing to take back");
    }

    #[test]
    fn results_and_roles() {
        assert_eq!(parse_result("wg"), Some(GameResult { winner: Color::Gold, reason: WinReason::Goal }));
        assert_eq!(parse_result("bt").unwrap().winner, Color::Silver);
        assert_eq!(parse_result("w"), None);
        assert_eq!(Role::from_letter("v"), Some(Role::Viewer));
    }

    #[test]
    fn results_in_both_forms() {
        let asip = GameState::from_record(Record::decode("result=bg\n").unwrap());
        let browser = GameState::from_record(Record::decode(r#"{"result":"w","reason":"t"}"#).unwrap());
        assert_eq!(asip.result.unwrap().winner, Color::Silver);
        assert_eq!(browser.result, Some(GameResult { winner: Color::Gold, reason: WinReason::Timeout }));
        assert_eq!(browser.result_code.as_deref(), Some("wt"));
    }

    #[test]
    fn state_with_clock() {
        let raw = Record::decode(
            "role=v\nwplayer=* bot_a\nbplayer=bot_b\nturn=b\nmoves=1w Ra1%132w\nresult=\n\
             tcwreserve=60\ntcbreserve=45\ntimeonserver=1000\nbstartmove=990\ntcgamenow=300\nstarttime=700\n",
        )
        .unwrap();
        let s = GameState::from_record(raw);
        assert_eq!(s.role, Some(Role::Viewer));
        assert_eq!(s.players, [Some("bot_a".into()), Some("bot_b".into())]);
        assert_eq!(s.moves, ["Ra1"]);
        assert!(s.started && s.result.is_none());
        let clock = s.clock.unwrap();
        assert_eq!(clock.reserves, [Duration::from_secs(60), Duration::from_secs(45)]);
        assert_eq!(clock.turn_elapsed, Duration::from_secs(10));
        assert_eq!(clock.game_elapsed, Some(Duration::from_secs(300)));
        assert_eq!(s.takeback, None);
    }

    #[test]
    fn rated_time_controls_lose_their_mark() {
        // As game 539472's state had it.
        let s = GameState::from_record(Record::decode(r#"{"timecontrol":"R 2m/5m/100/0/30m"}"#).unwrap());
        assert_eq!(s.time_control.as_deref(), Some("2m/5m/100/0/30m"));
        assert!(s.rated);
        let s = GameState::from_record(
            Record::decode(r#"{"timecontrol":"2m/5m/100/0/30m","rated":"0"}"#).unwrap(),
        );
        assert_eq!(s.time_control.as_deref(), Some("2m/5m/100/0/30m"));
        assert!(!s.rated);
    }

    #[test]
    fn takeback_requests() {
        // As in game 539467's updates, while silver's answer was awaited.
        let s = GameState::from_record(Record::decode(r#"{"turn":"b","takeback":"w 6b"}"#).unwrap());
        assert_eq!(s.takeback, Some(Color::Gold));
        let s = GameState::from_record(Record::decode(r#"{"turn":"b","denied_w":"1"}"#).unwrap());
        assert_eq!(s.takeback, None);
    }
}
