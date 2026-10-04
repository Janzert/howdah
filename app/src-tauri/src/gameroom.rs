//! Spectating arimaa.com games: the gameroom login, the live-games list,
//! and a task per followed game.
//!
//! Watching a game seats the session as a viewer the browser client's way
//! (`Lobby::watch`), turns it into a match between two remote players, and
//! starts a task that long-polls the game server. Each reply goes through
//! the session (`sync_remote`, `set_remote_clock`, `finish_remote`): the
//! server is the authority on the moves, clocks and result. The task stops
//! when the game ends, when the user stops watching, or when the session
//! moves on to another game (its generation changes).
//!
//! The watch's own state (following, reconnecting, ended) goes out
//! as `gameroom://watch` ([`WatchView`]).
//!
//! A login can be remembered ([`SavedLogin`]): the username and the
//! password, obfuscated, in a file in the app config dir. The password
//! never goes back to the frontend; logging in with an empty password uses
//! the saved one.
//!
//! A finished game gets a permanent id, the record's `GameId` tag. The
//! final state usually carries it (`finishedId`); otherwise it's looked up
//! with `findgameid`, a few times, since right after the end the server may
//! not know it yet.

use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use howdah_arimaa::{Color, TimeControl};
use howdah_gameroom::{Asip, DEFAULT_GAMEROOM, Error, GameServer, GameState, Http, Lobby, ViewerSeat};

use crate::backend::{Events, emit, emit_session};
use crate::controller::{Controller, SharedSession};
use crate::dto::{AnimStep, ApiError, GameroomStatus, LiveGameView, WatchState, WatchView};
use crate::session::{Player, RemoteClock, Session};

/// Event carrying a [`WatchView`].
pub const WATCH_UPDATE: &str = "gameroom://watch";

/// How long the server may hold a long poll; the browser client asks for
/// the same.
const MAXWAIT: Duration = Duration::from_secs(300);

/// Longest wait between retries after failed polls.
const MAX_BACKOFF: Duration = Duration::from_secs(30);

fn lock<T>(m: &Mutex<T>) -> std::sync::MutexGuard<'_, T> {
    m.lock().unwrap_or_else(|e| e.into_inner())
}

fn net_error(e: Error) -> ApiError {
    ApiError::state(format!("arimaa.com: {e}"))
}

/// Hides a saved password from a casual look at the file. It's
/// obfuscation, not encryption: anyone with the file and this code can
/// read it.
const OBFUSCATION_KEY: &[u8] = b"Howdah obfuscates; it doesn't encrypt.";

fn obfuscate(password: &str) -> String {
    let key = OBFUSCATION_KEY.iter().cycle();
    password.bytes().zip(key).map(|(b, k)| format!("{:02x}", b ^ k)).collect()
}

fn deobfuscate(text: &str) -> Option<String> {
    if !text.len().is_multiple_of(2) {
        return None;
    }
    let bytes: Option<Vec<u8>> = (0..text.len())
        .step_by(2)
        .zip(OBFUSCATION_KEY.iter().cycle())
        .map(|(i, k)| u8::from_str_radix(text.get(i..i + 2)?, 16).ok().map(|b| b ^ k))
        .collect();
    String::from_utf8(bytes?).ok()
}

/// A remembered login, in a file of its own (mode 600 on Unix).
pub struct SavedLogin {
    path: Option<PathBuf>,
}

impl SavedLogin {
    /// Keeps the login at `path`; `None` never saves one (tests).
    pub fn new(path: Option<PathBuf>) -> SavedLogin {
        SavedLogin { path }
    }

    /// The saved username and password, if there are any.
    fn load(&self) -> Option<(String, String)> {
        let text = std::fs::read_to_string(self.path.as_ref()?).ok()?;
        let v: serde_json::Value = serde_json::from_str(&text).ok()?;
        let username = v.get("username")?.as_str()?.to_string();
        Some((username, deobfuscate(v.get("password")?.as_str()?)?))
    }

    fn username(&self) -> Option<String> {
        self.load().map(|(u, _)| u)
    }

    fn save(&self, username: &str, password: &str) -> std::io::Result<()> {
        let Some(path) = &self.path else { return Ok(()) };
        if let Some(dir) = path.parent() {
            std::fs::create_dir_all(dir)?;
        }
        let text = serde_json::json!({
            "username": username,
            "password": obfuscate(password),
            "note": "The password is obfuscated, not encrypted.",
        });
        let mut options = std::fs::OpenOptions::new();
        options.write(true).create(true).truncate(true);
        #[cfg(unix)]
        std::os::unix::fs::OpenOptionsExt::mode(&mut options, 0o600);
        use std::io::Write;
        options.open(path)?.write_all(format!("{text:#}\n").as_bytes())?;
        #[cfg(unix)]
        std::fs::set_permissions(path, std::os::unix::fs::PermissionsExt::from_mode(0o600))?;
        Ok(())
    }

    fn forget(&self) {
        if let Some(path) = &self.path {
            let _ = std::fs::remove_file(path);
        }
    }
}

/// The gameroom session: who is logged in, and the lobby to ask.
pub struct Gameroom {
    http: Http,
    lobby: tokio::sync::Mutex<Option<Lobby>>,
    username: Mutex<Option<String>>,
    saved: SavedLogin,
}

impl Gameroom {
    pub fn new(saved: SavedLogin) -> Gameroom {
        let http = Http::new(&howdah_gameroom::user_agent(), None).expect("an HTTP client");
        Gameroom { http, lobby: tokio::sync::Mutex::new(None), username: Mutex::new(None), saved }
    }

    pub fn status(&self) -> GameroomStatus {
        GameroomStatus { username: lock(&self.username).clone(), saved_username: self.saved.username() }
    }

    /// Logs in (the browser client's way), ending any earlier login. An
    /// empty password uses the saved one for that username. With
    /// `remember`, a successful login is saved; without, any saved login is
    /// forgotten.
    pub async fn login(
        &self,
        username: &str,
        password: &str,
        remember: bool,
    ) -> Result<GameroomStatus, ApiError> {
        let username = username.trim();
        let password = match self.saved.load() {
            Some((saved_user, saved)) if password.is_empty() && saved_user == username => saved,
            _ => password.to_string(),
        };
        if username.is_empty() || password.is_empty() {
            return Err(ApiError::illegal("enter a username and password"));
        }
        let mut lobby = Lobby::new(self.http.clone(), DEFAULT_GAMEROOM, Asip::V2);
        lobby.login(username, &password).await.map_err(net_error)?;
        let mut slot = self.lobby.lock().await;
        if let Some(mut old) = slot.replace(lobby) {
            let _ = old.logout().await;
        }
        *lock(&self.username) = Some(username.to_string());
        if remember {
            self.saved
                .save(username, &password)
                .map_err(|e| ApiError::state(format!("logged in, but couldn't save the login: {e}")))?;
        } else {
            self.saved.forget();
        }
        Ok(self.status())
    }

    pub async fn logout(&self) -> Result<(), ApiError> {
        let old = self.lobby.lock().await.take();
        *lock(&self.username) = None;
        match old {
            Some(mut lobby) => lobby.logout().await.map_err(net_error),
            None => Ok(()),
        }
    }

    pub async fn live_games(&self) -> Result<Vec<LiveGameView>, ApiError> {
        let lobby = self.lobby.lock().await;
        let lobby = lobby.as_ref().ok_or_else(|| ApiError::state("log in to arimaa.com first"))?;
        let games = lobby.live_games().await.map_err(net_error)?;
        Ok(games
            .into_iter()
            .map(|g| {
                let [gold, silver] = g.players;
                LiveGameView {
                    gid: g.gid,
                    gold,
                    silver,
                    time_control: g.time_control,
                    rated: g.rated,
                    postal: g.postal,
                }
            })
            .collect())
    }

    /// The permanent id of finished game `gid`.
    async fn find_game_id(&self, gid: &str) -> Result<String, ApiError> {
        let lobby = self.lobby.lock().await;
        let lobby = lobby.as_ref().ok_or_else(|| ApiError::state("log in to arimaa.com first"))?;
        lobby.find_game_id(gid).await.map_err(net_error)
    }

    /// A viewer seat at game `gid`, and its full state.
    async fn seat(&self, gid: &str) -> Result<(GameServer, ViewerSeat, GameState), ApiError> {
        let (mut server, how) = {
            let lobby = self.lobby.lock().await;
            let lobby = lobby.as_ref().ok_or_else(|| ApiError::state("log in to arimaa.com first"))?;
            lobby.watch(gid, Color::Gold).await.map_err(net_error)?
        };
        let state = server.game_state().await.map_err(net_error)?;
        Ok((server, how, state))
    }
}

/// A session's followed game.
pub struct Watch {
    view: Arc<Mutex<WatchView>>,
    task: Option<tauri::async_runtime::JoinHandle<()>>,
}

impl Watch {
    pub fn view(&self) -> WatchView {
        lock(&self.view).clone()
    }

    /// Stops following, and returns the final view.
    pub fn stop(&mut self) -> WatchView {
        if let Some(task) = self.task.take() {
            task.abort();
        }
        let mut view = lock(&self.view);
        view.state = WatchState::Stopped;
        view.detail = None;
        view.clone()
    }
}

impl Drop for Watch {
    fn drop(&mut self) {
        if let Some(task) = self.task.take() {
            task.abort();
        }
    }
}

/// What a session needs to follow a game.
pub struct Target {
    pub session: SharedSession,
    pub controller: Controller,
    pub events: Events,
}

/// Seats a viewer at game `gid`, makes `target`'s session that game, and
/// starts following it.
pub async fn watch(gameroom: &Arc<Gameroom>, gid: &str, target: Target) -> Result<Watch, ApiError> {
    let gid = gid.trim();
    let (server, how, state) = gameroom.seat(gid).await?;
    let generation = {
        let mut s = lock(&target.session);
        start(&mut s, &state);
        s.generation()
    };
    let delayed = matches!(how, ViewerSeat::Asip { .. });
    let view = Arc::new(Mutex::new(WatchView {
        gid: gid.to_string(),
        state: WatchState::Following,
        detail: match how {
            ViewerSeat::Asip { why } => Some(format!("moves arrive about every 10 s ({why})")),
            ViewerSeat::Browser => None,
        },
        delayed,
        finished_id: None,
    }));
    let next = update(&target, generation, &state, &view);
    emit(&target.events, WATCH_UPDATE, lock(&view).clone());
    let gameroom = gameroom.clone();
    let view2 = view.clone();
    let task = match next {
        Next::Follow => tauri::async_runtime::spawn(follow(gameroom, server, target, generation, view2)),
        Next::Stop => tauri::async_runtime::spawn(find_id(gameroom, target, generation, view2)),
    };
    let task = Some(task);
    Ok(Watch { view, task })
}

/// Starts a match between the game's players, as the server reports them.
fn start(s: &mut Session, state: &GameState) {
    let name = |i: usize, side: &str| state.players[i].clone().unwrap_or_else(|| side.to_string());
    let players = [Player::Remote { name: name(0, "Gold") }, Player::Remote { name: name(1, "Silver") }];
    let tc: Option<TimeControl> = state.time_control.as_deref().and_then(|t| t.parse().ok());
    s.start_match(players, [tc; 2], false);
    // As the arimaa.com archive has them.
    let event = state.raw.nonempty("event").unwrap_or_else(|| "Casual game".into());
    let mut tags = vec![("Event".to_string(), event), ("Site".into(), "Over the Net".into())];
    for (i, side) in ["Gold", "Silver"].into_iter().enumerate() {
        if let Some(p) = &state.players[i] {
            tags.push((side.into(), p.clone()));
        }
    }
    if let Some(tc) = &state.time_control {
        tags.push(("TimeControl".into(), tc.clone()));
    }
    s.set_tags(tags);
}

enum Next {
    Follow,
    Stop,
}

/// Where the game stands after a server state.
#[derive(Debug, PartialEq, Eq)]
enum Outcome {
    Playing,
    Ended,
    /// The server's state doesn't fit the game (an illegal move, a result
    /// the rules disagree with or this client can't read).
    Failed(String),
}

/// Brings the session up to the server's state: its moves, clocks and
/// result. Returns the animation for a single new move.
fn apply(s: &mut Session, generation: u64, state: &GameState) -> (Vec<AnimStep>, Outcome) {
    let (animation, mut outcome) = match s.sync_remote(generation, &state.moves) {
        Ok(a) => (a, Outcome::Playing),
        Err(e) => (Vec::new(), Outcome::Failed(e.message)),
    };
    if let Some(c) = state.clock {
        let reported =
            RemoteClock { reserves: c.reserves, turn_elapsed: c.turn_elapsed, game_elapsed: c.game_elapsed };
        let _ = s.set_remote_clock(generation, reported);
    }
    if outcome == Outcome::Playing {
        match (state.result, &state.result_code) {
            (Some(result), _) => {
                outcome = match s.finish_remote(generation, result, None) {
                    Ok(()) => Outcome::Ended,
                    Err(e) => Outcome::Failed(e.message),
                };
                if let Some(id) = &state.finished_id {
                    s.set_tag("GameId", id);
                }
            }
            (None, Some(code)) => {
                outcome =
                    Outcome::Failed(format!("the game ended with a result this client can't read: {code:?}"));
            }
            (None, None) => {}
        }
    }
    (animation, outcome)
}

/// Brings the session and the watch view up to the server's state, and
/// sends out what changed.
fn update(target: &Target, generation: u64, state: &GameState, view: &Mutex<WatchView>) -> Next {
    let outcome = {
        let mut s = lock(&target.session);
        if s.generation() != generation {
            drop(s);
            set_state(target, view, WatchState::Stopped, None);
            return Next::Stop;
        }
        let (animation, outcome) = apply(&mut s, generation, state);
        emit_session(&target.events, &s, animation, None);
        outcome
    };
    target.controller.poke();
    let mut v = lock(view);
    let before = v.clone();
    let next = match outcome {
        Outcome::Playing => {
            if v.state == WatchState::Reconnecting {
                v.state = WatchState::Following;
                v.detail = None;
            }
            Next::Follow
        }
        Outcome::Ended => {
            v.state = WatchState::Ended;
            v.detail = None;
            v.finished_id = state.finished_id.clone();
            Next::Stop
        }
        Outcome::Failed(e) => {
            v.state = WatchState::Failed;
            v.detail = Some(e);
            Next::Stop
        }
    };
    if *v != before {
        emit(&target.events, WATCH_UPDATE, v.clone());
    }
    next
}

fn set_state(target: &Target, view: &Mutex<WatchView>, state: WatchState, detail: Option<String>) {
    let mut v = lock(view);
    if (v.state, &v.detail) == (state, &detail) {
        return;
    }
    v.state = state;
    v.detail = detail;
    emit(&target.events, WATCH_UPDATE, v.clone());
}

/// Long-polls the game server until the game ends or the session moves on.
/// Failed polls are retried with a growing pause; the server may also
/// close a waiting poll with no reply at all.
async fn follow(
    gameroom: Arc<Gameroom>,
    mut server: GameServer,
    target: Target,
    generation: u64,
    view: Arc<Mutex<WatchView>>,
) {
    let mut backoff = Duration::from_secs(1);
    loop {
        match server.update(MAXWAIT).await {
            Ok(state) => {
                backoff = Duration::from_secs(1);
                if let Next::Stop = update(&target, generation, &state, &view) {
                    find_id(gameroom, target, generation, view).await;
                    return;
                }
                continue;
            }
            Err(Error::Empty) => {}
            Err(e @ (Error::Network(_) | Error::Status(_) | Error::BadReply(_))) => {
                set_state(&target, &view, WatchState::Reconnecting, Some(e.to_string()));
            }
            Err(e) => {
                // The server refused the seat, or the table is gone (`No Game
                // Data` once a finished game is cleared away).
                set_state(&target, &view, WatchState::Failed, Some(e.to_string()));
                return;
            }
        }
        if lock(&target.session).generation() != generation {
            set_state(&target, &view, WatchState::Stopped, None);
            return;
        }
        tokio::time::sleep(backoff).await;
        backoff = (backoff * 2).min(MAX_BACKOFF);
    }
}

/// Looks up an ended game's permanent id if its final state had none,
/// and adds it to the record and the view. Right after the end the server
/// may not know it yet, so it tries a few times, waiting longer each time.
async fn find_id(gameroom: Arc<Gameroom>, target: Target, generation: u64, view: Arc<Mutex<WatchView>>) {
    let gid = {
        let v = lock(&view);
        if v.state != WatchState::Ended || v.finished_id.is_some() {
            return;
        }
        v.gid.clone()
    };
    for attempt in 1..=4 {
        tokio::time::sleep(Duration::from_secs(2 * attempt)).await;
        let Ok(id) = gameroom.find_game_id(&gid).await else { continue };
        {
            let mut s = lock(&target.session);
            if s.generation() != generation {
                return;
            }
            s.set_tag("GameId", &id);
        }
        let mut v = lock(&view);
        v.finished_id = Some(id);
        emit(&target.events, WATCH_UPDATE, v.clone());
        return;
    }
}

#[cfg(test)]
mod tests {
    use howdah_arimaa::{GameResult, WinReason};
    use howdah_gameroom::Record;

    use super::*;

    const SETUPS: &str = "1w Ra1 Rb1 Rc1 Dd1 Re1 Rf1 Dg1 Rh1 Ra2 Mb2 Cc2 Hd2 Ee2 Cf2 Hg2 Rh2%13\
        1b rh7 ra7 rh8 rg8 rf8 rc8 rb8 ra8 cf7 cc7 de8 dd8 hg7 hb7 me7 ed7%13";

    /// A `gamestate` reply in ASIP 1.0's `key=value` form.
    fn state(moves: &str, extra: &str) -> GameState {
        let text = format!(
            "wplayer=* bot_a\nbplayer=bot_b\ntimecontrol=1/0/0/0/0\nturn=w\nmoves={SETUPS}{moves}\n\
             tcwreserve=0\ntcbreserve=0\ntimeonserver=1000\nwstartmove=990\n{extra}"
        );
        GameState::from_record(Record::decode(&text).unwrap())
    }

    #[test]
    fn saved_passwords_are_obfuscated() {
        for p in ["", "secret", "pässwörd with a long tail that wraps the key around twice over"] {
            let hidden = obfuscate(p);
            assert!(p.is_empty() || !hidden.contains(p));
            assert_eq!(deobfuscate(&hidden).as_deref(), Some(p));
        }
        assert_eq!(deobfuscate("abc"), None);
        assert_eq!(deobfuscate("zz"), None);

        let path = std::env::temp_dir().join(format!("howdah-saved-login-{}.json", std::process::id()));
        let saved = SavedLogin::new(Some(path.clone()));
        saved.save("alice", "secret").unwrap();
        let text = std::fs::read_to_string(&path).unwrap();
        assert!(text.contains("alice") && !text.contains("secret"));
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            assert_eq!(std::fs::metadata(&path).unwrap().permissions().mode() & 0o777, 0o600);
        }
        assert_eq!(saved.load(), Some(("alice".into(), "secret".into())));
        saved.forget();
        assert_eq!(saved.username(), None);
    }

    #[test]
    fn the_session_becomes_the_servers_game() {
        let mut s = Session::new();
        let first = state("2w Ee2n", "");
        start(&mut s, &first);
        let g = s.generation();
        assert_eq!(apply(&mut s, g, &first).1, Outcome::Playing);
        let v = s.view();
        assert_eq!(v.ply, 3);
        let players = v.players.unwrap();
        assert_eq!((players.gold.name.as_str(), players.silver.name.as_str()), ("bot_a", "bot_b"));
        assert!(!v.can_take_back && !v.plays_live);
        let clock = v.clock.unwrap();
        assert_eq!(clock.turn_elapsed_ms / 1000, 10, "the turn's time comes from the server");
        assert!(s.export(false).contains("bot_a"), "the players are in the record's tags");

        // A new move animates; the same state again changes nothing.
        let next = state("2w Ee2n%132b ed7s", "");
        let (animation, outcome) = apply(&mut s, g, &next);
        assert!(!animation.is_empty() && outcome == Outcome::Playing);
        assert!(apply(&mut s, g, &next).0.is_empty());
    }

    #[test]
    fn the_servers_result_ends_the_game() {
        let mut s = Session::new();
        let ended = state("2w Ee2n", "result=b\nreason=t\nfinishedId=671437\n");
        start(&mut s, &ended);
        let g = s.generation();
        assert_eq!(apply(&mut s, g, &ended).1, Outcome::Ended);
        assert!(s.export(false).contains("[GameId \"671437\"]"), "{}", s.export(false));
        let result = GameResult { winner: Color::Silver, reason: WinReason::Timeout };
        assert_eq!(s.view().result, Some(result));
    }

    #[test]
    fn a_state_that_doesnt_fit_fails() {
        let mut s = Session::new();
        let first = state("2w Ee2n", "");
        start(&mut s, &first);
        let g = s.generation();
        apply(&mut s, g, &first);
        let illegal = state("2w Ee2n%132b Ee3n", "");
        assert!(matches!(apply(&mut s, g, &illegal).1, Outcome::Failed(_)));
        let unknown = state("2w Ee2n", "result=wz\n");
        assert!(matches!(apply(&mut s, g, &unknown).1, Outcome::Failed(e) if e.contains("\"wz\"")));
    }
}
