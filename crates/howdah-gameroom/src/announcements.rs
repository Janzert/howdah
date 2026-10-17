//! The gameroom's announcements: `state`'s `announce` list (seen
//! 2026-10-17), each `{modified, message}`. `modified` is when it was
//! last edited (Unix seconds, the server's clock), so some are years old
//! and an old one comes back as new when it's edited. `message` is HTML
//! written for the browser lobby; it's read here into plain styled text
//! (paragraphs, list items, bold, italic and links), so that no server
//! HTML reaches the app's pages.

use crate::bots::resolve;
use crate::wire::Record;

/// Where the announcements' relative links point from.
const LINK_BASE: &str = "http://arimaa.com/arimaa/gameroom/";

/// One announcement.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Announcement {
    /// When it was last edited, in Unix seconds (the server's clock).
    pub modified: Option<i64>,
    pub blocks: Vec<Block>,
}

/// A paragraph or a list item.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Block {
    pub item: bool,
    pub spans: Vec<Span>,
}

/// A run of text in one style. Line breaks are `\n`.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Span {
    pub text: String,
    pub bold: bool,
    pub italic: bool,
    /// An absolute http(s) URL; links to anything else are left as text.
    pub link: Option<String>,
}

impl Announcement {
    /// An entry of `state`'s `announce` list; none without any text.
    pub fn from_record(r: &Record) -> Option<Announcement> {
        let blocks = parse_message(&r.str("message")?);
        (!blocks.is_empty()).then(|| Announcement { modified: r.int("modified"), blocks })
    }
}

/// The text of an announcement's HTML, as blocks of styled spans. Comments,
/// scripts, styles and unknown tags' markup are dropped (their text is
/// kept).
pub fn parse_message(html: &str) -> Vec<Block> {
    let mut p = Parser::default();
    let mut rest = html;
    while !rest.is_empty() {
        if let Some(after) = rest.strip_prefix("<!--") {
            rest = after.find("-->").map_or("", |i| &after[i + 3..]);
        } else if let Some(tag) =
            rest.strip_prefix('<').filter(|t| t.starts_with(|c: char| c.is_ascii_alphabetic() || c == '/'))
        {
            let end = tag.find('>').unwrap_or(tag.len());
            let skip = p.tag(&tag[..end]);
            rest = tag.get(end + 1..).unwrap_or("");
            if let Some(name) = skip {
                // The contents of a script or style aren't text.
                let close = format!("</{name}");
                rest = find_ignore_case(rest, &close)
                    .map_or("", |i| rest[i..].find('>').map_or("", |j| &rest[i + j + 1..]));
            }
        } else {
            let end = rest[1..].find('<').map_or(rest.len(), |i| i + 1);
            p.text(&decode_entities(&rest[..end]));
            rest = &rest[end..];
        }
    }
    p.end_block();
    p.blocks
}

#[derive(Default)]
struct Parser {
    blocks: Vec<Block>,
    current: Block,
    bold: u32,
    italic: u32,
    /// Open links; `None` for one whose target isn't a web page.
    links: Vec<Option<String>>,
}

impl Parser {
    /// Takes a tag (between `<` and `>`). Returns the name of an element
    /// whose contents are to be skipped.
    fn tag(&mut self, tag: &str) -> Option<&'static str> {
        let (closing, tag) = match tag.strip_prefix('/') {
            Some(t) => (true, t),
            None => (false, tag),
        };
        let name_end = tag.find(|c: char| c.is_whitespace() || c == '/').unwrap_or(tag.len());
        let name = tag[..name_end].to_ascii_lowercase();
        let count = |n: &mut u32| if closing { *n = n.saturating_sub(1) } else { *n += 1 };
        match name.as_str() {
            "script" | "style" if !closing => return Some(if name == "script" { "script" } else { "style" }),
            "br" => self.text_raw("\n"),
            "b" | "strong" => count(&mut self.bold),
            "i" | "em" => count(&mut self.italic),
            "a" if closing => {
                self.links.pop();
            }
            "a" => {
                let link =
                    attribute(&tag[name_end..], "href").and_then(|h| web_link(decode_entities(&h).trim()));
                self.links.push(link);
            }
            "li" => {
                self.end_block();
                self.current.item = !closing;
            }
            "h1" | "h2" | "h3" | "h4" | "h5" | "h6" => {
                self.end_block();
                count(&mut self.bold);
            }
            "p" | "div" | "center" | "ul" | "ol" | "table" | "tr" | "blockquote" | "hr" => self.end_block(),
            _ => {}
        }
        None
    }

    /// Adds HTML text: whitespace runs read as one space.
    fn text(&mut self, text: &str) {
        let mut collapsed = String::with_capacity(text.len());
        for c in text.chars() {
            if c.is_whitespace() && c != '\u{a0}' {
                if !collapsed.ends_with(' ') {
                    collapsed.push(' ');
                }
            } else {
                collapsed.push(c);
            }
        }
        self.text_raw(&collapsed);
    }

    fn text_raw(&mut self, text: &str) {
        let style = Span {
            text: String::new(),
            bold: self.bold > 0,
            italic: self.italic > 0,
            link: self.links.iter().rev().find_map(|l| l.clone()),
        };
        let spans = &mut self.current.spans;
        match spans.last_mut() {
            Some(last) if Span { text: String::new(), ..last.clone() } == style => last.text.push_str(text),
            _ => spans.push(Span { text: text.to_string(), ..style }),
        }
    }

    /// Ends the block being written: spaces at its ends and around line
    /// breaks go, and so do line breaks at its ends and empty spans.
    fn end_block(&mut self) {
        let mut block = std::mem::take(&mut self.current);
        for span in &mut block.spans {
            while let Some(i) = span.text.find(" \n").or_else(|| span.text.find("\n ")) {
                span.text.replace_range(i..i + 2, "\n");
            }
        }
        let is_blank = |s: &Span| s.text.trim_matches([' ', '\n']).is_empty();
        while block.spans.first().is_some_and(is_blank) {
            block.spans.remove(0);
        }
        while block.spans.last().is_some_and(is_blank) {
            block.spans.pop();
        }
        if let Some(first) = block.spans.first_mut() {
            first.text = first.text.trim_start_matches([' ', '\n']).to_string();
        }
        if let Some(last) = block.spans.last_mut() {
            last.text = last.text.trim_end_matches([' ', '\n']).to_string();
        }
        if !block.spans.is_empty() {
            self.blocks.push(block);
        }
        // A list item's text after its end (no `</li>`) is a new item.
        self.current.item = false;
    }
}

/// A link's absolute URL, if it's a web page: http(s), or relative to the
/// gameroom. Other schemes (`javascript:`, `mailto:`) give none.
fn web_link(href: &str) -> Option<String> {
    let scheme_end = href.find(':').filter(|&i| !href[..i].contains(['/', '?', '#']));
    match scheme_end.map(|i| href[..i].to_ascii_lowercase()) {
        Some(s) if s == "http" || s == "https" => Some(href.to_string()),
        Some(_) => None,
        None if href.is_empty() || href.starts_with("//") => None,
        None => Some(resolve(LINK_BASE, href)),
    }
}

/// An attribute's value in a tag's attributes (quoted or not).
fn attribute(attrs: &str, name: &str) -> Option<String> {
    let mut rest = attrs;
    loop {
        rest = rest.trim_start_matches(|c: char| c.is_whitespace() || c == '/');
        if rest.is_empty() {
            return None;
        }
        let key_end = rest.find(|c: char| c.is_whitespace() || c == '=' || c == '/').unwrap_or(rest.len());
        let key = &rest[..key_end];
        rest = rest[key_end..].trim_start();
        let value = if let Some(v) = rest.strip_prefix('=') {
            let v = v.trim_start();
            let (value, after) = match v.chars().next() {
                Some(q @ ('"' | '\'')) => {
                    let inner = &v[1..];
                    let end = inner.find(q).unwrap_or(inner.len());
                    (&inner[..end], inner.get(end + 1..).unwrap_or(""))
                }
                _ => {
                    let end = v.find(char::is_whitespace).unwrap_or(v.len());
                    (&v[..end], &v[end..])
                }
            };
            rest = after;
            Some(value)
        } else {
            None
        };
        if key.eq_ignore_ascii_case(name) {
            return value.map(str::to_string);
        }
    }
}

/// HTML's character references: the common named ones and numeric ones.
/// Others are left as they are.
fn decode_entities(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let mut rest = text;
    while let Some(i) = rest.find('&') {
        out.push_str(&rest[..i]);
        rest = &rest[i..];
        let decoded = rest.find(';').filter(|&j| j <= 10).and_then(|j| {
            let name = &rest[1..j];
            let c = match name {
                "amp" => Some('&'),
                "lt" => Some('<'),
                "gt" => Some('>'),
                "quot" => Some('"'),
                "apos" => Some('\''),
                "nbsp" => Some('\u{a0}'),
                _ => name.strip_prefix('#').and_then(|n| {
                    let code = match n.strip_prefix(['x', 'X']) {
                        Some(hex) => u32::from_str_radix(hex, 16).ok(),
                        None => n.parse().ok(),
                    };
                    code.and_then(char::from_u32)
                }),
            };
            c.map(|c| (c, j))
        });
        match decoded {
            Some((c, j)) => {
                out.push(c);
                rest = &rest[j + 1..];
            }
            None => {
                out.push('&');
                rest = &rest[1..];
            }
        }
    }
    out.push_str(rest);
    out
}

fn find_ignore_case(haystack: &str, needle: &str) -> Option<usize> {
    haystack.to_ascii_lowercase().find(&needle.to_ascii_lowercase())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn span(text: &str) -> Span {
        Span { text: text.into(), ..Span::default() }
    }

    fn texts(blocks: &[Block]) -> Vec<String> {
        blocks.iter().map(|b| b.spans.iter().map(|s| s.text.as_str()).collect()).collect()
    }

    #[test]
    fn reads_a_seen_announcement() {
        // As seen 2026-10-17, shortened.
        let html = "<center>\n<b>Endless Endgame Event</b><br /><br />\n\n<!--\n<p>Registration is now \
            open.</p>\n-->\n\n<p>The pairings can be found in the <a href='/arimaa/forum/cgi/YaBB.cgi?\
            action=display;board=events' target=_blank>EEE forum thread</a>. <!--This round ends \
            soon. -->For information, see <a href='http://arimaa.com/arimaa/mwiki/' target=_blank>its \
            wiki page</a>.</p>\n\n</center>";
        let blocks = parse_message(html);
        assert_eq!(
            texts(&blocks),
            [
                "Endless Endgame Event",
                "The pairings can be found in the EEE forum thread. For information, see its wiki page."
            ]
        );
        assert_eq!(blocks[0].spans, [Span { bold: true, ..span("Endless Endgame Event") }]);
        let link = &blocks[1].spans[1];
        assert_eq!(link.text, "EEE forum thread");
        assert_eq!(
            link.link.as_deref(),
            Some("http://arimaa.com/arimaa/forum/cgi/YaBB.cgi?action=display;board=events")
        );
        assert_eq!(blocks[1].spans[3].link.as_deref(), Some("http://arimaa.com/arimaa/mwiki/"));
    }

    #[test]
    fn keeps_line_breaks_and_list_items() {
        let blocks = parse_message("<p>One<br>two <br/> three</p><ul><li>a<li><i>b</i></ul>after");
        assert_eq!(texts(&blocks), ["One\ntwo\nthree", "a", "b", "after"]);
        assert_eq!(blocks.iter().map(|b| b.item).collect::<Vec<_>>(), [false, true, true, false]);
        assert!(blocks[2].spans[0].italic);
    }

    #[test]
    fn drops_scripts_and_links_that_are_not_web_pages() {
        let blocks = parse_message(
            "Hi<script>alert('x')</script> <a href=\"javascript:alert(1)\">there</a> \
             <a href=\"https://discord.gg/x\">&amp; here</a><style>b{}</style>",
        );
        assert_eq!(texts(&blocks), ["Hi there & here"]);
        assert_eq!(blocks[0].spans[0], span("Hi there "));
        assert_eq!(blocks[0].spans[1].link.as_deref(), Some("https://discord.gg/x"));
    }

    #[test]
    fn decodes_entities() {
        assert_eq!(
            decode_entities("a &lt;b&gt; &#39;c&#x27; &bogus; & d&nbsp;e"),
            "a <b> 'c' &bogus; & d\u{a0}e"
        );
    }

    #[test]
    fn reads_the_list_entries() {
        let r =
            Record::decode(r#"{"modified":1780251348,"message":"<center>\nWelcome!\n</center>"}"#).unwrap();
        let a = Announcement::from_record(&r).unwrap();
        assert_eq!(a.modified, Some(1780251348));
        assert_eq!(texts(&a.blocks), ["Welcome!"]);
        let empty = Record::decode(r#"{"modified":1,"message":"<center><!-- gone --></center>"}"#).unwrap();
        assert_eq!(Announcement::from_record(&empty), None);
    }
}
