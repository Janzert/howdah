//! Finding a player's games: the gameroom's player search
//! (`searchPlayers.cgi`, which needs a session) and a player's past games
//! (`pastgames.cgi?id=<player id>`, 50 a page, newest first); and the
//! postal games being played (`postalgames.cgi`), which the lobby's live
//! list leaves out.
//!
//! All are HTML pages made for people, so they're read leniently: a row
//! that doesn't parse is skipped rather than failing the page.

use howdah_arimaa::Color;

/// A player the search found.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PlayerMatch {
    /// The player id, which [`PastGame`] lists are asked for by.
    pub id: String,
    pub username: String,
    /// The real name, as the player gave it.
    pub name: Option<String>,
}

/// A game from a player's past games.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PastGame {
    /// The permanent id.
    pub id: String,
    /// Gold's and silver's usernames.
    pub players: [String; 2],
    /// The players' ratings now (the page doesn't give the game's).
    pub ratings: [Option<String>; 2],
    pub time_control: Option<String>,
    /// Whether it was rated (an `R`, in bold on newer pages, before the
    /// time control).
    pub rated: bool,
    pub winner: Option<Color>,
    /// The reason letter, as the server writes it.
    pub reason: Option<String>,
    /// The last move's number.
    pub moves: Option<u32>,
    /// When it finished, as the page writes it but without its "YLT"
    /// label ("Jan 4, 2014 10:38 am"; "Sun 4:23 pm" in the past week): in
    /// the login's time zone, or UTC without a session.
    pub finished: Option<String>,
}

/// A postal game being played, from the gameroom's postal games page.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PostalGame {
    /// The gameroom id, to watch it by.
    pub gid: String,
    /// Gold's and silver's usernames.
    pub players: [String; 2],
    pub ratings: [Option<String>; 2],
    pub time_control: Option<String>,
    pub rated: bool,
}

/// A page of a player's past games.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct PastGames {
    pub games: Vec<PastGame>,
    /// The offset of the next (older) page, if there is one.
    pub next: Option<u32>,
}

/// The players on a search results page.
pub fn parse_search(page: &str) -> Vec<PlayerMatch> {
    const LINK: &str = "playerpage.cgi?id=";
    let mut players = Vec::new();
    let mut rest = page;
    while let Some(i) = rest.find(LINK) {
        rest = &rest[i + LINK.len()..];
        let id: String = rest.chars().take_while(char::is_ascii_digit).collect();
        let Some(username) = rest.find('>').and_then(|s| {
            let body = &rest[s + 1..];
            body.find("</a>").map(|e| text_of(&body[..e]))
        }) else {
            break;
        };
        // The real name is the next cell.
        let name = rest
            .find("</td>")
            .map(|e| &rest[e + 5..])
            .and_then(|r| Some(&r[r.find("<td")?..]))
            .and_then(|r| Some(text_of(&r[r.find('>')? + 1..r.find("</td>")?])))
            .filter(|n| !n.is_empty());
        if !id.is_empty() && !username.is_empty() {
            players.push(PlayerMatch { id, username, name });
        }
    }
    players
}

/// A page of past games, the one at `offset`.
pub fn parse_past_games(page: &str, offset: u32) -> PastGames {
    // Each row's middle cell starts with the time control in a
    // `<font size=1>`; gold's cell ends the text before it.
    let parts: Vec<&str> = page.split("<font size=1>").collect();
    let games = parts.windows(2).filter_map(|w| past_game(w[0], w[1])).collect();
    let next = page
        .split("&off=")
        .skip(1)
        .filter_map(|r| r.chars().take_while(char::is_ascii_digit).collect::<String>().parse().ok())
        .filter(|&n| n > offset)
        .min();
    PastGames { games, next }
}

/// A row from the text before its middle cell and the text from there on.
fn past_game(before: &str, row: &str) -> Option<PastGame> {
    let before = &before[..before.rfind("<td")?];
    let gold = &before[before.rfind("<td")?..];
    let time_control = text_of(&row[..row.find("</font>")?]);
    let (rated, time_control) = match time_control.strip_prefix("R ") {
        Some(tc) => (true, tc.to_string()),
        None => (false, time_control),
    };
    let time_control = Some(time_control).filter(|t| !t.is_empty());
    let id: String = {
        let r = &row[row.find("jsShowGame.cgi?gid=")? + 19..];
        r.chars().take_while(char::is_ascii_digit).collect()
    };
    // Silver's cell is the first after the middle cell's board table.
    let after = &row[row.find("</table>")? + 8..];
    let after = &after[after.find("</td>")? + 5..];
    let mut cells = Vec::new();
    let mut rest = after;
    while let Some(s) = rest.find("<td") {
        let body = &rest[s..];
        let Some(open) = body.find('>') else { break };
        let Some(close) = body.find("</td>") else { break };
        cells.push(&body[open + 1..close]);
        rest = &body[close + 5..];
    }
    let silver = cells.first()?;
    let [(gold_name, gold_rating, gold_won), (silver_name, silver_rating, silver_won)] =
        [player_cell(gold)?, player_cell(silver)?];
    let mut info = cells[1..].iter().map(|c| text_of(c)).filter(|t| !t.is_empty());
    let reason = info.next().filter(|r| r.chars().all(|c| c.is_ascii_alphabetic()));
    let moves = info.next().and_then(|m| m.parse().ok());
    let finished = info.next().map(|f| f.strip_suffix("YLT").unwrap_or(&f).trim_end().to_string());
    let winner = match (gold_won, silver_won) {
        (true, false) => Some(Color::Gold),
        (false, true) => Some(Color::Silver),
        _ => None,
    };
    (!id.is_empty()).then_some(PastGame {
        id,
        players: [gold_name, silver_name],
        ratings: [gold_rating, silver_rating],
        time_control,
        rated,
        winner,
        reason,
        moves,
        finished,
    })
}

/// The games on the postal games page. Its rows are laid out as a past
/// games page's, with the board linking `openGame('<gid>', …)`.
pub fn parse_postal_games(page: &str) -> Vec<PostalGame> {
    let parts: Vec<&str> = page.split("<font size=1>").collect();
    parts.windows(2).filter_map(|w| postal_game(w[0], w[1])).collect()
}

fn postal_game(before: &str, row: &str) -> Option<PostalGame> {
    let before = &before[..before.rfind("<td")?];
    let gold = &before[before.rfind("<td")?..];
    let time_control = text_of(&row[..row.find("</font>")?]);
    let (rated, time_control) = match time_control.strip_prefix("R ") {
        Some(tc) => (true, tc.to_string()),
        None => (false, time_control),
    };
    let gid: String = {
        let r = &row[row.find("openGame('")? + 10..];
        r.chars().take_while(char::is_ascii_digit).collect()
    };
    let after = &row[row.find("</table>")? + 8..];
    let after = &after[after.find("</td>")? + 5..];
    let silver = &after[after.find("<td")?..];
    let silver = &silver[..silver.find("</td>")?];
    let [(gold_name, gold_rating, _), (silver_name, silver_rating, _)] =
        [player_cell(gold)?, player_cell(silver)?];
    (!gid.is_empty()).then_some(PostalGame {
        gid,
        players: [gold_name, silver_name],
        ratings: [gold_rating, silver_rating],
        time_control: Some(time_control).filter(|t| !t.is_empty()),
        rated,
    })
}

/// A player's cell: the username, the rating, and whether they won (a `*`
/// next to the name).
fn player_cell(cell: &str) -> Option<(String, Option<String>, bool)> {
    let a = cell.find("<a ")?;
    let name_start = a + cell[a..].find('>')? + 1;
    let name_end = name_start + cell[name_start..].find("</a>")?;
    let name = text_of(&cell[name_start..name_end]);
    let after = &cell[name_end + 4..];
    let (mark, details) = after.split_once("<br>").unwrap_or((after, ""));
    let won = cell[..a].contains('*') || mark.contains('*');
    let rating = text_of(details)
        .split_whitespace()
        .find(|t| t.chars().all(|c| c.is_ascii_digit()))
        .map(str::to_string);
    (!name.is_empty()).then_some((name, rating, won))
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

#[cfg(test)]
mod tests {
    use super::*;

    /// `postalgames.cgi` (2026-10-06), trimmed to its first row, with the
    /// players renamed.
    const POSTAL: &str = r#"<tr><td colspan=3 align=center><br><b>Postal Games</b></td></tr><tr>
  <td align=right>
    <a href="javascript:playerPage(13566)">gold_player</a>
    <br><font size=-1>1206 &nbsp; US</font>
  </td>
  <td align=center>
    <nobr><font size=1><b>R</b> <a href="timecontrol.cgi?tc=1d/60d/100/0/300d/21d" target=_blank>1d/60d/100/0/300d/21d</a></font></nobr><br><table background="/arimaa/graphics/BoardWithPiecesIcon.jpg" width=50 height=50 border=0 cellspacing=0 cellpadding=0>
  <tr>
    <td >
<a href="javascript:openGame('539426','v','w');" title="Fri 1:08 am">
  <img src="/arimaa/graphics/animals/blank.gif" width=25 height=49 border=0>
</a>
    </td>
    <td >
<!-- a href="javascript:openGame('539426','v','b');" target="watch539426" -->
<a href="javascript:openGame('539426','v','b');" >
  <img src="/arimaa/graphics/animals/blank.gif" width=25 height=49 border=0>
</a>
    </td>
  </tr>
</table>

  </td>
  <td align=left>
    <a href="javascript:playerPage(4609)">silver_player</a>
    <br><font size=-1>US &nbsp; 1000</font>
  </td>
</tr>
<tr>"#;

    #[test]
    fn postal_games_are_read() {
        let games = parse_postal_games(POSTAL);
        assert_eq!(
            games,
            [PostalGame {
                gid: "539426".into(),
                players: ["gold_player".into(), "silver_player".into()],
                ratings: [Some("1206".into()), Some("1000".into())],
                time_control: Some("1d/60d/100/0/300d/21d".into()),
                rated: true,
            }]
        );
    }

    /// `searchPlayers.cgi` for "Janzert", trimmed (2026-10-06).
    const SEARCH: &str = r#"<h2 align=center>Results</h2>
<form method=POST>
  Enter all or part of username or real name: <input size=20 name=any> (substring match on username and fullname)
</form>
 <p align=center>Results for <font color=green>Janzert</font> <p><center><table><tr><td><a href='playerpage.cgi?id=20871' target=_blank>bot_monitor</a></td> <td><font color=green>Janzert</font> Brian Haskin</td> <td></td></tr>
<tr><td><a href='playerpage.cgi?id=397' target=_blank><font color=green>Janzert</font></a></td> <td>Brian Haskin</td> <td></td></tr>
<tr><td><a href='playerpage.cgi?id=21487' target=_blank><font color=green>Janzert</font>_test</a></td> <td></td> <td></td></tr>
</table></center>"#;

    /// `pastgames.cgi?id=397`, trimmed to the header's end, two rows and
    /// the footer (2026-10-06). The first row is indented, the others not.
    const PAST: &str = r#"    <td align=left>
      <a href="?id=397&s=ct">Comments</a>
    </td>
  </tr>
  <tr>
  <td align=right>
     <a href="javascript:playerPage(397)"><b>Janzert</b></a>
<br>1677 &nbsp; US
  </td>
  <td align=center>
    <font size=1> 15s/2m/100/0/4h/2m</font><br>
<table background="/arimaa/graphics/BoardWithPiecesIcon2.jpg" width=50 height=50 border=0 cellspacing=0 cellpadding=0>
  <tr>
    <td >
<a href="/arimaa/games/jsShowGame.cgi?gid=287937&s=w" target=_blank >
  <img src="/arimaa/graphics/animals/blank.gif" width=25 height=49 border=0>
</a>
    </td>
    <td >
<a href="/arimaa/games/jsShowGame.cgi?gid=287937&s=b" target=_blank >
  <img src="/arimaa/graphics/animals/blank.gif" width=25 height=49 border=0>
</a>
    </td>
  </tr>
</table>
  </td>
  <td align=left>
    <a href="javascript:playerPage(21487)">Janzert_test</a> *
<br>US &nbsp; 1400
  </td>
  <td align=right>
    &nbsp; &nbsp;
  </td>
  <td align=center>
    t
  </td>
  <td align=right>
    4
  </td>
  <td align=right>
    &nbsp; &nbsp;
  </td>
  <td align=left>
    Jan 4, 2014 10:38 am YLT
  </td>
  <td align=right>
    &nbsp; &nbsp;
  </td>
  <td align=left>
    <a href="comments.cgi?gid=287937" target=_blank>add</a>
  </td>
</tr>
<tr>
  <td align=right>
    * <a href="javascript:playerPage(7653)">bot_OpFor</a>
<br>1825 &nbsp; US
  </td>
  <td align=center>
    <font size=1><b>R</b> 15s/1/100/2/2</font><br>
<table background="/arimaa/graphics/BoardWithPiecesIcon2.jpg" width=50 height=50 border=0 cellspacing=0 cellpadding=0>
  <tr>
    <td >
<a href="/arimaa/games/jsShowGame.cgi?gid=120092&s=w" target=_blank >
  <img src="/arimaa/graphics/animals/blank.gif" width=25 height=49 border=0>
</a>
    </td>
  </tr>
</table>
  </td>
  <td align=left>
    <a href="javascript:playerPage(397)"><b>Janzert</b></a>
<br>US &nbsp; 1677
  </td>
  <td align=right>
    &nbsp; &nbsp;
  </td>
  <td align=center>
    e
  </td>
  <td align=right>
    10
  </td>
  <td align=right>
    &nbsp; &nbsp;
  </td>
  <td align=left>
    Oct 8, 2009 3:56 am YLT
  </td>
  <td align=left>
    <a href="comments.cgi?gid=120092" target=_blank>add</a>
  </td>
</tr>

</table>
  <p>
        <a href="?id=397&off=50">Previous</a>
</center>"#;

    #[test]
    fn search_results_are_read() {
        let found = parse_search(SEARCH);
        let summary: Vec<(&str, &str, Option<&str>)> =
            found.iter().map(|p| (p.id.as_str(), p.username.as_str(), p.name.as_deref())).collect();
        assert_eq!(
            summary,
            [
                ("20871", "bot_monitor", Some("Janzert Brian Haskin")),
                ("397", "Janzert", Some("Brian Haskin")),
                ("21487", "Janzert_test", None),
            ]
        );
        assert!(parse_search("<h2>Results</h2> no rows").is_empty());
    }

    #[test]
    fn past_games_are_read() {
        let page = parse_past_games(PAST, 0);
        assert_eq!(page.next, Some(50));
        assert_eq!(
            page.games,
            [
                PastGame {
                    id: "287937".into(),
                    players: ["Janzert".into(), "Janzert_test".into()],
                    ratings: [Some("1677".into()), Some("1400".into())],
                    time_control: Some("15s/2m/100/0/4h/2m".into()),
                    rated: false,
                    winner: Some(Color::Silver),
                    reason: Some("t".into()),
                    moves: Some(4),
                    finished: Some("Jan 4, 2014 10:38 am".into()),
                },
                PastGame {
                    id: "120092".into(),
                    players: ["bot_OpFor".into(), "Janzert".into()],
                    ratings: [Some("1825".into()), Some("1677".into())],
                    time_control: Some("15s/1/100/2/2".into()),
                    rated: true,
                    winner: Some(Color::Gold),
                    reason: Some("e".into()),
                    moves: Some(10),
                    finished: Some("Oct 8, 2009 3:56 am".into()),
                },
            ]
        );
        assert_eq!(parse_past_games(PAST, 50).next, None, "only older pages are next");
        assert_eq!(parse_past_games("<html>nothing</html>", 0), PastGames::default());
    }
}
