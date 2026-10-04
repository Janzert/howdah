//! HTTP clients for the gameroom lobby and the game server.
//!
//! The server's access rules (see the module docs in `lib.rs`): requests
//! under `/arimaa/gameroom/` need a Referer there, CGIs only answer over
//! plain HTTP, and a refused request is a 404. Requests are spaced out so
//! the client never looks like a scraper; only the long poll waits.

use std::sync::Arc;
use std::time::{Duration, Instant};

use tokio::sync::Mutex;

use crate::state::{GameState, Role};
use crate::wire::{Format, Record, encode_request};

/// The gameroom directory on arimaa.com.
pub const DEFAULT_GAMEROOM: &str = "http://arimaa.com/arimaa/gameroom/";

/// Shortest gap between requests, except the long poll.
const MIN_INTERVAL: Duration = Duration::from_millis(1000);

/// How long an ordinary request may take.
const REQUEST_TIMEOUT: Duration = Duration::from_secs(20);

/// Fields never logged as they are.
const SECRETS: [&str; 4] = ["password", "sid", "auth", "tid"];

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("network error: {0}")]
    Network(String),
    /// The server's answer to requests it won't allow.
    #[error("the server refused the request (HTTP 404)")]
    Refused,
    #[error("HTTP {0}")]
    Status(u16),
    /// The server closed the connection without a reply (a long poll can).
    #[error("empty reply")]
    Empty,
    #[error("{0}")]
    BadReply(String),
    /// The reply carried an `error` field.
    #[error("{0}")]
    Server(String),
    #[error("not logged in")]
    NotLoggedIn,
}

/// The ASIP version, which picks the lobby and the request encoding.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Asip {
    V1,
    V2,
}

impl Asip {
    fn format(self) -> Format {
        match self {
            Asip::V1 => Format::KeyValue,
            Asip::V2 => Format::Json,
        }
    }

    fn lobby(self) -> &'static str {
        match self {
            Asip::V1 => "client1gr.cgi",
            Asip::V2 => "client2gr.cgi",
        }
    }

    fn game_server(self) -> &'static str {
        match self {
            Asip::V1 => "client1gs.cgi",
            Asip::V2 => "client2gs.cgi",
        }
    }
}

/// One request and its reply, with secrets redacted, for logging.
#[derive(Clone, Debug)]
pub struct Exchange {
    pub url: String,
    pub request: Vec<(String, String)>,
    /// HTTP status, or `None` if the request failed before one arrived.
    pub status: Option<u16>,
    pub reply: String,
    pub elapsed: Duration,
}

pub type NetLog = Arc<dyn Fn(&Exchange) + Send + Sync>;

/// Replaces the values of secret `key=value` fields and JSON members
/// (string or number values, at any depth).
pub fn redact(text: &str) -> String {
    let lines: Vec<String> = text
        .split('\n')
        .map(|line| match line.split_once('=') {
            Some((k, _)) if SECRETS.contains(&k) => format!("{k}=<redacted>"),
            _ => line.to_string(),
        })
        .collect();
    let mut out = lines.join("\n");
    for key in SECRETS {
        let pattern = format!("\"{key}\":");
        let mut rest = out.as_str();
        let mut redacted = String::new();
        while let Some(i) = rest.find(&pattern) {
            redacted.push_str(&rest[..i + pattern.len()]);
            rest = rest[i + pattern.len()..].trim_start();
            let end = if let Some(s) = rest.strip_prefix('"') {
                1 + s.find('"').map_or(s.len(), |e| e + 1)
            } else {
                rest.find([',', '}', ']']).unwrap_or(rest.len())
            };
            redacted.push_str("\"<redacted>\"");
            rest = &rest[end..];
        }
        redacted.push_str(rest);
        out = redacted;
    }
    out
}

/// The HTTP side shared by the lobby and game servers.
#[derive(Clone)]
pub struct Http {
    client: reqwest::Client,
    last: Arc<Mutex<Option<Instant>>>,
    log: Option<NetLog>,
}

impl Http {
    pub fn new(user_agent: &str, log: Option<NetLog>) -> Result<Http, Error> {
        let client = reqwest::Client::builder()
            .user_agent(user_agent)
            .build()
            .map_err(|e| Error::Network(e.to_string()))?;
        Ok(Http { client, last: Arc::new(Mutex::new(None)), log })
    }

    /// Posts `params` in `format` and decodes the reply. `wait` is how long
    /// the server may hold the request (a long poll); other requests are
    /// spaced at least [`MIN_INTERVAL`] apart.
    pub async fn post(
        &self,
        url: &str,
        format: Format,
        params: &[(&str, String)],
        wait: Option<Duration>,
    ) -> Result<Record, Error> {
        if wait.is_none() {
            let mut last = self.last.lock().await;
            if let Some(t) = *last {
                tokio::time::sleep_until((t + MIN_INTERVAL).into()).await;
            }
            *last = Some(Instant::now());
        }
        let body = encode_request(format, params);
        let started = Instant::now();
        let timeout = wait.map_or(REQUEST_TIMEOUT, |w| w + REQUEST_TIMEOUT);
        let sent = self
            .client
            .post(url)
            // 4steps sends both formats with this type.
            .header("Content-Type", "application/x-www-form-urlencoded")
            .header("Referer", url)
            .body(body)
            .timeout(timeout)
            .send()
            .await;
        let (status, reply) = match sent {
            Ok(r) => {
                let status = r.status().as_u16();
                (Some(status), r.text().await.map_err(|e| Error::Network(e.to_string())))
            }
            Err(e) => (None, Err(Error::Network(e.to_string()))),
        };
        if let Some(log) = &self.log {
            let request = params
                .iter()
                .map(|(k, v)| {
                    let v = if SECRETS.contains(k) { "<redacted>".into() } else { v.clone() };
                    (k.to_string(), v)
                })
                .collect();
            let text = match &reply {
                Ok(t) => redact(t),
                Err(e) => format!("<{e}>"),
            };
            log(&Exchange { url: url.to_string(), request, status, reply: text, elapsed: started.elapsed() });
        }
        let reply = reply?;
        match status {
            Some(404) => return Err(Error::Refused),
            Some(s) if !(200..300).contains(&s) => return Err(Error::Status(s)),
            _ => {}
        }
        if reply.trim().is_empty() {
            return Err(Error::Empty);
        }
        let record = Record::decode(&reply).map_err(Error::BadReply)?;
        if let Some(e) = record.nonempty("error") {
            // Errors can quote a session id ("… expired [<sid>]").
            let e = params
                .iter()
                .filter(|(k, v)| SECRETS.contains(k) && !v.is_empty())
                .fold(e, |e, (_, v)| e.replace(v.as_str(), "<redacted>"));
            return Err(Error::Server(e));
        }
        Ok(record)
    }
}

/// A game in the lobby's lists.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct GameInfo {
    pub gid: String,
    /// Gold's and silver's usernames, where the list gives them.
    pub players: [Option<String>; 2],
    pub time_control: Option<String>,
    pub rated: bool,
    pub postal: bool,
}

impl GameInfo {
    fn from_record(r: &Record) -> Option<GameInfo> {
        let name = |k: &str| r.nonempty(k);
        Some(GameInfo {
            gid: r.nonempty("id").or_else(|| r.nonempty("gid"))?,
            players: [name("wusername"), name("busername")],
            time_control: r.nonempty("timecontrol"),
            rated: r.flag("rated"),
            postal: r.flag("postal"),
        })
    }
}

/// A reserved seat: what `sit` needs.
#[derive(Clone, Debug)]
pub struct Seat {
    /// The game server URL, made absolute.
    pub gsurl: String,
    pub tid: String,
    pub grid: String,
    /// The format the reservation reply came in.
    pub reply_format: Option<Format>,
}

impl Seat {
    /// The game server URL for `asip`: the server's file name swapped for
    /// that version's (4steps talks to `client2gs.cgi` for ASIP 2.0).
    pub fn server_url(&self, asip: Asip) -> String {
        match self.gsurl.rfind('/') {
            Some(i) => format!("{}{}", &self.gsurl[..=i], asip.game_server()),
            None => self.gsurl.clone(),
        }
    }
}

/// A gameroom (lobby) session.
pub struct Lobby {
    http: Http,
    /// The gameroom directory, ending in `/`.
    base: String,
    asip: Asip,
    sid: Option<String>,
    grid: Option<String>,
}

impl Lobby {
    pub fn new(http: Http, base: &str, asip: Asip) -> Lobby {
        let base = if base.ends_with('/') { base.to_string() } else { format!("{base}/") };
        Lobby { http, base, asip, sid: None, grid: None }
    }

    /// Switches the ASIP version for later requests, keeping the session.
    pub fn set_asip(&mut self, asip: Asip) {
        self.asip = asip;
    }

    pub fn is_logged_in(&self) -> bool {
        self.sid.is_some()
    }

    async fn post(&self, asip: Asip, params: &[(&str, String)]) -> Result<Record, Error> {
        let url = format!("{}{}", self.base, asip.lobby());
        self.http.post(&url, asip.format(), params, None).await
    }

    fn sid(&self) -> Result<String, Error> {
        self.sid.clone().ok_or(Error::NotLoggedIn)
    }

    pub async fn login(&mut self, username: &str, password: &str) -> Result<(), Error> {
        let r = self
            .post(
                self.asip,
                &[("action", "login".into()), ("username", username.into()), ("password", password.into())],
            )
            .await?;
        self.sid =
            Some(r.nonempty("sid").ok_or_else(|| Error::BadReply("no sid in the login reply".into()))?);
        self.grid = r.nonempty("grid");
        Ok(())
    }

    pub async fn logout(&mut self) -> Result<(), Error> {
        let sid = self.sid()?;
        self.sid = None;
        self.post(self.asip, &[("action", "logout".into()), ("sid", sid)]).await.map(drop)
    }

    /// The live games (ASIP 2.0's `state`; ASIP 1.0 has no such list).
    pub async fn live_games(&self) -> Result<Vec<GameInfo>, Error> {
        let r = self.post(Asip::V2, &[("action", "state".into()), ("sid", self.sid()?)]).await?;
        Ok(r.list("livegames").iter().filter_map(GameInfo::from_record).collect())
    }

    /// Reserves a seat at game `gid`: a player's side, or a viewer's.
    pub async fn reserve_seat(&self, gid: &str, role: Role) -> Result<Seat, Error> {
        let r = self
            .post(
                self.asip,
                &[
                    ("action", "reserveseat".into()),
                    ("sid", self.sid()?),
                    ("gid", gid.into()),
                    ("role", role.letter().to_string()),
                ],
            )
            .await?;
        let field =
            |k: &str| r.nonempty(k).ok_or_else(|| Error::BadReply(format!("no {k} in the seat reply")));
        Ok(Seat {
            gsurl: self.resolve(&field("gsurl")?),
            tid: field("tid")?,
            grid: r.nonempty("grid").or_else(|| self.grid.clone()).unwrap_or_default(),
            reply_format: r.format,
        })
    }

    /// Makes a game server URL absolute. ASIP 2.0 can return one relative
    /// to the game server directory (a 4steps workaround).
    fn resolve(&self, gsurl: &str) -> String {
        if gsurl.starts_with("http://") || gsurl.starts_with("https://") {
            return gsurl.to_string();
        }
        let root = self.base.trim_end_matches('/').rsplit_once('/').map_or(self.base.as_str(), |(r, _)| r);
        format!("{root}/java/ys/ms4/v5/{}", gsurl.trim_start_matches('/'))
    }
}

/// A seat at a game on the game server.
pub struct GameServer {
    http: Http,
    url: String,
    format: Format,
    sid: String,
    auth: Option<String>,
    lastchange: String,
    moves: String,
    chat: String,
}

impl GameServer {
    /// Sits down at the reserved seat, talking `asip` to the server.
    pub async fn sit(http: Http, seat: &Seat, asip: Asip) -> Result<GameServer, Error> {
        let url = seat.server_url(asip);
        let format = asip.format();
        let r = http
            .post(
                &url,
                format,
                &[("action", "sit".into()), ("tid", seat.tid.clone()), ("grid", seat.grid.clone())],
                None,
            )
            .await?;
        let sid = r.nonempty("sid").ok_or_else(|| Error::BadReply("no sid in the sit reply".into()))?;
        Ok(GameServer {
            http,
            url,
            format,
            sid,
            auth: None,
            lastchange: "0".into(),
            moves: String::new(),
            chat: String::new(),
        })
    }

    pub fn url(&self) -> &str {
        &self.url
    }

    /// Talks to another game server CGI from now on, in `format`, keeping
    /// the session (to try the browser client's `client3gs.cgi`).
    pub fn switch_to(&mut self, url: &str, format: Format) {
        self.url = url.to_string();
        self.format = format;
    }

    /// The full game state.
    pub async fn game_state(&mut self) -> Result<GameState, Error> {
        let r = self
            .http
            .post(
                &self.url,
                self.format,
                &[("action", "gamestate".into()), ("sid", self.sid.clone()), ("wait", "0".into())],
                None,
            )
            .await?;
        Ok(self.take(r, false))
    }

    /// Waits up to `maxwait` for a change (the long poll) and returns the
    /// state with it. If the server's move or chat lengths don't match
    /// what's been received (a takeback shortens them), it fetches the
    /// full state instead.
    pub async fn update(&mut self, maxwait: Duration) -> Result<GameState, Error> {
        let r = self
            .http
            .post(
                &self.url,
                self.format,
                &[
                    ("action", "updategamestate".into()),
                    ("sid", self.sid.clone()),
                    ("wait", "1".into()),
                    ("lastchange", self.lastchange.clone()),
                    ("moveslength", self.moves.len().to_string()),
                    ("chatlength", self.chat.len().to_string()),
                    ("maxwait", maxwait.as_secs().to_string()),
                ],
                Some(maxwait),
            )
            .await?;
        let lengths_match = |r: &Record, moves: usize, chat: usize| {
            let added = |k: &str| added(r, k).len();
            let fits = |k: &str, have: usize, add: usize| r.int(k).is_none_or(|n| n as usize == have + add);
            fits("moveslength", moves, added("moves")) && fits("chatlength", chat, added("chat"))
        };
        if !lengths_match(&r, self.moves.len(), self.chat.len()) {
            return self.game_state().await;
        }
        Ok(self.take(r, true))
    }

    /// Keeps what later requests need and builds the state, adding a
    /// partial reply's moves and chat to what came before.
    fn take(&mut self, mut r: Record, partial: bool) -> GameState {
        if let Some(a) = r.nonempty("auth") {
            self.auth = Some(a);
        }
        if let Some(c) = r.nonempty("lastchange") {
            self.lastchange = c;
        }
        let (moves, chat) = if partial {
            (added(&r, "moves"), added(&r, "chat"))
        } else {
            (r.str("moves").unwrap_or_default(), r.str("chat").unwrap_or_default())
        };
        if partial {
            self.moves.push_str(&moves);
            self.chat.push_str(&chat);
        } else {
            self.moves = moves;
            self.chat = chat;
        }
        r.fields.insert("moves".into(), self.moves.clone().into());
        r.fields.insert("chat".into(), self.chat.clone().into());
        GameState::from_record(r)
    }
}

/// The new part of `moves` or `chat` in an update: the field itself in
/// ASIP 1.0, or `movesadd`/`chatadd` as the browser client reads them.
fn added(r: &Record, key: &str) -> String {
    r.nonempty(&format!("{key}add")).or_else(|| r.str(key)).unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn secrets_are_redacted() {
        let kv = redact("sid=abc\nmoves=1w Ra1\nauth=xyz\n");
        assert_eq!(kv, "sid=<redacted>\nmoves=1w Ra1\nauth=<redacted>\n");
        let json = redact(r#"{"sid":"abc","grid":"1","tid":"t"}"#);
        assert_eq!(json, r#"{"sid":"<redacted>","grid":"1","tid":"<redacted>"}"#);
        let numbers = redact(r#"{"sid":6283,"me":{"auth":"x","id":"1"},"n":[{"sid": 5}]}"#);
        assert_eq!(
            numbers,
            r#"{"sid":"<redacted>","me":{"auth":"<redacted>","id":"1"},"n":[{"sid":"<redacted>"}]}"#
        );
    }

    #[test]
    fn seat_urls() {
        let http = Http::new("test", None).unwrap();
        let lobby = Lobby::new(http, DEFAULT_GAMEROOM, Asip::V2);
        assert_eq!(lobby.resolve("client1gs.cgi"), "http://arimaa.com/arimaa/java/ys/ms4/v5/client1gs.cgi");
        let abs = "http://arimaa.com/arimaa/java/ys/ms4/v5/bot1gs.cgi";
        assert_eq!(lobby.resolve(abs), abs);
        let seat = Seat { gsurl: abs.into(), tid: String::new(), grid: String::new(), reply_format: None };
        assert_eq!(seat.server_url(Asip::V2), "http://arimaa.com/arimaa/java/ys/ms4/v5/client2gs.cgi");
    }
}
