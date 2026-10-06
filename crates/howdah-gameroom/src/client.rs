//! HTTP clients for the gameroom lobby and the game server.
//!
//! The server's access rules (see the module docs in `lib.rs`): requests
//! under `/arimaa/gameroom/` need a Referer there, CGIs only answer over
//! plain HTTP, and a refused request is a 404. Requests are spaced out so
//! the client never looks like a scraper; only the long poll waits.

use std::sync::Arc;
use std::time::{Duration, Instant};

use tokio::sync::Mutex;

use howdah_arimaa::Color;

use crate::clock_sync::ClockSync;
use crate::finished::{FinishedGame, RecentGame};
use crate::players::{PastGames, PlayerMatch, parse_past_games, parse_search};
use crate::state::{GameState, Role};
use crate::wire::{Format, Record, encode_request};

/// The gameroom directory on arimaa.com.
pub const DEFAULT_GAMEROOM: &str = "http://arimaa.com/arimaa/gameroom/";

/// Shortest gap between requests, except the long poll.
const MIN_INTERVAL: Duration = Duration::from_millis(1000);

/// How long an ordinary request may take.
const REQUEST_TIMEOUT: Duration = Duration::from_secs(20);

/// TCP keepalive on every connection: probes after this long idle, then
/// every [`KEEPALIVE_INTERVAL`], giving up after [`KEEPALIVE_RETRIES`]
/// unanswered. A long poll sits idle for minutes; without them a
/// connection that died silently (a network change, a NAT dropping it)
/// would go unnoticed until the request timed out. The server's kernel
/// answers the probes, so they cost the gameroom nothing.
const KEEPALIVE_IDLE: Duration = Duration::from_secs(15);
const KEEPALIVE_INTERVAL: Duration = Duration::from_secs(5);
const KEEPALIVE_RETRIES: u32 = 3;

/// How long connecting may take.
const CONNECT_TIMEOUT: Duration = Duration::from_secs(10);

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
    /// The lobby session has expired (or was ended elsewhere): log in
    /// again.
    #[error("the gameroom login has expired")]
    Expired,
}

/// Whether a server error message means the lobby session is gone
/// ("Gameroom: Session id is invalid or expired [<sid>]").
fn is_expired(message: &str) -> bool {
    message.contains("invalid or expired") || message.starts_with("Session Expired")
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

/// Redacts secret values in a URL's query.
pub fn redact_query(url: &str) -> String {
    let Some((base, query)) = url.split_once('?') else { return url.to_string() };
    let parts: Vec<String> = query
        .split('&')
        .map(|p| match p.split_once('=') {
            Some((k, _)) if SECRETS.contains(&k) => format!("{k}=<redacted>"),
            _ => p.to_string(),
        })
        .collect();
    format!("{base}?{}", parts.join("&"))
}

/// The text between `start` and the next `end` in `text`.
fn between<'a>(text: &'a str, start: &str, end: char) -> Option<&'a str> {
    let rest = &text[text.find(start)? + start.len()..];
    Some(&rest[..rest.find(end)?])
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
            // The browser login answers with a redirect that sets the cookie.
            .redirect(reqwest::redirect::Policy::none())
            .connect_timeout(CONNECT_TIMEOUT)
            .tcp_keepalive(KEEPALIVE_IDLE)
            .tcp_keepalive_interval(KEEPALIVE_INTERVAL)
            .tcp_keepalive_retries(KEEPALIVE_RETRIES)
            .build()
            .map_err(|e| Error::Network(e.to_string()))?;
        Ok(Http { client, last: Arc::new(Mutex::new(None)), log })
    }

    /// Posts the browser login form and returns the cookies it sets (the
    /// session is `sid`). A successful login is a redirect; a page back
    /// means it failed, and its text is the error.
    pub async fn login_form(
        &self,
        url: &str,
        params: &[(&str, String)],
    ) -> Result<Vec<(String, String)>, Error> {
        {
            let mut last = self.last.lock().await;
            if let Some(t) = *last {
                tokio::time::sleep_until((t + MIN_INTERVAL).into()).await;
            }
            *last = Some(Instant::now());
        }
        let started = Instant::now();
        let sent = self
            .client
            .post(url)
            .header("Content-Type", "application/x-www-form-urlencoded")
            .header("Referer", url)
            .body(encode_request(Format::KeyValue, params))
            .timeout(REQUEST_TIMEOUT)
            .send()
            .await
            .map_err(|e| Error::Network(e.to_string()));
        let status = sent.as_ref().ok().map(|r| r.status().as_u16());
        let mut cookies = Vec::new();
        if let Ok(r) = &sent {
            for v in r.headers().get_all("set-cookie") {
                let pair = v.to_str().unwrap_or("").split(';').next().unwrap_or("");
                if let Some((k, v)) = pair.split_once('=') {
                    cookies.push((k.trim().to_string(), v.trim().to_string()));
                }
            }
        }
        if let Some(log) = &self.log {
            let request = params
                .iter()
                .map(|(k, v)| {
                    (k.to_string(), if SECRETS.contains(k) { "<redacted>".into() } else { v.clone() })
                })
                .collect();
            let names: Vec<&str> = cookies.iter().map(|(k, _)| k.as_str()).collect();
            let reply = format!("<sets cookies {names:?}>");
            log(&Exchange { url: url.to_string(), request, status, reply, elapsed: started.elapsed() });
        }
        let response = sent?;
        match response.status().as_u16() {
            404 => Err(Error::Refused),
            300..400 if cookies.iter().any(|(k, _)| k == "sid") => Ok(cookies),
            200 => {
                let page = response.text().await.unwrap_or_default();
                Err(Error::Server(format!("login refused: {}", page_text(&page, ""))))
            }
            s => Err(Error::Status(s)),
        }
    }

    /// Gets a page, sending the lobby session as the `sid` cookie when
    /// given. The log shows the URL with secrets redacted and only the
    /// page's size, since pages carry session ids.
    pub async fn get_page(&self, url: &str, cookies: Option<&str>) -> Result<String, Error> {
        self.page(url, cookies, None).await
    }

    /// Posts a form and gets the page that answers it, as
    /// [`Http::get_page`] does. The form's values aren't logged.
    pub async fn post_page(
        &self,
        url: &str,
        cookies: Option<&str>,
        params: &[(&str, String)],
    ) -> Result<String, Error> {
        self.page(url, cookies, Some(encode_request(Format::KeyValue, params))).await
    }

    async fn page(&self, url: &str, cookies: Option<&str>, form: Option<String>) -> Result<String, Error> {
        {
            let mut last = self.last.lock().await;
            if let Some(t) = *last {
                tokio::time::sleep_until((t + MIN_INTERVAL).into()).await;
            }
            *last = Some(Instant::now());
        }
        let started = Instant::now();
        let mut request = match form {
            Some(body) => {
                self.client.post(url).header("Content-Type", "application/x-www-form-urlencoded").body(body)
            }
            None => self.client.get(url),
        };
        request = request.header("Referer", url).timeout(REQUEST_TIMEOUT);
        if let Some(cookies) = cookies {
            request = request.header("Cookie", cookies);
        }
        let (status, reply) = match request.send().await {
            Ok(r) => (Some(r.status().as_u16()), r.text().await.map_err(|e| Error::Network(e.to_string()))),
            Err(e) => (None, Err(Error::Network(e.to_string()))),
        };
        if let Some(log) = &self.log {
            let text = match &reply {
                Ok(t) => format!("<a page of {} bytes>", t.len()),
                Err(e) => format!("<{e}>"),
            };
            let url = redact_query(url);
            log(&Exchange { url, request: Vec::new(), status, reply: text, elapsed: started.elapsed() });
        }
        let reply = reply?;
        match status {
            Some(404) => Err(Error::Refused),
            Some(s) if !(200..300).contains(&s) => Err(Error::Status(s)),
            _ => Ok(reply),
        }
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
        self.post_timed(url, format, params, wait).await.map(|(r, _)| r)
    }

    /// [`Http::post`], also returning when the request went out (after any
    /// wait to space it from the last one).
    pub async fn post_timed(
        &self,
        url: &str,
        format: Format,
        params: &[(&str, String)],
        wait: Option<Duration>,
    ) -> Result<(Record, Instant), Error> {
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
            return Err(if is_expired(&e) { Error::Expired } else { Error::Server(e) });
        }
        Ok((record, started))
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

/// How a viewer seat was made, which decides how quickly moves arrive.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ViewerSeat {
    /// The browser client's way (`opengamewin.cgi`): moves arrive as
    /// they're made.
    Browser,
    /// An ASIP viewer seat, used when the browser way failed (the reason
    /// is given): the server sends moves in ~10 s steps.
    Asip { why: String },
}

/// The lobby's game lists, from one `state` request.
#[derive(Clone, Debug, Default)]
pub struct LobbyGames {
    pub live: Vec<GameInfo>,
    /// The last few games finished in the gameroom, newest first.
    pub recent: Vec<RecentGame>,
    /// The user's games: ones they created that wait for an opponent, and
    /// ones they play (`mygames`).
    pub mine: Vec<GameInfo>,
    /// Games others created, with a seat free (`opengames`), apart from
    /// scheduled ones that haven't started and the user's own.
    pub open: Vec<GameInfo>,
}

impl LobbyGames {
    /// The lists in an ASIP 2.0 `state` reply.
    pub fn from_state(r: &Record) -> LobbyGames {
        let games = |key: &str| r.list(key).iter().filter_map(GameInfo::from_record).collect::<Vec<_>>();
        let mine = games("mygames");
        // The lobby leaves out games scheduled for later (`schts` after the
        // reply's `time`).
        let now = r.int("time").unwrap_or(i64::MAX);
        let open = r
            .list("opengames")
            .iter()
            .filter(|g| g.int("schts").unwrap_or(0) < now)
            .filter_map(GameInfo::from_record)
            .filter(|g| !mine.iter().any(|m| m.gid == g.gid))
            .collect();
        LobbyGames {
            live: games("livegames"),
            recent: r.list("recentgames").iter().filter_map(RecentGame::from_record).collect(),
            mine,
            open,
        }
    }
}

/// A game opened by id: a live one is followed from a viewer seat, and a
/// finished one comes whole.
pub enum Opened {
    Live(GameServer, ViewerSeat),
    Finished(FinishedGame),
}

/// A gameroom (lobby) session.
pub struct Lobby {
    http: Http,
    /// The gameroom directory, ending in `/`.
    base: String,
    asip: Asip,
    sid: Option<String>,
    grid: Option<String>,
    /// The browser login's cookies (`sid` among them), as a `Cookie` header.
    cookies: Option<String>,
    /// The user's time zone, in seconds west of UTC as the browser sends
    /// it (`14400` for UTC-4).
    timezone: i32,
}

impl Lobby {
    pub fn new(http: Http, base: &str, asip: Asip) -> Lobby {
        let base = if base.ends_with('/') { base.to_string() } else { format!("{base}/") };
        Lobby { http, base, asip, sid: None, grid: None, cookies: None, timezone: 0 }
    }

    /// Sets the time zone the next [`Lobby::login`] gives the server, in
    /// seconds west of UTC (JavaScript's `getTimezoneOffset()` times 60,
    /// as the browser sends it). The session's pages show times in it
    /// (the gameroom's "YLT", Your Local Time); without one they're UTC.
    pub fn set_timezone(&mut self, seconds_west: i32) {
        self.timezone = seconds_west;
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

    /// Logs in as the browser client does (`login.cgi`). Its session
    /// works for the ASIP lobby too, and [`Lobby::watch`] needs it.
    pub async fn login(&mut self, username: &str, password: &str) -> Result<(), Error> {
        let url = format!("{}login.cgi", self.base);
        let params = [
            ("email", username.into()),
            ("password", password.into()),
            ("timezone", self.timezone.to_string()),
        ];
        let cookies = self.http.login_form(&url, &params).await?;
        self.sid = cookies.iter().find(|(k, _)| k == "sid").map(|(_, v)| v.clone());
        self.cookies = Some(cookies.iter().map(|(k, v)| format!("{k}={v}")).collect::<Vec<_>>().join("; "));
        Ok(())
    }

    /// Logs in over ASIP. Only [`Lobby::login`]'s session can get a viewer
    /// seat the browser's way.
    pub async fn login_asip(&mut self, username: &str, password: &str) -> Result<(), Error> {
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
        self.cookies = None;
        self.post(self.asip, &[("action", "logout".into()), ("sid", sid)]).await.map(drop)
    }

    /// The live games (ASIP 2.0's `state`; ASIP 1.0 has no such list).
    pub async fn live_games(&self) -> Result<Vec<GameInfo>, Error> {
        Ok(self.games().await?.live)
    }

    /// The live, recently finished, the user's and open games, from one
    /// ASIP 2.0 `state` (the request the browser lobby makes every 20 s).
    pub async fn games(&self) -> Result<LobbyGames, Error> {
        let r = self.post(Asip::V2, &[("action", "state".into()), ("sid", self.sid()?)]).await?;
        Ok(LobbyGames::from_state(&r))
    }

    /// Reserves a seat at game `gid`: a player's side, or a viewer's.
    /// Viewer seats work only over ASIP 1.0 (2.0 gives HTTP 500).
    pub async fn reserve_seat(&self, gid: &str, role: Role) -> Result<Seat, Error> {
        self.reserve_seat_over(self.asip, gid, role).await
    }

    async fn reserve_seat_over(&self, asip: Asip, gid: &str, role: Role) -> Result<Seat, Error> {
        let r = self
            .post(
                asip,
                &[
                    ("action", "reserveseat".into()),
                    ("sid", self.sid()?),
                    ("gid", gid.into()),
                    ("role", role.letter().to_string()),
                ],
            )
            .await?;
        self.seat_from(&r)
    }

    fn seat_from(&self, r: &Record) -> Result<Seat, Error> {
        let field =
            |k: &str| r.nonempty(k).ok_or_else(|| Error::BadReply(format!("no {k} in the seat reply")));
        Ok(Seat {
            gsurl: self.resolve(&field("gsurl")?),
            tid: field("tid")?,
            grid: r.nonempty("grid").or_else(|| self.grid.clone()).unwrap_or_default(),
            reply_format: r.format,
        })
    }

    /// Creates a game with the user as `side` (`newgame`). The reply is
    /// the user's seat at it, as from [`Lobby::reserve_seat`]; the game's
    /// gameroom id comes too, when the reply has it. The app sits
    /// with [`Lobby::play`] instead, the browser's way.
    pub async fn new_game(
        &self,
        side: Color,
        time_control: &str,
        rated: bool,
    ) -> Result<(Option<String>, Seat), Error> {
        let r = self
            .post(
                self.asip,
                &[
                    ("action", "newgame".into()),
                    ("sid", self.sid()?),
                    ("role", Role::Player(side).letter().to_string()),
                    ("timecontrol", time_control.into()),
                    ("rated", if rated { "1" } else { "0" }.into()),
                ],
            )
            .await?;
        Ok((r.nonempty("gid").or_else(|| r.nonempty("id")), self.seat_from(&r)?))
    }

    /// Cancels an open game the user created that nobody has joined
    /// (`cancelopengame`). Over ASIP 2.0 the server answers with an error
    /// even when it worked (a 4steps workaround), so this uses 1.0.
    pub async fn cancel_open_game(&self, gid: &str) -> Result<(), Error> {
        self.post(Asip::V1, &[("action", "cancelopengame".into()), ("sid", self.sid()?), ("gid", gid.into())])
            .await
            .map(drop)
    }

    /// The user's own games (ASIP 2.0 `state`'s `mygames`), and the
    /// open games others created.
    pub async fn my_games(&self) -> Result<(Vec<Record>, Vec<Record>), Error> {
        let r = self.post(Asip::V2, &[("action", "state".into()), ("sid", self.sid()?)]).await?;
        Ok((r.list("mygames"), r.list("opengames")))
    }

    /// Takes the user's seat as `side` at game `gid` the browser client's
    /// way (`opengamewin.cgi` with that role), followed on the browser
    /// client's game server.
    pub async fn play(&self, gid: &str, side: Color) -> Result<GameServer, Error> {
        match self.browser_open_as(gid, Role::Player(side), side).await? {
            Opened::Live(server, _) => Ok(server),
            Opened::Finished(_) => Err(Error::Server(format!("game {gid} has ended"))),
        }
    }

    /// The permanent id of a finished game from its gameroom (temporary) id,
    /// over ASIP 1.0's `findgameid`. Right after a game ends the server may
    /// not know it yet (an error, "Cannot find the game with temp id"), so
    /// callers retry.
    pub async fn find_game_id(&self, tid: &str) -> Result<String, Error> {
        let mut params = vec![("action", "findgameid".to_string()), ("tid", tid.to_string())];
        if let Some(sid) = &self.sid {
            params.push(("sid", sid.clone()));
        }
        let r = self.post(Asip::V1, &params).await?;
        r.nonempty("gid").ok_or_else(|| Error::BadReply("no gid in the findgameid reply".into()))
    }

    /// Watches game `gid` as a viewer, the board seen from `side`, as the
    /// browser client does: the seat from `opengamewin.cgi` (ASIP viewer
    /// seats get moves only in ~10 s steps), followed on the browser
    /// client's game server, `client3gs.cgi` (the one the server is sized
    /// for, sending only new moves). If that fails, an ASIP viewer seat is
    /// followed over ASIP 1.0 instead, when the gameroom id is known (from
    /// an ASIP login).
    pub async fn watch(&self, gid: &str, side: Color) -> Result<(GameServer, ViewerSeat), Error> {
        match self.open(gid, side).await? {
            Opened::Live(server, how) => Ok((server, how)),
            Opened::Finished(_) => Err(Error::Server(format!("game {gid} has ended"))),
        }
    }

    /// Opens game `gid` as the browser gameroom does, whether it's live
    /// (a gameroom id: followed as [`Lobby::watch`] says) or finished (a
    /// permanent id: the whole game, from its viewer page).
    pub async fn open(&self, gid: &str, side: Color) -> Result<Opened, Error> {
        match self.browser_open(gid, side).await {
            Ok(opened) => Ok(opened),
            Err(e @ (Error::NotLoggedIn | Error::Expired | Error::Server(_))) => Err(e),
            Err(e) if self.grid.is_some() => {
                let seat = self.reserve_seat_over(Asip::V1, gid, Role::Viewer).await?;
                let server = GameServer::sit(self.http.clone(), &seat, Asip::V1).await?;
                Ok(Opened::Live(server, ViewerSeat::Asip { why: e.to_string() }))
            }
            Err(e) => Err(e),
        }
    }

    /// The browser client's way into a game: `opengamewin.cgi` (with the
    /// lobby session as the `sid` cookie). For a live game it reserves a
    /// viewer seat and sends the window to `js_sit.cgi`, whose page holds
    /// the game server session; a finished game's page holds the game.
    /// An id the server doesn't know gets an error page (a
    /// [`Error::Server`] with its message).
    async fn browser_open(&self, gid: &str, side: Color) -> Result<Opened, Error> {
        self.browser_open_as(gid, Role::Viewer, side).await
    }

    async fn browser_open_as(&self, gid: &str, role: Role, side: Color) -> Result<Opened, Error> {
        if gid.is_empty() || !gid.chars().all(|c| c.is_ascii_digit()) {
            return Err(Error::Server(format!("not a game id: {gid:?}")));
        }
        let sid = self.sid()?;
        let side = if side == Color::Gold { 'w' } else { 'b' };
        let role = role.letter();
        let url = format!("{}opengamewin.cgi?client=1&gameid={gid}&role={role}&side={side}", self.base);
        let cookies = self.cookies.clone().unwrap_or_else(|| format!("sid={sid}"));
        let page = self.http.get_page(&url, Some(&cookies)).await?;
        if let Some(game) = FinishedGame::from_page(gid, &page) {
            return Ok(Opened::Finished(game));
        }
        // The page also has a commented-out refresh to `gameroom.cgi`.
        let game_page = page
            .split("URL=")
            .skip(1)
            .filter_map(|rest| rest.split('"').next())
            .find(|u| u.contains("js_sit.cgi"));
        let Some(game_page) = game_page else {
            if let Some(e) = error_page(&page) {
                let e = page_text(&e, &sid);
                return Err(if is_expired(&e) { Error::Expired } else { Error::Server(e) });
            }
            return Err(Error::BadReply(format!(
                "opengamewin.cgi gave no game page: {}",
                page_text(&page, &sid)
            )));
        };
        let page = self.http.get_page(game_page, None).await?;
        let Some(gs_sid) = between(&page, "arimaa.vars.sessionid = \"", '"') else {
            return Err(Error::BadReply(format!("js_sit.cgi gave no session: {}", page_text(&page, &sid))));
        };
        let url = between(&page, "arimaa.vars.webservice = \"", '"')
            .map_or_else(|| format!("{}gameserver/client3gs.cgi", self.root()), str::to_string);
        let server = GameServer::join(self.http.clone(), &url, Format::Json, gs_sid);
        Ok(Opened::Live(server, ViewerSeat::Browser))
    }

    /// Searches the players by username or real name (any part of
    /// either), as the gameroom's "Search Players" page does.
    pub async fn search_players(&self, text: &str) -> Result<Vec<PlayerMatch>, Error> {
        let sid = self.sid()?;
        let url = format!("{}searchPlayers.cgi", self.base);
        let cookies = self.cookies.clone().unwrap_or_else(|| format!("sid={sid}"));
        let page = self.http.post_page(&url, Some(&cookies), &[("any", text.trim().to_string())]).await?;
        // The results page looks like an error page too, headed "Results".
        if let Some(e) = error_page(&page).filter(|e| !e.starts_with("Results")) {
            let e = page_text(&e, &sid);
            return Err(if is_expired(&e) { Error::Expired } else { Error::Server(e) });
        }
        Ok(parse_search(&page))
    }

    /// Player `player_id`'s finished games, newest first, 50 from
    /// `offset` on (`pastgames.cgi`, which needs no session).
    pub async fn player_games(&self, player_id: &str, offset: u32) -> Result<PastGames, Error> {
        if player_id.is_empty() || !player_id.chars().all(|c| c.is_ascii_digit()) {
            return Err(Error::Server(format!("not a player id: {player_id:?}")));
        }
        let mut url = format!("{}pastgames.cgi?id={player_id}", self.base);
        if offset > 0 {
            url.push_str(&format!("&off={offset}"));
        }
        // The session's cookie makes the times local (in the login's time zone).
        let page = self.http.get_page(&url, self.cookies.as_deref()).await?;
        Ok(parse_past_games(&page, offset))
    }

    /// Makes a game server URL absolute. ASIP 2.0 can return one relative
    /// to the game server directory (a 4steps workaround).
    fn resolve(&self, gsurl: &str) -> String {
        if gsurl.starts_with("http://") || gsurl.starts_with("https://") {
            return gsurl.to_string();
        }
        format!("{}java/ys/ms4/v5/{}", self.root(), gsurl.trim_start_matches('/'))
    }

    /// The site directory above the gameroom (`…/arimaa/`).
    fn root(&self) -> &str {
        let trimmed = self.base.trim_end_matches('/');
        trimmed.rsplit_once('/').map_or(self.base.as_str(), |(r, _)| &self.base[..r.len() + 1])
    }
}

/// A seat at a game on the game server.
///
/// Every reply stamped with `timeonserver` (the full state, the long
/// poll, an action's) adds a sample to the seat's [`ClockSync`], and the
/// states it returns carry clocks placed on the local clock with it.
pub struct GameServer {
    http: Http,
    url: String,
    format: Format,
    sid: String,
    auth: Option<String>,
    lastchange: String,
    moves: String,
    chat: String,
    sync: Arc<std::sync::Mutex<ClockSync>>,
}

/// Posts a request at a seat, adding the reply's `timeonserver` to `sync`.
async fn timed_post(
    http: &Http,
    sync: &std::sync::Mutex<ClockSync>,
    url: &str,
    format: Format,
    params: &[(&str, String)],
    wait: Option<Duration>,
) -> Result<Record, Error> {
    let (r, sent) = http.post_timed(url, format, params, wait).await?;
    if let Some(t) = r.int("timeonserver") {
        sync.lock().unwrap_or_else(|e| e.into_inner()).add(sent, Instant::now(), t);
    }
    Ok(r)
}

/// The state in a full `gamestate` reply, with its clock estimated.
fn estimated_state(r: Record, sync: &std::sync::Mutex<ClockSync>) -> GameState {
    let mut state = GameState::from_record(r);
    if let Some(c) = &mut state.clock {
        c.estimate(&sync.lock().unwrap_or_else(|e| e.into_inner()));
    }
    state
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
            sync: Default::default(),
        })
    }

    /// Follows a game with a session that's already seated (from the
    /// browser client's seat).
    fn join(http: Http, url: &str, format: Format, sid: &str) -> GameServer {
        GameServer {
            http,
            url: url.to_string(),
            format,
            sid: sid.to_string(),
            auth: None,
            lastchange: "0".into(),
            moves: String::new(),
            chat: String::new(),
            sync: Default::default(),
        }
    }

    pub fn url(&self) -> &str {
        &self.url
    }

    /// A handle for acting at this seat while another task long-polls it.
    /// It needs the `auth` from a [`GameServer::game_state`] first.
    pub fn actions(&self) -> Option<Actions> {
        Some(Actions {
            http: self.http.clone(),
            url: self.url.clone(),
            format: self.format,
            sid: self.sid.clone(),
            auth: self.auth.clone()?,
            sync: self.sync.clone(),
        })
    }

    /// The seat's estimate of the server's clock.
    pub fn clock_sync(&self) -> ClockSync {
        self.sync.lock().unwrap_or_else(|e| e.into_inner()).clone()
    }

    /// The full game state.
    pub async fn game_state(&mut self) -> Result<GameState, Error> {
        let r = timed_post(
            &self.http,
            &self.sync,
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
    /// what's been received, it fetches the full state instead. (A
    /// takeback doesn't shorten them: the server appends `takeback` lines.)
    pub async fn update(&mut self, maxwait: Duration) -> Result<GameState, Error> {
        let r = timed_post(
            &self.http,
            &self.sync,
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
        estimated_state(r, &self.sync)
    }
}

/// Acting at a seat: the requests that need its `auth`.
#[derive(Clone)]
pub struct Actions {
    http: Http,
    url: String,
    format: Format,
    sid: String,
    auth: String,
    /// The seat's clock estimate, shared with its [`GameServer`].
    sync: Arc<std::sync::Mutex<ClockSync>>,
}

impl Actions {
    /// Sends `action` with `extra` parameters and returns the reply.
    pub async fn act(&self, action: &str, extra: &[(&str, String)]) -> Result<Record, Error> {
        let mut params =
            vec![("action", action.to_string()), ("sid", self.sid.clone()), ("auth", self.auth.clone())];
        params.extend(extra.iter().cloned());
        timed_post(&self.http, &self.sync, &self.url, self.format, &params, None).await
    }

    /// The full state, as [`GameServer::game_state`] gets it, from this
    /// handle (it doesn't change what the polling task has received).
    pub async fn game_state(&self) -> Result<GameState, Error> {
        let r = self.act("gamestate", &[("wait", "0".into())]).await?;
        Ok(estimated_state(r, &self.sync))
    }
}

/// A page's text without tags, short, with the session id redacted, for
/// errors.
fn page_text(page: &str, sid: &str) -> String {
    let mut text = String::new();
    let mut in_tag = false;
    for c in page.chars() {
        match c {
            '<' => in_tag = true,
            '>' => in_tag = false,
            c if !in_tag => text.push(c),
            _ => {}
        }
    }
    let mut text = text.split_whitespace().collect::<Vec<_>>().join(" ");
    if !sid.is_empty() {
        text = text.replace(sid, "<redacted>");
    }
    text.chars().take(200).collect()
}

/// The message of a gameroom error page ("Expired Game" with "Cannot find
/// the game id for this game.", say): its heading and its bold text.
fn error_page(page: &str) -> Option<String> {
    if !page.contains("Use the back button") {
        return None;
    }
    let heading = between(page, "<h2 align=center>", '<')?.trim();
    let detail = page.rsplit_once("<b>").and_then(|(_, rest)| rest.split_once("</b>")).map(|(b, _)| b);
    Some(match detail {
        Some(detail) => format!("{heading}: {detail}"),
        None => heading.to_string(),
    })
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
    fn pages_are_read() {
        let page = r#"<meta CONTENT="0; URL=http://x/v5/js_sit.cgi?sid=123&grid=3&rand=9">"#;
        assert_eq!(between(page, "URL=", '"'), Some("http://x/v5/js_sit.cgi?sid=123&grid=3&rand=9"));
        assert_eq!(between(r#"vars.sessionid = "42";"#, "vars.sessionid = \"", '"'), Some("42"));
        assert_eq!(
            redact_query("http://x/js_sit.cgi?sid=123&grid=3"),
            "http://x/js_sit.cgi?sid=<redacted>&grid=3"
        );
        assert_eq!(page_text("<b>Error</b> for 123", "123"), "Error for <redacted>");
        let expired = "<h2 align=center>Expired Game</h2>\n<p><i>Use the back button of your browser.</i>\n\
            <p align=center><b>Cannot find the game id for this game. <!-- playerid = 1, gameid = 2 --></b></p>";
        let e = error_page(expired).unwrap();
        assert_eq!(page_text(&e, ""), "Expired Game: Cannot find the game id for this game.");
        assert_eq!(error_page("<html>a login page</html>"), None);
        let session = "<h2 align=center>Session Expired</h2>\n<p align=center><i>Use the back button of your \
            browser to return to the previous page.</i></p>\n<p><b>Your session has expired, please login again.</b>";
        assert!(is_expired(&page_text(&error_page(session).unwrap(), "")));
        assert!(is_expired("Gameroom: Session id is invalid or expired [<redacted>]"));
        let results = "<h2 align=center>Results</h2>\n<p align=center><i>Use the back button of your browser \
            to return to the previous page.</i></p>";
        assert!(error_page(results).unwrap().starts_with("Results"), "search results look like errors");
        assert!(!is_expired("Expired Game: Cannot find the game id for this game."));
    }

    #[test]
    fn lobby_lists() {
        let r = Record::decode(
            r#"{"time":1000,
              "mygames":[{"id":"7","wusername":"me","busername":null,"timecontrol":"2m/5m","rated":"0"}],
              "opengames":[{"id":"7","wusername":"me","busername":""},
                           {"id":"8","wusername":"","busername":"them","timecontrol":"1m/2m","rated":"1","schts":"0"},
                           {"id":"9","wusername":"later","busername":"","schts":"5000"}],
              "livegames":[{"id":"6","wusername":"a","busername":"b"}]}"#,
        )
        .unwrap();
        let games = LobbyGames::from_state(&r);
        assert_eq!(games.live.len(), 1);
        assert_eq!(games.mine.len(), 1);
        assert_eq!(games.mine[0].players, [Some("me".into()), None]);
        assert_eq!(games.open.len(), 1);
        let open = &games.open[0];
        assert_eq!((open.gid.as_str(), open.rated), ("8", true));
        assert_eq!(open.players, [None, Some("them".into())]);
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
