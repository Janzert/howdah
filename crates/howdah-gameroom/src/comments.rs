//! The comments people leave on a finished game (`comments.cgi?gid=<id>`).
//!
//! The page a browser opens is a frame around `comments.cgi?gid=<id>&view=comm`,
//! which lists the comments oldest first: each a table with the text in
//! one cell and, in the next, the writer (a `playerPageU('<name>')` link)
//! and when it was written. Before them, in an HTML comment, is the game
//! server's log of the game, which isn't read. A game's chat isn't among
//! the comments. Neither page needs a session; with one the times are in
//! the login's zone ("YLT"), otherwise GMT.
//!
//! The past games pages say how many comments a game has
//! ([`crate::PastGame::comments`]).

use crate::announcements::decode_entities;

/// A comment on a finished game.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct GameComment {
    /// The writer's username.
    pub author: String,
    /// When it was written, as the page writes it but without the zone's
    /// label ("Jan 6, 2009 5:47 am").
    pub written: Option<String>,
    /// The text, with the lines it was written in.
    pub text: String,
}

/// The comments on a game's comments frame, oldest first. Read leniently:
/// an entry that doesn't parse is skipped.
pub fn parse_comments(page: &str) -> Vec<GameComment> {
    const AUTHOR: &str = "playerPageU('";
    // Past the server's log, which could hold anything.
    let mut rest = match (page.find("<!--"), page.find("-->")) {
        (Some(open), Some(close)) if open < close => &page[close + 3..],
        _ => page,
    };
    let mut comments = Vec::new();
    while let Some(i) = rest.find(AUTHOR) {
        let (before, link) = (&rest[..i], &rest[i + AUTHOR.len()..]);
        let Some(end) = link.find("</td>") else { break };
        if let Some(comment) = comment(before, &link[..end]) {
            comments.push(comment);
        }
        rest = &link[end..];
    }
    comments
}

/// A comment from the page up to its writer's link, and that link's cell
/// from the name on.
fn comment(before: &str, link: &str) -> Option<GameComment> {
    let author = link[..link.find('\'')?].trim().to_string();
    let written = link.split_once("</a>").map(|(_, w)| lines_of(w)).filter(|w| !w.is_empty());
    let written = written.map(|w| {
        let w = w.strip_suffix("GMT").or_else(|| w.strip_suffix("YLT")).unwrap_or(&w);
        w.trim_end().to_string()
    });
    // The text's cell is the first plain one in the entry's colored cell,
    // and ends where the writer's row starts.
    let entry = &before[before.rfind("<td bgcolor").unwrap_or(0)..];
    let body = &entry[entry.find("<td>")? + 4..];
    let text = lines_of(&body[..body.rfind("</td>")?]);
    (!author.is_empty()).then_some(GameComment { author, written, text })
}

/// HTML's text: tags dropped, lines kept (`<br>`s, or else the source's
/// own), entities read, spaces
/// collapsed within each line, and no empty lines at either end.
fn lines_of(html: &str) -> String {
    // With `<br>`s the source's own line ends are only spacing.
    let breaks = html.to_ascii_lowercase().contains("<br");
    let plain = |s: &str| if breaks { s.replace(['\n', '\r'], " ") } else { s.to_string() };
    let mut text = String::new();
    let mut rest = html;
    while let Some(open) = rest.find('<') {
        text.push_str(&plain(&rest[..open]));
        let tag = &rest[open..];
        let Some(close) = tag.find('>') else {
            rest = "";
            break;
        };
        if tag[1..close].trim_start().to_ascii_lowercase().starts_with("br") {
            text.push('\n');
        }
        rest = &tag[close + 1..];
    }
    text.push_str(&plain(rest));
    let lines: Vec<String> =
        decode_entities(&text).lines().map(|l| l.split_whitespace().collect::<Vec<_>>().join(" ")).collect();
    let first = lines.iter().position(|l| !l.is_empty()).unwrap_or(lines.len());
    let last = lines.iter().rposition(|l| !l.is_empty()).map_or(first, |i| i + 1);
    lines[first..last].join("\n")
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A game's `comments.cgi?gid=…&view=comm` (2026-11-14), the log cut to
    /// two lines, names and words changed; the third entry is made up.
    const PAGE: &str = r##"<html>
<head>
<script language="Javascript"  type="text/javascript" src="js/windows.js"></script>
</head>
<body xbgcolor="#d8d8f8">
<!--
1231188913 [Mon Jan  5 20:55:13 2009] b player joining
1231191665 [Mon Jan  5 21:41:05 2009] game finished with result w g

-->
<center>
<table border=0 width="100%">
<tr><td bgcolor=#c8c8f8>
<table border=0>
<tr>
  <td>

Nice game. Are you going to climb the whole bot ladder? :-)

  </td>
</tr>
<tr>
  <td>
<a href="javascript:playerPageU('Fritzlein
')">Fritzlein
</a> &nbsp; &nbsp; Jan 6, 2009 5:47 am GMT
  </td>
</tr>
</table>
</td></tr>
<tr><td> </td></tr>
<tr><td bgcolor=#d8f8d8>
<table border=0>
<tr>
  <td>

Maybe, it has been a few years since my last game. :}

  </td>
</tr>
<tr>
  <td>
<a href="javascript:playerPageU('bot_testbot
')">bot_testbot
</a> &nbsp; &nbsp; Jan 6, 2009 8:44 pm GMT
  </td>
</tr>
</table>
</td></tr>
<tr><td> </td></tr>
<tr><td bgcolor=#c8c8f8>
<table border=0>
<tr>
  <td>
Two   lines &amp; a <a href="http://example.com/">link</a>,<br>
then 3 &lt; 4.
  </td>
</tr>
<tr>
  <td>
<a href="javascript:playerPageU('someone')">someone</a> &nbsp; &nbsp; Sat 4:31 pm YLT
  </td>
</tr>
</table>
</td></tr>

</table>
</center>
</body>
</html>"##;

    #[test]
    fn comments_are_read() {
        let comment = |author: &str, written: &str, text: &str| GameComment {
            author: author.into(),
            written: Some(written.into()),
            text: text.into(),
        };
        assert_eq!(
            parse_comments(PAGE),
            [
                comment(
                    "Fritzlein",
                    "Jan 6, 2009 5:47 am",
                    "Nice game. Are you going to climb the whole bot ladder? :-)"
                ),
                comment(
                    "bot_testbot",
                    "Jan 6, 2009 8:44 pm",
                    "Maybe, it has been a few years since my last game. :}"
                ),
                comment("someone", "Sat 4:31 pm", "Two lines & a link,\nthen 3 < 4."),
            ]
        );
    }

    #[test]
    fn a_page_without_comments_has_none() {
        assert!(parse_comments("<html><body><!--\nlog\n-->\n<center><table></table></center>").is_empty());
        assert!(parse_comments("").is_empty());
    }
}
