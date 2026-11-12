//! A player's page and what it links to: the player page
//! (`playerpage.cgi?id=`), the rating history behind its graph
//! (`playerRatingHist.cgi?u=`, JSON), and the record by opponent
//! (`pastrecord.cgi?id=`, filtered by rated or unrated and bots or
//! humans, with `&oid=` for the games against one opponent). None needs
//! a session.
//!
//! The pages are HTML made for people, so they're read leniently: a field
//! that doesn't parse is left out rather than failing the page.

use crate::client::Error;

/// What a player's page says about them.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct PlayerProfile {
    pub username: String,
    /// The page's "Player #N" (not the player id).
    pub number: Option<u32>,
    /// The real name they gave.
    pub name: Option<String>,
    pub rating: Option<u32>,
    /// The rating's uncertainty.
    pub rating_uncertainty: Option<u32>,
    pub provisional: bool,
    /// The country, spelled out ("United States").
    pub country: Option<String>,
    /// "Now Online", "Yesterday", or a date, as the page writes it.
    pub last_seen: Option<String>,
    pub online: bool,
    /// The day they joined, as the page writes it ("Jan 31, 2003").
    pub joined: Option<String>,
    /// Games played: all, as gold, as silver.
    pub games: Option<u32>,
    pub games_gold: Option<u32>,
    pub games_silver: Option<u32>,
    pub wins: Option<u32>,
    pub losses: Option<u32>,
    pub draws: Option<u32>,
    /// The current streak: wins if positive, losses if negative.
    pub streak: Option<i32>,
    /// Whether the page offers Invite (a human's page does; a bot's
    /// doesn't).
    pub invite: bool,
    /// The player's time zone as seconds east of UTC, from the page's
    /// clock (`ZoneTime`).
    pub utc_offset: Option<i32>,
}

/// One rated game in a player's rating history.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RatingPoint {
    /// When the game was played (Unix seconds).
    pub time: i64,
    /// The player's rating before the game.
    pub rating: u32,
    /// The game's permanent id.
    pub game_id: String,
}

/// Which games a record counts.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct RecordFilter {
    /// Only rated games (`Some(true)`) or only unrated ones.
    pub rated: Option<bool>,
    /// Only games against bots (`Some(true)`) or only against humans.
    pub bots: Option<bool>,
}

impl RecordFilter {
    /// The filter's part of a `pastrecord.cgi` query, starting with `&`.
    pub fn query(&self) -> String {
        let mut q = String::new();
        if let Some(rated) = self.rated {
            q.push_str(if rated { "&ra=1" } else { "&ra=0" });
        }
        if let Some(bots) = self.bots {
            q.push_str(if bots { "&ty=b" } else { "&ty=h" });
        }
        q
    }
}

/// Games, wins, losses and draws.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Tally {
    pub games: u32,
    pub wins: u32,
    pub losses: u32,
    pub draws: u32,
}

/// A line of a record: the totals or one opponent.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RecordLine {
    /// The opponent's player id and username; `None` for the totals.
    pub opponent: Option<(String, String)>,
    pub all: Tally,
    /// The games the player played as gold and as silver.
    pub gold: Tally,
    pub silver: Tally,
}

/// A player's record by opponent, as `pastrecord.cgi` gives it.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct PlayerRecord {
    pub totals: Option<RecordLine>,
    /// Most games first, as the page orders them.
    pub opponents: Vec<RecordLine>,
}

/// Reads a player page.
pub fn parse_player_page(page: &str) -> Option<PlayerProfile> {
    let username = between(page, "<h2", "</h2>").and_then(|h| Some(text_of(&h[h.find('>')? + 1..])))?;
    if username.is_empty() {
        return None;
    }
    let mut p = PlayerProfile { username, ..PlayerProfile::default() };
    p.number = page.find("Player #").map(|i| digits(&page[i + 8..])).and_then(|d| d.parse().ok());
    for (label, value) in rows(page) {
        let text = text_of(value);
        match label.as_str() {
            "Name" => {
                // The Invite link follows the name.
                let name = text_of(&value[..value.find("<a ").unwrap_or(value.len())]);
                p.name = Some(name).filter(|n| !n.is_empty());
                p.invite = value.contains("invite.cgi");
            }
            "Rating" => {
                // "1677; RU 120 Provisional"
                let (rating, rest) = text.split_once(';').unwrap_or((&text, ""));
                p.rating = rating.trim().parse().ok();
                p.rating_uncertainty =
                    rest.trim().strip_prefix("RU").and_then(|r| digits(r.trim()).parse().ok());
                p.provisional = rest.contains("Provisional");
            }
            "From" => p.country = Some(text).filter(|t| !t.is_empty()),
            "Last seen" => {
                p.online = text == "Now Online";
                p.last_seen = Some(text).filter(|t| !t.is_empty());
            }
            "Joined" => p.joined = Some(text).filter(|t| !t.is_empty()),
            "Games played" => {
                // "96 (51 as Gold, 45 as Silver)"
                p.games = digits(&text).parse().ok();
                p.games_gold = text.find('(').and_then(|i| digits(&text[i + 1..]).parse().ok());
                p.games_silver = text.find(", ").and_then(|i| digits(&text[i + 2..]).parse().ok());
            }
            "Wins" => p.wins = text.parse().ok(),
            "Losses" => p.losses = text.parse().ok(),
            "Draws" => p.draws = text.parse().ok(),
            "Streak" => p.streak = text.parse().ok(),
            _ => {}
        }
    }
    p.utc_offset = zone_time(page);
    Some(p)
}

/// The rows of the page's tables with a label cell (`Label:`) and a
/// value cell, as (label without the colon, the value's HTML).
fn rows(page: &str) -> Vec<(String, &str)> {
    let mut out = Vec::new();
    let mut rest = page;
    while let Some(start) = rest.find("<tr") {
        let row = &rest[start + 3..];
        let end = row.find("</tr>").unwrap_or(row.len());
        let cells = cells(&row[..end]);
        if let [label, value] = cells[..] {
            let label = text_of(label);
            if let Some(label) = label.strip_suffix(':') {
                out.push((label.trim().to_string(), value));
            }
        }
        rest = &row[end..];
    }
    out
}

/// The cells of a row, as their inner HTML.
fn cells(row: &str) -> Vec<&str> {
    let mut cells = Vec::new();
    let mut rest = row;
    while let Some(s) = rest.find("<td") {
        let body = &rest[s..];
        let Some(open) = body.find('>') else { break };
        let Some(close) = body.find("</td>") else { break };
        if close < open {
            break;
        }
        cells.push(&body[open + 1..close]);
        rest = &body[close + 5..];
    }
    cells
}

/// The page clock's `var ZoneTime = (a - b);`, seconds east of UTC.
fn zone_time(page: &str) -> Option<i32> {
    let expr = between(page, "var ZoneTime =", ";")?;
    let expr = expr.trim().trim_start_matches('(').trim_end_matches(')');
    let (a, b) = expr.split_once(" - ")?;
    Some(a.trim().parse::<i32>().ok()? - b.trim().parse::<i32>().ok()?)
}

/// Reads `playerRatingHist.cgi`'s reply: `[{"u": …, "rh": [{"r", "id",
/// "t"}, …]}]`, with every value a string. Points that don't parse are
/// skipped.
pub fn parse_rating_history(reply: &str) -> Result<Vec<RatingPoint>, Error> {
    let value: serde_json::Value =
        serde_json::from_str(reply).map_err(|e| Error::Server(format!("rating history: {e}")))?;
    let Some(points) = value.get(0).and_then(|v| v.get("rh")).and_then(|v| v.as_array()) else {
        return Ok(Vec::new());
    };
    let field = |p: &serde_json::Value, k: &str| -> Option<String> {
        match p.get(k)? {
            serde_json::Value::String(s) => Some(s.clone()),
            serde_json::Value::Number(n) => Some(n.to_string()),
            _ => None,
        }
    };
    Ok(points
        .iter()
        .filter_map(|p| {
            Some(RatingPoint {
                time: field(p, "t")?.parse().ok()?,
                rating: field(p, "r")?.parse().ok()?,
                game_id: field(p, "id").filter(|id| !id.is_empty())?,
            })
        })
        .collect())
}

/// Reads a record page (`pastrecord.cgi`, without `oid`): a line of
/// totals and one per opponent, each followed by its gold and silver
/// lines.
pub fn parse_record(page: &str) -> PlayerRecord {
    let mut record = PlayerRecord::default();
    let mut current: Option<RecordLine> = None;
    let mut rest = page;
    let finish = |line: Option<RecordLine>, record: &mut PlayerRecord| {
        if let Some(line) = line {
            if line.opponent.is_some() {
                record.opponents.push(line);
            } else {
                record.totals = Some(line);
            }
        }
    };
    while let Some(start) = rest.find("<tr") {
        let row = &rest[start + 3..];
        let end = row.find("</tr>").unwrap_or(row.len());
        let cells = cells(&row[..end]);
        rest = &row[end..];
        if cells.len() < 6 {
            continue;
        }
        let side = text_of(cells[1]);
        let Some(tally) = tally(&cells[2..6]) else { continue };
        match side.as_str() {
            "*" => {
                finish(current.take(), &mut record);
                let opponent = opponent(cells[0]);
                if opponent.is_none() && text_of(cells[0]) != "Totals" {
                    continue;
                }
                current = Some(RecordLine {
                    opponent,
                    all: tally,
                    gold: Tally::default(),
                    silver: Tally::default(),
                });
            }
            "G" => {
                if let Some(c) = current.as_mut() {
                    c.gold = tally;
                }
            }
            "S" => {
                if let Some(c) = current.as_mut() {
                    c.silver = tally;
                }
            }
            _ => {}
        }
    }
    finish(current, &mut record);
    record
}

fn tally(cells: &[&str]) -> Option<Tally> {
    let n = |c: &str| text_of(c).parse::<u32>().ok();
    Some(Tally { games: n(cells[0])?, wins: n(cells[1])?, losses: n(cells[2])?, draws: n(cells[3])? })
}

/// The opponent in a record line's first cell: `<a href="?id=7653">name</a>`.
fn opponent(cell: &str) -> Option<(String, String)> {
    let i = cell.find("?id=")?;
    let id = digits(&cell[i + 4..]);
    let name_start = i + cell[i..].find('>')? + 1;
    let name = text_of(&cell[name_start..name_start + cell[name_start..].find("</a>")?]);
    (!id.is_empty() && !name.is_empty()).then_some((id, name))
}

fn between<'a>(s: &'a str, start: &str, end: &str) -> Option<&'a str> {
    let from = s.find(start)? + start.len();
    let to = from + s[from..].find(end)?;
    Some(&s[from..to])
}

/// The leading digits of `s`.
fn digits(s: &str) -> String {
    s.chars().take_while(char::is_ascii_digit).collect()
}

/// HTML's text, without tags, `&nbsp;` as spaces and whitespace collapsed.
fn text_of(html: &str) -> String {
    let mut text = String::new();
    let mut in_tag = false;
    for c in html.chars() {
        match c {
            '<' => in_tag = true,
            '>' => in_tag = false,
            c if !in_tag => text.push(c),
            _ => {}
        }
    }
    text.replace("&nbsp;", " ").split_whitespace().collect::<Vec<_>>().join(" ")
}

/// Whether `username` is safe to put in a URL as it is: letters, digits,
/// `_`, `-` and `.`.
pub fn plain_username(username: &str) -> bool {
    !username.is_empty() && username.chars().all(|c| c.is_ascii_alphanumeric() || "_-.".contains(c))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// `playerpage.cgi` (2026-11-11), trimmed, with the player renamed.
    const PAGE: &str = r##"<script language=javascript>
var AdjTime = 1000 * 1794446755;
var ZoneTime =  (0 - 18000);
</script>
  <body xbgcolor="#d8d8f8" background="">
    <h2 align=center>some_player</h2>
    <blockquote>
      <p align=center><i>Player #247</i></p>
      <hr width=70%>
      <p align=center><b>  <table>
    <tr valign=top>
      <td>Name:</td>
      <td>A Person &nbsp; &nbsp;       <a href="javascript:open('invite.cgi?who=some_player','','toolbar=yes');void('');">Invite</a>
 &nbsp; &nbsp; </td>
    </tr>
    <tr valign=top>
      <td><a href="/arimaa/rating/" >Rating</a>:</td>
      <td>
1677; RU 120 Provisional
&nbsp; &nbsp;
<a href="javascript:ratingGraph('some_player')"><img src="images/linechart.gif" border=0></a>
      </td>
    </tr>
    <tr valign=top>
      <td>From:</td>
      <td>United States</td>
    </tr>
    <tr valign=top>
      <td>Last seen:</td>
      <td>Now Online</td>
    </tr>
    <tr valign=top>
      <td>Joined:</td>
      <td>Jan 31, 2003</td>
    </tr>
    <tr valign=top>
      <td></td>
      <td></td>
    </tr>
    <tr valign=top>
      <td>Games played:</td>
      <td>
96 (51 as Gold, 45 as Silver)
      </td>
    </tr>
    <tr valign=top>
      <td>Wins:</td>
      <td>51</td>
    </tr>
    <tr valign=top>
      <td>Losses:</td>
      <td>45</td>
    </tr>
    <tr valign=top>
      <td>Draws:</td>
      <td>0</td>
    </tr>
    <tr valign=top>
      <td>Streak:</td>
      <td>-3</td>
    </tr>
    <tr valign=top >
      <td colspan=2 align=center>
        <table border=0>
          <tr>
            <td><a href="pastgames.cgi?id=397" xtarget="_blank">
Recent Games
</a>
</td>
          </tr>
        </table>
      </td>
    </tr>
  </table>"##;

    #[test]
    fn player_page() {
        assert_eq!(
            parse_player_page(PAGE),
            Some(PlayerProfile {
                username: "some_player".into(),
                number: Some(247),
                name: Some("A Person".into()),
                rating: Some(1677),
                rating_uncertainty: Some(120),
                provisional: true,
                country: Some("United States".into()),
                last_seen: Some("Now Online".into()),
                online: true,
                joined: Some("Jan 31, 2003".into()),
                games: Some(96),
                games_gold: Some(51),
                games_silver: Some(45),
                wins: Some(51),
                losses: Some(45),
                draws: Some(0),
                streak: Some(-3),
                invite: true,
                utc_offset: Some(-18000),
            })
        );
    }

    #[test]
    fn bot_page() {
        // A bot's page (2026-11-11) has no Invite, isn't provisional, and
        // was last seen "Yesterday".
        let page = PAGE
            .replace("(0 - 18000)", "(0 - 0)")
            .replace(r#"<a href="javascript:open('invite.cgi?who=some_player','','toolbar=yes');void('');">Invite</a>"#, "")
            .replace("1677; RU 120 Provisional", "1938; RU 49  ")
            .replace("Now Online", "Yesterday");
        let p = parse_player_page(&page).unwrap();
        assert_eq!(p.name.as_deref(), Some("A Person"));
        assert!(!p.invite && !p.provisional && !p.online);
        assert_eq!((p.rating, p.rating_uncertainty), (Some(1938), Some(49)));
        assert_eq!(p.last_seen.as_deref(), Some("Yesterday"));
        assert_eq!(p.utc_offset, Some(0));
    }

    #[test]
    fn not_a_player_page() {
        assert_eq!(parse_player_page("<html><body>Error</body></html>"), None);
    }

    #[test]
    fn rating_history() {
        let reply = r#"[{"u":"some_player","rh":[{"r":"1500","id":"166","t":"1044154722"},
            {"r":"1557","id":"220","t":"1044509214"},{"r":"x","id":"1","t":"1"}]}]"#;
        assert_eq!(
            parse_rating_history(reply).unwrap(),
            [
                RatingPoint { time: 1044154722, rating: 1500, game_id: "166".into() },
                RatingPoint { time: 1044509214, rating: 1557, game_id: "220".into() },
            ]
        );
        assert_eq!(parse_rating_history("[]").unwrap(), []);
        assert!(parse_rating_history("<html>").is_err());
    }

    /// `pastrecord.cgi` (2026-11-11), its first two lines, with the
    /// player and opponent renamed.
    const RECORD: &str = r##"<table>
  <tr bgcolor="#c8c8f8"><td><b>Against</b></td> <td><b>Side</b></td> <td><b>Games</b></td> <td><b>Wins</b></td> <td><b>Losses</b></td> <td><b>Draws</b></td> <td><b>Win%</b></td> </tr>
  <tr bgcolor=#d8d8f8><td>Totals</td> <td align=right>*</td> <td align=right><a href="?id=397&r=all">96</a></td> <td align=right><a href="?id=397&r=w">51</a></td> <td align=right><a href="?id=397&r=l">45</a></td> <td align=right><a href="?id=397&r=d">0</a></td> <td align=right>53.12</td></tr><tr bgcolor=#d8d8f8><td> </td> <td align=right>G</td> <td align=right><a href="?id=397&s=w">51</a></td> <td align=right><a href="?id=397&s=w&r=w">25</a></td> <td align=right><a href="?id=397&s=w&r=l">26</a></td> <td align=right><a href="?id=397&s=w&r=d">0</a></td> <td align=right>49.02</td></tr><tr bgcolor=#d8d8f8><td> </td> <td align=right>S</td> <td align=right><a href="?id=397&s=b">45</a></td> <td align=right><a href="?id=397&s=b&r=w">26</a></td> <td align=right><a href="?id=397&s=b&r=l">19</a></td> <td align=right><a href="?id=397&s=b&r=d">0</a></td> <td align=right>57.78</td></tr>
<tr bgcolor=#e8e8f8><td><a href="?id=7653">bot_x</a></td> <td align=right>*</td> <td align=right><a href="?id=397&oid=7653">11</a></td> <td align=right><a href="?id=397&oid=7653&r=w">0</a></td> <td align=right><a href="?id=397&oid=7653&r=l">11</a></td> <td align=right><a href="?id=397&oid=7653&r=d">0</a></td> <td align=right> 0.00</td></tr>
<tr bgcolor=#e8e8f8><td> </td> <td align=right>G</td> <td align=right><a href="?id=397&oid=7653&s=w">8</a></td> <td align=right><a href="?id=397&oid=7653&s=w&r=w">0</a></td> <td align=right><a href="?id=397&oid=7653&s=w&r=l">8</a></td> <td align=right><a href="?id=397&oid=7653&s=w&r=d">0</a></td> <td align=right> 0.00</td></tr>
<tr bgcolor=#e8e8f8><td> </td> <td align=right>S</td> <td align=right><a href="?id=397&oid=7653&s=b">3</a></td> <td align=right><a href="?id=397&oid=7653&s=b&r=w">0</a></td> <td align=right><a href="?id=397&oid=7653&s=b&r=l">3</a></td> <td align=right><a href="?id=397&oid=7653&s=b&r=d">0</a></td> <td align=right> 0.00</td></tr>
</table>"##;

    #[test]
    fn record() {
        let t = |games, wins, losses| Tally { games, wins, losses, draws: 0 };
        assert_eq!(
            parse_record(RECORD),
            PlayerRecord {
                totals: Some(RecordLine {
                    opponent: None,
                    all: t(96, 51, 45),
                    gold: t(51, 25, 26),
                    silver: t(45, 26, 19)
                }),
                opponents: vec![RecordLine {
                    opponent: Some(("7653".into(), "bot_x".into())),
                    all: t(11, 0, 11),
                    gold: t(8, 0, 8),
                    silver: t(3, 0, 3),
                }],
            }
        );
    }

    #[test]
    fn record_filter_query() {
        assert_eq!(RecordFilter::default().query(), "");
        assert_eq!(RecordFilter { rated: Some(true), bots: Some(false) }.query(), "&ra=1&ty=h");
        assert_eq!(RecordFilter { rated: Some(false), bots: Some(true) }.query(), "&ra=0&ty=b");
    }

    #[test]
    fn usernames() {
        assert!(plain_username("bot_Sharp2015CC"));
        assert!(!plain_username("a b") && !plain_username("") && !plain_username("x&u=y"));
    }
}
