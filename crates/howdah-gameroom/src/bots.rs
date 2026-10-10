//! The gameroom's server bots: bots that arimaa.com runs and that anyone
//! can start a game against, as 4steps's bot launcher does.
//!
//! The list is the bot ladder's page of every bot (`botLadderAll.cgi`,
//! with `?u=<username>` for the user's record against each; no session
//! needed). Each bot has a control page (`…/arimaa/bots/<bot>/index.cgi`)
//! saying what it is, whether it runs, and the time control of its games.
//! Posting that page's player form (`action=player`, `side`, the side the
//! bot plays, and `newgame=Start Bot`) starts the bot, which opens a game
//! in the gameroom with itself at that side; the user then sits at the
//! other one like at any open game.
//!
//! These are HTML pages made for people, read leniently.

/// A bot on the server's list.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ServerBot {
    /// Its username (`bot_…`).
    pub name: String,
    /// Its player id.
    pub player_id: Option<String>,
    pub rating: Option<u32>,
    /// The rating's uncertainty (30 the most accurate, 120 the least).
    pub rating_uncertainty: Option<u32>,
    /// Its control page, the one [`crate::Lobby::start_bot`] posts to.
    pub page: String,
    /// Whether the user has yet to beat it (the ladder's "To be won").
    pub to_be_won: bool,
    /// The user's games against it, and how many they won and lost; none
    /// when the page wasn't asked for a user, or they haven't played it.
    pub games: Option<u32>,
    pub won: Option<u32>,
    pub lost: Option<u32>,
}

/// What a bot's control page says.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct BotInfo {
    /// What the bot is, in its developer's words.
    pub about: Option<String>,
    /// Whether it runs now, and how many may ("Bot not currently running.
    /// Max bots allowed is: 10").
    pub status: Option<String>,
    /// The time control of the games it opens.
    pub time_control: Option<String>,
    /// Whether they're rated, when the page says.
    pub rated: Option<bool>,
    /// Whether players may start it (its page has the player's form).
    pub can_start: bool,
}

/// The bots on the bot ladder's page. `list_url` is the page's own URL,
/// which relative links are taken from.
pub fn parse_bot_list(page: &str, list_url: &str) -> Vec<ServerBot> {
    let mut rows = table_rows(page).into_iter();
    // The header row names the columns.
    let Some(header) = rows.by_ref().find(|r| r.iter().any(|c| text_of(c).eq_ignore_ascii_case("Bot_name")))
    else {
        return Vec::new();
    };
    let column = |name: &str| header.iter().position(|c| text_of(c).eq_ignore_ascii_case(name));
    let (Some(name_col), Some(link_col)) = (column("Bot_name"), column("Bot_Link")) else {
        return Vec::new();
    };
    let [rating, uncertainty, to_be_won, games, won, lost] =
        ["Bot_rating", "Bot_RU", "To_be_won", "Total_games", "Won", "Lost"].map(column);
    rows.filter_map(|row| {
        let cell = |i: Option<usize>| i.and_then(|i| row.get(i)).map(|c| text_of(c));
        let number = |i: Option<usize>| cell(i).and_then(|t| t.parse().ok());
        let name_cell = row.get(name_col)?;
        let name = link_text(name_cell)?;
        let player_id = between(name_cell, "playerPage(", ")").map(str::to_string);
        let page = resolve(list_url, between(row.get(link_col)?, "href=\"", "\"")?);
        Some(ServerBot {
            name,
            player_id,
            rating: number(rating),
            rating_uncertainty: number(uncertainty),
            page,
            to_be_won: cell(to_be_won).is_some_and(|t| t.eq_ignore_ascii_case("yes")),
            games: number(games),
            won: number(won),
            lost: number(lost),
        })
    })
    .collect()
}

/// A bot's control page. Only its player section counts: the developer's
/// section below it has a form to start the bot too, behind its password.
pub fn parse_bot_page(page: &str) -> BotInfo {
    let player = section(page, "Player's Control Section", "Developer's Control Section").unwrap_or("");
    let offer = between(player, "Time control:", ")").map(text_of);
    let (time_control, rated) = match &offer {
        Some(offer) => {
            let tc = offer.split(['(', ' ']).find(|s| !s.is_empty()).map(|s| s.trim_end_matches('.'));
            let rated = offer.split_once("rated:").map(|(_, r)| r.trim().eq_ignore_ascii_case("yes"));
            (tc.filter(|t| !t.is_empty()).map(str::to_string), rated)
        }
        None => (None, None),
    };
    let blockquote = |heading: &str| {
        section(page, heading, "</blockquote>")
            .and_then(|s| s.find("<blockquote>").map(|i| &s[i..]))
            .map(|s| text_of(s.split("Bot Logs").next().unwrap_or(s)))
            .filter(|t| !t.is_empty())
    };
    BotInfo {
        about: blockquote("Bot Info"),
        status: blockquote("Bot Status"),
        time_control,
        rated,
        can_start: player.contains("name=newgame"),
    }
}

/// The text of the page that answers starting a bot.
pub fn start_reply_text(page: &str) -> String {
    let body = page.find("<body").map_or(page, |i| &page[i..]);
    text_of(body)
}

/// Each `<tr>`'s cells (the HTML inside each `<td>`).
fn table_rows(page: &str) -> Vec<Vec<&str>> {
    let lower = page.to_ascii_lowercase();
    let mut rows = Vec::new();
    let mut at = 0;
    while let Some(start) = lower[at..].find("<tr").map(|i| at + i) {
        let end = lower[start + 3..].find("<tr").map_or(lower.len(), |i| start + 3 + i);
        let mut cells = Vec::new();
        let mut c = start;
        while let Some(open) = lower[c..end].find("<td").map(|i| c + i) {
            let Some(body) = lower[open..end].find('>').map(|i| open + i + 1) else { break };
            let close = lower[body..end].find("</td").map_or(end, |i| body + i);
            cells.push(&page[body..close]);
            c = close;
        }
        rows.push(cells);
        at = end;
    }
    rows
}

/// The text of the first link in `cell`.
fn link_text(cell: &str) -> Option<String> {
    let a = cell.find("<a ")?;
    let start = a + cell[a..].find('>')? + 1;
    let end = start + cell[start..].find("</a>")?;
    Some(text_of(&cell[start..end])).filter(|t| !t.is_empty())
}

/// `link` made absolute against the page at `base`.
fn resolve(base: &str, link: &str) -> String {
    if link.starts_with("http://") || link.starts_with("https://") {
        return link.to_string();
    }
    let origin_end = base.find("://").and_then(|i| base[i + 3..].find('/').map(|j| i + 3 + j));
    match (link.strip_prefix('/'), origin_end) {
        (Some(_), Some(o)) => format!("{}{link}", &base[..o]),
        _ => {
            let dir = base.split(['?', '#']).next().unwrap_or(base);
            format!("{}{link}", &dir[..dir.rfind('/').map_or(0, |i| i + 1)])
        }
    }
}

/// The text from `start` up to `end`.
fn between<'a>(text: &'a str, start: &str, end: &str) -> Option<&'a str> {
    let rest = &text[text.find(start)? + start.len()..];
    Some(&rest[..rest.find(end)?])
}

/// The page from `start` up to `end`, or to the end of the page.
fn section<'a>(page: &'a str, start: &str, end: &str) -> Option<&'a str> {
    let rest = &page[page.find(start)? + start.len()..];
    Some(&rest[..rest.find(end).unwrap_or(rest.len())])
}

/// HTML's text, tags read as spaces, `&nbsp;` and `&amp;` read, and
/// whitespace collapsed.
fn text_of(html: &str) -> String {
    let mut text = String::new();
    let mut in_tag = false;
    for c in html.chars() {
        match c {
            '<' => in_tag = true,
            '>' => {
                in_tag = false;
                text.push(' ');
            }
            c if !in_tag => text.push(c),
            _ => {}
        }
    }
    text.replace("&nbsp;", " ").replace("&amp;", "&").split_whitespace().collect::<Vec<_>>().join(" ")
}

#[cfg(test)]
mod tests {
    use super::*;

    const LIST_URL: &str = "http://arimaa.com/arimaa/gameroom/botLadderAll.cgi?u=some_user";

    /// `botLadderAll.cgi?u=…` (2026-10-10), trimmed to the table's header
    /// and two rows, the first given a made-up record.
    const LIST: &str = r#"<p>Bot game statistics for player: <a href="javascript:playerPage(21487)"> some_user</a>
<p><p>
<table border=0 >
  <tr>
    <td align=right><b><a href="?u=some_user&s=u">Bot_name </a></b></td>
    <td align=center><b><a href="?u=some_user&s=r">Bot_rating </a></b></td>
    <td align=center><b><a href="?u=some_user&s=k">Bot_RU </a></b></td>
    <td align=center>Bot_Link </td>
    <td align=center>To_be_won </td>
    <td align=center><b><a href="?u=some_user&s=g">Total_games </a></b></td>
    <td align=center>As_Gold </td>
    <td align=center>As_Silver </td>
    <td align=center>Won </td>
    <td align=center>Lost </td>
    <td align=center><b><a href="?u=some_user&s=w">Percent_won </a></b></td>
  </tr>
  <tr>
  <td align=right>* <a href="javascript:playerPage(8775)">bot_Bomb2005Lightning</a> </td>
  <td align=right>2189</td>
  <td align=right>120</td>
  <td align=right>* <a href="http://arimaa.com/arimaa/bots/bot_Bomb2005Lightning/index.cgi">Play</a></td>
  <td align=center> No </td>
  <td align=right>3</td>
  <td align=right>2</td>
  <td align=right>1</td>
  <td align=right>1</td>
  <td align=right>2</td>
  <td align=right>33</td>
</tr>
<tr>
  <td align=right>* <a href="javascript:playerPage(371)">bot_ShallowBlue</a> </td>
  <td align=right>1265</td>
  <td align=right>46</td>
  <td align=right>* <a href="/arimaa/bots/bot_ShallowBlue/index.cgi">Play</a></td>
  <td align=center> Yes </td>
  <td align=right></td>
  <td align=right></td>
  <td align=right></td>
  <td align=right></td>
  <td align=right></td>
  <td align=right></td>
</tr>

</table>

<br>"#;

    #[test]
    fn the_list_is_read() {
        let bots = parse_bot_list(LIST, LIST_URL);
        assert_eq!(
            bots,
            [
                ServerBot {
                    name: "bot_Bomb2005Lightning".into(),
                    player_id: Some("8775".into()),
                    rating: Some(2189),
                    rating_uncertainty: Some(120),
                    page: "http://arimaa.com/arimaa/bots/bot_Bomb2005Lightning/index.cgi".into(),
                    to_be_won: false,
                    games: Some(3),
                    won: Some(1),
                    lost: Some(2),
                },
                ServerBot {
                    name: "bot_ShallowBlue".into(),
                    player_id: Some("371".into()),
                    rating: Some(1265),
                    rating_uncertainty: Some(46),
                    page: "http://arimaa.com/arimaa/bots/bot_ShallowBlue/index.cgi".into(),
                    to_be_won: true,
                    games: None,
                    won: None,
                    lost: None,
                },
            ]
        );
        assert!(parse_bot_list("<html>Not Found</html>", LIST_URL).is_empty());
    }

    #[test]
    fn links_are_resolved() {
        let base = "http://arimaa.com/arimaa/gameroom/botLadderAll.cgi?u=x/y";
        assert_eq!(resolve(base, "/arimaa/bots/a/index.cgi"), "http://arimaa.com/arimaa/bots/a/index.cgi");
        assert_eq!(
            resolve(base, "../bots/a/index.cgi"),
            "http://arimaa.com/arimaa/gameroom/../bots/a/index.cgi"
        );
        assert_eq!(resolve(base, "https://x.org/b"), "https://x.org/b");
    }

    /// `bots/bot_ShallowBlue/index.cgi` (2026-10-10), the developer's
    /// section cut short.
    const PAGE: &str = r#"<html>
  <head>
    <title>bot_shallowBlue Control Page</title>
  </head>
  <body>
<center>
    <h3>bot_shallowBlue Control Page</h3>
</center>

<p><b>Bot Info</b>
<blockquote>
<p>
Developed by Don Dailey and orignally called bot_Occam. This
version searches 4 steps deep (1 ply).

</blockquote>

<p><b>Bot Status</b> <font size=-1><a href="">Refresh</a></font>
<blockquote>
<p>Bot not currently running.

<br>Max bots allowed is: 10
<p>
<a href="logs">Bot Logs</a>
</blockquote>

<p><b>Player's Control Section</b>
<p>
<blockquote>
<form method=POST>
<input type=hidden name=action value=player>
* Open a new game. Time control: 2/2/100/5/0 (rated: yes).
<br>Bot plays
<select name=side>
<option value=w>Gold</option>
<option value=b>Silver</option>
</select>
<input type=submit name=newgame value="Start Bot">

<p>
* Players cannot have the bot join games. Please check again later or
contact the developer.

</form>
</blockquote>


<p><b>Developer's Control Section</b>
<blockquote>
<form method=POST>
<p>
Open a new game. Time control: 9/9/100/5/0 (rated: no).
<input type=submit name=newgame value="Start Bot">
</form>
</blockquote>
  </body>
</html>"#;

    #[test]
    fn a_bot_page_is_read() {
        assert_eq!(
            parse_bot_page(PAGE),
            BotInfo {
                about: Some(
                    "Developed by Don Dailey and orignally called bot_Occam. This version searches 4 steps \
                     deep (1 ply)."
                        .into()
                ),
                status: Some("Bot not currently running. Max bots allowed is: 10".into()),
                time_control: Some("2/2/100/5/0".into()),
                rated: Some(true),
                can_start: true,
            }
        );
        // Without the player's form, only the developer can start it.
        let closed = PAGE.replace("<input type=submit name=newgame value=\"Start Bot\">\n\n<p>\n*", "<p>*");
        let info = parse_bot_page(&closed);
        assert!(!info.can_start);
        assert_eq!(info.time_control.as_deref(), Some("2/2/100/5/0"));
        assert_eq!(parse_bot_page("<html>Not Found</html>"), BotInfo::default());
    }

    #[test]
    fn the_start_reply_is_text() {
        assert_eq!(
            start_reply_text(
                "<html><head><title>t</title></head><body><p>Bot started.<br>Game  opened</body>"
            ),
            "Bot started. Game opened"
        );
    }
}
