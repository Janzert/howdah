//! Finished games: the gameroom's recent-games list, and a finished game as
//! the browser gameroom shows it.
//!
//! `opengamewin.cgi` on a finished game's permanent id answers with the
//! browser client's viewer page, which carries the whole game inline as
//! `arimaa.vars.<name>="<value>"` lines (JavaScript strings): the players,
//! ratings, time control, result and reason, `movelist` (one move per line,
//! with `1w`/`1b` labels), `timeused` (seconds per move) and `chat`, whose
//! first lines say when the game was played.

use std::time::Duration;

use howdah_arimaa::{GameRecord, RecordError};

use crate::state::GameState;
use crate::wire::{Format, Record};

/// A game from the lobby's `recentgames` list (ASIP 2.0 `state`), the last
/// few games finished in the gameroom.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RecentGame {
    /// The permanent id.
    pub id: String,
    /// Gold's and silver's usernames.
    pub players: [Option<String>; 2],
    pub ratings: [Option<String>; 2],
    pub time_control: Option<String>,
    pub rated: bool,
    pub postal: bool,
    /// The winner (`w`/`b`) and the reason letter, as the server writes
    /// them (old games can have `d` or `u` for the winner).
    pub result: Option<String>,
    pub reason: Option<String>,
    /// The last move's number (`5` for a game ending at 5b): the
    /// archive's `plycount`, which counts moves per side.
    pub moves: Option<u32>,
    /// When it ended, in Unix seconds.
    pub ended: Option<i64>,
    pub event: Option<String>,
}

impl RecentGame {
    pub fn from_record(r: &Record) -> Option<RecentGame> {
        let name = |k: &str| r.nonempty(k);
        Some(RecentGame {
            id: r.nonempty("id")?,
            players: [name("wusername"), name("busername")],
            ratings: [name("wrating"), name("brating")],
            time_control: name("timecontrol"),
            rated: r.flag("rated"),
            postal: r.flag("postal"),
            result: name("result"),
            reason: name("termination"),
            moves: r.int("plycount").and_then(|n| u32::try_from(n).ok()),
            ended: r.int("endts").filter(|&t| t > 0),
            event: name("event"),
        })
    }
}

/// A finished game, read from its viewer page.
#[derive(Clone, Debug, PartialEq)]
pub struct FinishedGame {
    /// The permanent id.
    pub id: String,
    /// The page's `arimaa.vars`, as strings.
    pub vars: Record,
}

impl FinishedGame {
    /// Reads game `id`'s viewer page, or `None` if the page holds no game
    /// (an error page, or the redirect a live game gets).
    pub fn from_page(id: &str, page: &str) -> Option<FinishedGame> {
        let vars = page_vars(page);
        vars.fields.contains_key("movelist").then(|| FinishedGame { id: id.to_string(), vars })
    }

    /// Gold's and silver's usernames.
    pub fn players(&self) -> [Option<String>; 2] {
        [self.vars.nonempty("wplayer"), self.vars.nonempty("bplayer")]
    }

    /// Seconds each move took, setups included, as the page gives them.
    pub fn time_used(&self) -> Vec<u32> {
        let text = self.vars.str("timeused").unwrap_or_default();
        text.split_whitespace().filter_map(|t| t.parse().ok()).collect()
    }

    /// The day it was played, `YYYY.MM.DD` as the record's `Date` tag has
    /// it, from the chat's "Played Sat Jan  4 10:03:39 2014 GMT to …" line.
    pub fn date(&self) -> Option<String> {
        let chat = self.vars.str("chat")?;
        let line = chat.lines().find_map(|l| l.trim().strip_prefix("Played "))?;
        let words: Vec<&str> = line.split_whitespace().collect();
        let [_, month, day, _, year, ..] = words[..] else { return None };
        const MONTHS: [&str; 12] =
            ["Jan", "Feb", "Mar", "Apr", "May", "Jun", "Jul", "Aug", "Sep", "Oct", "Nov", "Dec"];
        let month = MONTHS.iter().position(|m| *m == month)? + 1;
        let day: u32 = day.parse().ok()?;
        let year: u32 = year.parse().ok()?;
        Some(format!("{year:04}.{month:02}.{day:02}"))
    }

    /// The game as a final game server state: players, time control,
    /// moves, result and the permanent id, for a watch that missed the
    /// end.
    pub fn state(&self) -> GameState {
        let mut vars = self.vars.clone();
        if let Some(moves) = vars.str("movelist") {
            vars.fields.insert("moves".into(), moves.into());
        }
        vars.fields.insert("finishedId".into(), self.id.clone().into());
        GameState::from_record(vars)
    }

    /// The game as a record: its moves and result, with the players,
    /// ratings, time control, date and id as tags, and each move's time
    /// as a `%emt` command in its comment.
    pub fn record(&self) -> Result<GameRecord, RecordError> {
        let v = &self.vars;
        // The result goes in with the moves, so the reader puts it on the
        // last move (or refuses one the board contradicts).
        let mut text = String::new();
        let result = match v.nonempty("result").as_deref() {
            Some("w") => Some("1-0"),
            Some("b") => Some("0-1"),
            Some("d") => Some("1/2-1/2"),
            Some(_) => Some("*"),
            None => None,
        };
        if let Some(result) = result {
            if let Some(reason) = v.nonempty("reason").filter(|r| r.chars().all(|c| c.is_ascii_alphabetic()))
            {
                text.push_str(&format!("[ResultCode \"{reason}\"]\n"));
            }
            text.push_str(&format!("[Result \"{result}\"]\n\n"));
        }
        text.push_str(&v.str("movelist").unwrap_or_default());
        text.push('\n');
        let mut record = GameRecord::parse(&text)?;
        let [gold, silver] = self.players();
        let tags = [
            ("Gold", gold),
            ("Silver", silver),
            ("GoldRating", v.nonempty("wrating")),
            ("SilverRating", v.nonempty("brating")),
            ("TimeControl", v.nonempty("timecontrol")),
            ("Date", self.date()),
            ("GameId", Some(self.id.clone())),
        ];
        for (name, value) in tags {
            if let Some(value) = value {
                record.set_tag(name, value);
            }
        }
        let main = record.tree.main_line();
        for (&node, secs) in main[1..].iter().zip(self.time_used()) {
            if let Ok(annotation) = record.tree.annotation_mut(node) {
                annotation.set_elapsed(Duration::from_secs(secs.into()));
            }
        }
        Ok(record)
    }
}

/// The `arimaa.vars.<name> = "<value>"` assignments in a page, with the
/// JavaScript string escapes undone.
fn page_vars(page: &str) -> Record {
    const PREFIX: &str = "arimaa.vars.";
    let mut record = Record { fields: Default::default(), format: Some(Format::KeyValue) };
    let mut rest = page;
    while let Some(i) = rest.find(PREFIX) {
        rest = &rest[i + PREFIX.len()..];
        let name_len = rest.find(|c: char| !(c.is_ascii_alphanumeric() || c == '_')).unwrap_or(rest.len());
        let name = &rest[..name_len];
        let Some(value) = rest[name_len..].trim_start().strip_prefix('=') else { continue };
        let Some(value) = value.trim_start().strip_prefix('"') else { continue };
        let mut text = String::new();
        let mut chars = value.char_indices();
        let mut end = None;
        while let Some((j, c)) = chars.next() {
            match c {
                '"' => {
                    end = Some(j);
                    break;
                }
                '\\' => match chars.next() {
                    Some((_, 'n')) => text.push('\n'),
                    Some((_, 't')) => text.push('\t'),
                    Some((_, 'r')) => {}
                    Some((_, c)) => text.push(c),
                    None => break,
                },
                c => text.push(c),
            }
        }
        let Some(end) = end else { break };
        record.fields.insert(name.to_string(), text.into());
        rest = &value[end + 1..];
    }
    record
}

#[cfg(test)]
mod tests {
    use howdah_arimaa::{Color, GameResult, GameTree, WinReason};

    use super::*;

    /// The viewer page of game 671438, trimmed.
    const PAGE: &str = r#"<script type="text/javascript">
arimaa.vars.viewfrom="w"
arimaa.vars.title="Game 671438"
arimaa.vars.wplayer="bot_testbot"
arimaa.vars.bplayer="bot_OpFor"
arimaa.vars.wrating="1400"
arimaa.vars.brating="1848"
arimaa.vars.timecontrol="30s/0/0/0/3m"
arimaa.vars.rated="0"
arimaa.vars.result="b"
arimaa.vars.reason="s"
arimaa.vars.movelist="1w Ra1 Rb1 Rc1 Cd1 Ce1 Rf1 Rg1 Rh1 Ra2 Hb2 Dc2 Hd2 Ee2 Df2 Mg2 Rh2\n1b rh7 ra7 rh8 rg8 rf8 rc8 rb8 ra8 ce8 cd8 df7 dc7 hg7 hb7 me7 ed7\n2w Hd2n Ee2n Ce1n Ee3n\n2b hg7s ed7s ed6s ed5s"
arimaa.vars.chat="bot_testbot (1400) vs bot_OpFor (1848)\nPlayed Sun Oct  4 06:58:10 2026 GMT to Sun Oct  4 07:01:12 2026 GMT\n\nAverage move times: \"quoted\" \\ 0.28m\n"
arimaa.vars.timeused="1 1 20 20"
arimaa.vars.nosetupreserve = "1"
arimaa.vars.planmove = "-1" // force moves to be added as real moves,
arimaa.window();
</script>"#;

    #[test]
    fn page_vars_are_read() {
        let vars = page_vars(PAGE);
        assert_eq!(vars.str("wplayer").as_deref(), Some("bot_testbot"));
        assert_eq!(vars.str("nosetupreserve").as_deref(), Some("1"));
        assert_eq!(vars.str("planmove").as_deref(), Some("-1"));
        assert!(vars.str("chat").unwrap().contains("times: \"quoted\" \\ 0.28m\n"));
        assert_eq!(vars.str("movelist").unwrap().lines().count(), 4);
        assert!(!vars.fields.contains_key("window"));
    }

    #[test]
    fn a_finished_game_becomes_a_record() {
        let game = FinishedGame::from_page("671438", PAGE).unwrap();
        assert_eq!(game.time_used(), [1, 1, 20, 20]);
        assert_eq!(game.date().as_deref(), Some("2026.10.04"));
        let state = game.state();
        assert_eq!(state.moves.len(), 4);
        assert_eq!(state.moves[2], "Hd2n Ee2n Ce1n Ee3n");
        assert_eq!(state.result, Some(GameResult { winner: Color::Silver, reason: WinReason::Score }));
        assert_eq!(state.finished_id.as_deref(), Some("671438"));
        let record = game.record().unwrap();
        assert_eq!(record.tag("Gold"), Some("bot_testbot"));
        assert_eq!(record.tag("SilverRating"), Some("1848"));
        assert_eq!(record.tag("GameId"), Some("671438"));
        assert_eq!(record.tag("Date"), Some("2026.10.04"));
        let end = record.tree.line_end(GameTree::ROOT);
        let main = record.tree.main_line();
        let emt: Vec<_> = main[1..].iter().map(|&n| record.tree[n].annotation().command("emt")).collect();
        assert_eq!(emt, ["0:00:01", "0:00:01", "0:00:20", "0:00:20"].map(|t| Some(t.to_string())));
        assert!(
            record.to_record().contains("2s hg7s ed7s ed6s ed5s {[%emt 0:00:20]}"),
            "{}",
            record.to_record()
        );
        assert_eq!(record.tree.main_game().moves().len(), 4);
        assert_eq!(
            record.tree[end].result(),
            Some(GameResult { winner: Color::Silver, reason: WinReason::Score })
        );
    }

    #[test]
    fn pages_without_a_game() {
        let expired = "<title>Expired Game</title><b>Cannot find the game id for this game.</b>";
        assert_eq!(FinishedGame::from_page("1", expired), None);
        let live = r#"<meta HTTP-EQUIV="REFRESH" CONTENT="0; URL=http://x/js_sit.cgi?sid=1">"#;
        assert_eq!(FinishedGame::from_page("1", live), None);
    }

    #[test]
    fn recent_games_are_read() {
        let r = Record::decode(
            r#"{"recentgames":[{"id":"671442","wusername":"a","busername":"b","wrating":"2678",
                "timecontrol":"20s/20m/100","rated":"1","postal":"0","result":"w","termination":"g",
                "plycount":"33","endts":"1791130988","event":"Casual game"}]}"#,
        )
        .unwrap();
        let games: Vec<RecentGame> =
            r.list("recentgames").iter().filter_map(RecentGame::from_record).collect();
        assert_eq!(games.len(), 1);
        let g = &games[0];
        assert_eq!(
            (g.id.as_str(), g.players[1].as_deref(), g.ratings[1].as_deref()),
            ("671442", Some("b"), None)
        );
        assert!(g.rated && !g.postal);
        assert_eq!(
            (g.result.as_deref(), g.reason.as_deref(), g.moves, g.ended),
            (Some("w"), Some("g"), Some(33), Some(1791130988))
        );
    }
}
