//! arimaa.com's puzzle pages (`/arimaa/puzzles/`): the list of puzzles
//! (`list.cgi`), each puzzle's page (`show.cgi?p=<id>`, with its hint and
//! author) and its answer (`show.cgi?w=<id>`, viewer variables with the
//! puzzle's position and solution, which `howdah_arimaa::ViewerGame`
//! reads). No login is needed; like the gameroom, the pages answer only
//! with a Referer from the same section, which [`Http`] sends. Read only.

use crate::client::{Error, Http};

/// Where the puzzle pages are.
pub const DEFAULT_PUZZLES: &str = "http://arimaa.com/arimaa/puzzles/";

/// One puzzle in the list.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PuzzleEntry {
    /// The puzzle's id, such as `p4`.
    pub id: String,
    /// Its title in the list, usually the question.
    pub title: String,
}

/// A heading of the list ("1 Move Puzzles", ...) and its puzzles.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PuzzleGroup {
    pub name: String,
    pub puzzles: Vec<PuzzleEntry>,
}

/// What a puzzle's page says beside the board.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct PuzzlePage {
    /// The question, in bold above the board.
    pub question: Option<String>,
    /// The hint the page's Hint link shows (empty when there's none).
    pub hint: String,
    /// "Composed By:" the puzzle's author.
    pub author: Option<String>,
}

/// Whether `id` is a puzzle id as the list uses them: `p` and digits.
pub fn valid_id(id: &str) -> bool {
    id.strip_prefix('p').is_some_and(|n| !n.is_empty() && n.bytes().all(|b| b.is_ascii_digit()))
}

/// Reads the list page: each `<p>` heading followed by a list of
/// `show.cgi?p=<id>` links. Links before any heading go in a group with no
/// name.
pub fn parse_puzzle_list(page: &str) -> Vec<PuzzleGroup> {
    let mut groups: Vec<PuzzleGroup> = Vec::new();
    let mut rest = page;
    loop {
        let next_heading = rest.find("<p>");
        let next_link = rest.find("show.cgi?p=");
        match (next_heading, next_link) {
            (Some(h), l) if l.is_none_or(|l| h < l) => {
                let after = &rest[h + 3..];
                let end = after.find('<').unwrap_or(after.len());
                let name = text_of(&after[..end]);
                rest = &after[end..];
                // A heading is only kept if links follow it.
                if !name.is_empty() {
                    groups.push(PuzzleGroup { name, puzzles: Vec::new() });
                }
            }
            (_, Some(l)) => {
                let after = &rest[l + "show.cgi?p=".len()..];
                let id_end = after.find(|c: char| !c.is_ascii_alphanumeric()).unwrap_or(after.len());
                let id = &after[..id_end];
                let title = after
                    .find('>')
                    .and_then(|s| after[s + 1..].find("</a>").map(|e| text_of(&after[s + 1..s + 1 + e])))
                    .unwrap_or_default();
                if valid_id(id) {
                    if groups.is_empty() {
                        groups.push(PuzzleGroup { name: String::new(), puzzles: Vec::new() });
                    }
                    let group = groups.last_mut().expect("one at least");
                    if !group.puzzles.iter().any(|p| p.id == id) {
                        group.puzzles.push(PuzzleEntry { id: id.to_string(), title });
                    }
                }
                rest = &after[id_end..];
            }
            _ => break,
        }
    }
    groups.retain(|g| !g.puzzles.is_empty());
    groups
}

/// Reads a puzzle's page: the bold question, the hint in its `answer()`
/// script (`alert("...")`), and the author after "Composed By:".
pub fn parse_puzzle_page(page: &str) -> PuzzlePage {
    let hint = page
        .find("function answer()")
        .and_then(|i| page[i..].find("alert(\"").map(|j| &page[i + j + 7..]))
        .and_then(js_string)
        .unwrap_or_default();
    let question = page
        .find("<b>")
        .and_then(|i| page[i + 3..].find("</b>").map(|j| text_of(&page[i + 3..i + 3 + j])))
        .filter(|q| !q.is_empty());
    let author = page.find("Composed By:").and_then(|i| {
        let after = &page[i..];
        let b = after.find("<b>")? + 3;
        let e = after[b..].find("</b>")?;
        Some(text_of(&after[b..b + e])).filter(|a| !a.is_empty())
    });
    PuzzlePage { question, hint, author }
}

/// A JavaScript string's contents up to its closing quote, unescaped.
fn js_string(s: &str) -> Option<String> {
    let mut out = String::new();
    let mut chars = s.chars();
    while let Some(c) = chars.next() {
        match c {
            '"' => return Some(out.trim().to_string()),
            '\\' => match chars.next()? {
                'n' => out.push('\n'),
                't' => out.push('\t'),
                other => out.push(other),
            },
            c => out.push(c),
        }
    }
    None
}

/// HTML's text: tags dropped, the common entities read, spaces collapsed.
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
    let text = text
        .replace("&nbsp;", " ")
        .replace("&lt;", "<")
        .replace("&gt;", ">")
        .replace("&quot;", "\"")
        .replace("&#39;", "'")
        .replace("&amp;", "&");
    text.split_whitespace().collect::<Vec<_>>().join(" ")
}

/// The puzzle pages, read with `http` (which spaces requests and sends a
/// Referer).
#[derive(Clone)]
pub struct Puzzles {
    http: Http,
    base: String,
}

impl Puzzles {
    pub fn new(http: Http, base: &str) -> Puzzles {
        Puzzles { http, base: base.to_string() }
    }

    /// Every puzzle, by the list's headings.
    pub async fn list(&self) -> Result<Vec<PuzzleGroup>, Error> {
        Ok(parse_puzzle_list(&self.http.get_page(&format!("{}list.cgi", self.base), None).await?))
    }

    /// A puzzle's answer: viewer variables with its position, `startmove`
    /// and solution.
    pub async fn answer(&self, id: &str) -> Result<String, Error> {
        self.http.get_page(&self.url(id, 'w')?, None).await
    }

    /// A puzzle's page: question, hint and author.
    pub async fn page(&self, id: &str) -> Result<PuzzlePage, Error> {
        Ok(parse_puzzle_page(&self.http.get_page(&self.url(id, 'p')?, None).await?))
    }

    fn url(&self, id: &str, what: char) -> Result<String, Error> {
        if !valid_id(id) {
            return Err(Error::Server(format!("not a puzzle id: {id:?}")));
        }
        Ok(format!("{}show.cgi?{what}={id}", self.base))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // Made-up pages in the puzzle pages' layout.
    const LIST: &str = "<center><h2>Arimaa Puzzles</h2>\n<p>\nSolving some puzzles can help.\n\
        <p>\n<table><tr><td>\n<p>1 Move Puzzles\n<ul>\n\
        <li><a href='show.cgi?p=p4' target=_blank>Get the rabbit to goal.</a>\n<br>\n\
        <li><a href='show.cgi?p=p12' target=_blank>Silver to move &amp; win</a>\n<br>\n</ul>\n\
        <p>2 Move Puzzles\n<ul>\n<li><a href='show.cgi?p=p7' target=_blank>Gold to play</a>\n</ul>\n\
        <p>More Puzzles<ul><li><a href='show.cgi?p=p23' target=_blank>Who wins?</a>\n</ul>\n\
        </td></tr><tr><td><a href=\"make.cgi\">Create new puzzle</a></td></tr></table>";

    #[test]
    fn reads_the_list() {
        let groups = parse_puzzle_list(LIST);
        let names: Vec<&str> = groups.iter().map(|g| g.name.as_str()).collect();
        assert_eq!(names, ["1 Move Puzzles", "2 Move Puzzles", "More Puzzles"]);
        assert_eq!(
            groups[0].puzzles,
            [
                PuzzleEntry { id: "p4".into(), title: "Get the rabbit to goal.".into() },
                PuzzleEntry { id: "p12".into(), title: "Silver to move & win".into() },
            ]
        );
        assert_eq!(groups[2].puzzles[0].id, "p23");
    }

    #[test]
    fn reads_a_puzzle_page() {
        let page = "<script>\nfunction answer(){\n  alert(\"First the \\\"dog\\\".\");\n}\n</script>\n\
            <td><b>Get the rabbit to goal.\n</b>\n<ul><li>Use the <font color=red><b>&lt;</b></font> button\
            </ul>\n<p>Composed By: <b>Someone</b> (someone\n; player #9\n)";
        let p = parse_puzzle_page(page);
        assert_eq!(p.question.as_deref(), Some("Get the rabbit to goal."));
        assert_eq!(p.hint, "First the \"dog\".");
        assert_eq!(p.author.as_deref(), Some("Someone"));
        let none = parse_puzzle_page("function answer(){\n  alert(\"\");\n}");
        assert_eq!((none.hint.as_str(), none.author), ("", None));
    }

    #[test]
    fn ids() {
        assert!(valid_id("p4") && valid_id("p117"));
        assert!(!valid_id("p") && !valid_id("4") && !valid_id("p4&x=1") && !valid_id("../p4"));
    }
}
