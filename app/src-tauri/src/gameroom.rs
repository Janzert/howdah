//! Playing and watching arimaa.com games: the gameroom login, the lobby's
//! lists and invitations, and a task per followed game (`docs/GAMEROOM.md`
//! has the design).
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
//! as `gameroom://watch` ([`WatchView`]). If the game server drops the
//! seat, the watch takes a new one ([`reseat`]), or gets the final state
//! if the game ended meanwhile.
//!
//! An expired lobby login is renewed with the saved login, if there is
//! one ([`Gameroom::relogin`]); otherwise the user is logged out.
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
//!
//! Games are opened by id ([`open`]) the browser gameroom's way: a live
//! game's id gets a viewer seat and is followed as above, and a finished
//! game's permanent id gets the whole game, loaded as a record. The lobby's
//! `state` lists both the live games and the last few finished ones.
//!
//! Playing ([`play`]) takes the user's seat at a side instead: that side
//! is the session's human player, and its moves are sent by a second task
//! ([`send_moves`]), woken when the session changes ([`Watch::poke`]). The
//! server answers a move it drops with `ok` too (it does so for a few
//! seconds after refusing one), so a move counts as played only once the
//! server's list has it: if it hasn't come back after a few seconds, the
//! task checks the full state and sends it again. The same task sends the
//! user's takeback requests and answers ([`send_takeback`]); the server's
//! state says which request is open.

use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use howdah_arimaa::{Color, GameRecord, TimeControl, notation};
use howdah_gameroom::{
    Actions, Asip, DEFAULT_GAMEROOM, Error, GameInfo, GameServer, GameState, Http, Invitation, InviteOutcome,
    Lobby, Opened, PastGame, RecentGame, Role, ViewerSeat, parse_chat, parse_result,
};
use tokio::sync::Notify;

use crate::backend::{Events, emit, emit_session};
use crate::controller::{Controller, SharedSession};
use crate::dto::{
    AnimStep, ApiError, ChatLineView, GameroomGames, GameroomStatus, InvitationAnswer, InvitationOutcome,
    InvitationView, LiveGameView, PastGameView, PlayerGamesView, PlayerMatchView, PostalGameView,
    RecentGameView, WatchState, WatchView,
};
use crate::session::{OutgoingMove, Player, RemoteClock, Session, TakebackAction, TakebackEnd};

/// Event carrying a [`WatchView`].
pub const WATCH_UPDATE: &str = "gameroom://watch";

/// Event carrying an [`InvitationAnswer`]: how an invitation the user sent
/// was answered.
pub const INVITATION_ANSWER: &str = "gameroom://invitation";

/// Event carrying [`GameroomGames`] from the lobby watcher, while logged
/// in: new invitations, and the user's games waiting on their move.
pub const LOBBY_UPDATE: &str = "gameroom://lobby";

/// How often the lobby watcher asks for the lobby's state (the browser
/// lobby asks every 20 s while it's open). It also keeps the login from
/// expiring while idle.
const LOBBY_POLL: Duration = Duration::from_secs(60);

/// How long the server may hold a long poll; the browser client asks for
/// the same.
const MAXWAIT: Duration = Duration::from_secs(300);

/// Longest wait between retries after failed polls.
const MAX_BACKOFF: Duration = Duration::from_secs(30);

/// Longest wait between retries after a network error (no connection,
/// a dropped one), so play resumes soon after the network does. Such
/// requests mostly never reach the server; errors from the server itself
/// back off to [`MAX_BACKOFF`].
const NETWORK_BACKOFF: Duration = Duration::from_secs(5);

/// The watch's note while the network is down. A move entered meanwhile
/// shows as being sent ("Sending your move…") until it goes through.
const CONNECTION_LOST: &str = "Connection to arimaa.com lost; retrying";

/// How often the follow task checks whether the computer slept.
const SLEEP_CHECK: Duration = Duration::from_secs(5);

/// How far the wall clock may run ahead of the monotonic clock between
/// checks before it counts as a sleep.
const SLEEP_GAP: Duration = Duration::from_secs(5);

/// How many new seats a watch takes in a row, after the game server drops
/// one, before it gives up.
const MAX_RESEATS: u32 = 3;

/// How long a sent move may take to come back in the server's state
/// before the state is checked and the move sent again. Each try after
/// that waits this much longer, up to [`MAX_BACKOFF`].
const CONFIRM_WAIT: Duration = Duration::from_secs(5);

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

/// Runs a lobby call (`|lobby| call`, a future). If the login has expired,
/// it logs in again with the saved login and tries once more
/// ([`Gameroom::relogin`]).
macro_rules! with_lobby {
    ($gameroom:expr, |$l:ident| $call:expr) => {{
        let mut slot = $gameroom.lobby.lock().await;
        let first = {
            let $l = slot.as_ref().ok_or_else(|| ApiError::state("log in to arimaa.com first"))?;
            $call.await
        };
        match first {
            Err(Error::Expired) => {
                let $l = $gameroom.relogin(&mut slot).await?;
                $call.await.map_err(net_error)
            }
            result => result.map_err(net_error),
        }
    }};
}

/// The gameroom session: who is logged in, and the lobby to ask.
pub struct Gameroom {
    http: Http,
    lobby: tokio::sync::Mutex<Option<Lobby>>,
    username: Mutex<Option<String>>,
    saved: SavedLogin,
    /// Lobby-wide events (not a session's).
    events: Events,
    /// Waits for answers to the user's invitations, by their time.
    waiters: Mutex<std::collections::HashMap<String, tauri::async_runtime::JoinHandle<()>>>,
    /// The lobby watcher, while logged in.
    watcher: Mutex<Option<tauri::async_runtime::JoinHandle<()>>>,
}

impl Gameroom {
    pub fn new(saved: SavedLogin, events: Events) -> Gameroom {
        let http = Http::new(&howdah_gameroom::user_agent(), None).expect("an HTTP client");
        Gameroom {
            http,
            lobby: tokio::sync::Mutex::new(None),
            username: Mutex::new(None),
            saved,
            events,
            waiters: Mutex::new(Default::default()),
            watcher: Mutex::new(None),
        }
    }

    pub fn status(&self) -> GameroomStatus {
        GameroomStatus { username: lock(&self.username).clone(), saved_username: self.saved.username() }
    }

    /// Logs in (the browser client's way), ending another user's earlier
    /// login ([`ends_old_login`]). An empty password uses the saved one for
    /// that username. With `remember`, a successful login is saved; without,
    /// any saved login is forgotten.
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
        let mut lobby = new_lobby(&self.http);
        lobby.login(username, &password).await.map_err(net_error)?;
        let mut slot = self.lobby.lock().await;
        let old_user = lock(&self.username).clone();
        if let Some(mut old) = slot.replace(lobby)
            && ends_old_login(old_user.as_deref(), username)
        {
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
        if let Some(w) = lock(&self.watcher).take() {
            w.abort();
        }
        for (_, w) in lock(&self.waiters).drain() {
            w.abort();
        }
        let old = self.lobby.lock().await.take();
        *lock(&self.username) = None;
        match old {
            Some(mut lobby) => lobby.logout().await.map_err(net_error),
            None => Ok(()),
        }
    }

    /// Logs in again with the saved login after the lobby session
    /// expired. Without one (or if it's another user's), it's logged out,
    /// and the user has to log in again.
    async fn relogin<'a>(&self, slot: &'a mut Option<Lobby>) -> Result<&'a Lobby, ApiError> {
        let user = lock(&self.username).clone();
        let Some((username, password)) = self.saved.load().filter(|(u, _)| Some(u) == user.as_ref()) else {
            *slot = None;
            *lock(&self.username) = None;
            return Err(ApiError::state("the arimaa.com login has expired: log in again"));
        };
        let mut lobby = new_lobby(&self.http);
        lobby.login(&username, &password).await.map_err(net_error)?;
        Ok(slot.insert(lobby))
    }

    /// The live games and the last few finished ones.
    pub async fn games(&self) -> Result<GameroomGames, ApiError> {
        let games = with_lobby!(self, |l| l.games())?;
        let views = |list: Vec<GameInfo>| list.into_iter().map(game_view).collect();
        let invitations = games
            .invited_me
            .into_iter()
            .map(|i| invitation_view(i, true))
            .chain(games.i_invited.into_iter().map(|i| invitation_view(i, false)))
            .collect();
        Ok(GameroomGames {
            user: lock(&self.username).clone(),
            live: views(games.live),
            recent: games.recent.into_iter().map(recent_view).collect(),
            mine: views(games.mine),
            open: views(games.open),
            invitations,
        })
    }

    /// Starts the lobby watcher: every [`LOBBY_POLL`] while logged in, the
    /// lobby's lists go out as [`LOBBY_UPDATE`].
    pub fn watch_lobby(self: &Arc<Self>) {
        let gameroom = self.clone();
        let task = tauri::async_runtime::spawn(async move {
            loop {
                if lock(&gameroom.username).is_none() {
                    return;
                }
                if let Ok(games) = gameroom.games().await {
                    emit(&gameroom.events, LOBBY_UPDATE, games);
                }
                tokio::time::sleep(LOBBY_POLL).await;
            }
        });
        if let Some(old) = lock(&self.watcher).replace(task) {
            old.abort();
        }
    }

    /// Invites player `who` to a game with the user as `side`. A task
    /// waits for the answer and sends it as [`INVITATION_ANSWER`].
    pub async fn invite(
        self: &Arc<Self>,
        who: &str,
        side: Color,
        time_control: &str,
        rated: bool,
        message: &str,
    ) -> Result<(), ApiError> {
        let (who, time_control) = (who.trim(), time_control.trim());
        if who.is_empty() {
            return Err(ApiError::illegal("enter the username to invite"));
        }
        time_control.parse::<TimeControl>().map_err(|e| ApiError::illegal(e.to_string()))?;
        let (iid, ts) = with_lobby!(self, |l| l.invite(who, side, time_control, rated, message.trim()))?;
        let gameroom = self.clone();
        let who = who.to_string();
        let key = ts.clone();
        let task = tauri::async_runtime::spawn(async move {
            let outcome = gameroom.await_answer(&iid, &ts, side).await;
            if let Some(outcome) = outcome {
                emit(&gameroom.events, INVITATION_ANSWER, InvitationAnswer { opponent: who, outcome });
            }
            lock(&gameroom.waiters).remove(&ts);
        });
        lock(&self.waiters).insert(key, task);
        Ok(())
    }

    /// Long-polls the inviter's waiting page until the invitation is
    /// answered or gone. `None` once logged out. The wait runs on a copy
    /// of the lobby session, so other lobby calls go on meanwhile.
    async fn await_answer(&self, iid: &str, ts: &str, side: Color) -> Option<InvitationOutcome> {
        loop {
            let lobby = self.lobby.lock().await.clone()?;
            match lobby.wait_invite(iid, ts).await {
                Ok(InviteOutcome::Accepted { gid }) => {
                    return Some(InvitationOutcome::Accepted { gid, side });
                }
                Ok(InviteOutcome::Declined { message }) => {
                    return Some(InvitationOutcome::Declined { message });
                }
                Ok(_) => {
                    // No answer yet: still open?
                    let games: Result<_, ApiError> = async { with_lobby!(self, |l| l.games()) }.await;
                    match games {
                        Ok(g) if g.i_invited.iter().any(|i| i.invited_id == iid && i.created == ts) => {}
                        Ok(_) => return Some(InvitationOutcome::Gone),
                        Err(_) => tokio::time::sleep(Duration::from_secs(10)).await,
                    }
                }
                Err(_) => tokio::time::sleep(Duration::from_secs(10)).await,
            }
        }
    }

    /// Accepts the invitation from `inviter_id` sent at `created`, and
    /// returns the game it made and the user's side in it.
    async fn accept_invite(&self, inviter_id: &str, created: &str) -> Result<(String, Color), ApiError> {
        let open = with_lobby!(self, |l| l.games())?;
        let inv = open
            .invited_me
            .into_iter()
            .find(|i| i.inviter_id == inviter_id && i.created == created)
            .ok_or_else(|| ApiError::state("that invitation is no longer open"))?;
        with_lobby!(self, |l| l.accept_invite(inviter_id, created))?;
        // The game it made: the newest of the user's games against the
        // inviter.
        let games = with_lobby!(self, |l| l.games())?;
        let inviter = inv.inviter_side.index();
        let gid = games
            .mine
            .iter()
            .filter(|g| {
                g.players[inviter]
                    .as_deref()
                    .zip(inv.opponent.as_deref())
                    .is_some_and(|(a, b)| a.eq_ignore_ascii_case(b))
            })
            .filter_map(|g| g.gid.parse::<u64>().ok())
            .max()
            .ok_or_else(|| ApiError::state("accepted, but the game isn't among your games yet"))?;
        Ok((gid.to_string(), inv.inviter_side.opponent()))
    }

    /// Declines the invitation from `inviter_id` sent at `created`.
    pub async fn decline_invite(
        &self,
        inviter_id: &str,
        created: &str,
        reason: &str,
    ) -> Result<(), ApiError> {
        let reason = Some(reason.trim()).filter(|r| !r.is_empty()).unwrap_or("Declined.");
        with_lobby!(self, |l| l.decline_invite(inviter_id, created, reason))
    }

    /// Cancels the user's invitation to `invited_id` sent at `created`.
    pub async fn cancel_invite(&self, invited_id: &str, created: &str) -> Result<(), ApiError> {
        if let Some(w) = lock(&self.waiters).remove(created) {
            w.abort();
        }
        with_lobby!(self, |l| l.cancel_invite(invited_id, created, "Canceled."))
    }

    /// Creates a game with the user as `side`, and returns its gameroom
    /// id. `time_control` is in the gameroom's format (`2m/5m/100/0/30m`).
    async fn create_game(&self, side: Color, time_control: &str, rated: bool) -> Result<String, ApiError> {
        let time_control = time_control.trim();
        time_control.parse::<TimeControl>().map_err(|e| ApiError::illegal(e.to_string()))?;
        let (gid, _) = with_lobby!(self, |l| l.new_game(side, time_control, rated))?;
        gid.ok_or_else(|| ApiError::state("arimaa.com created the game but gave no id for it"))
    }

    /// Cancels game `gid`, which the user created and nobody has joined.
    pub async fn cancel_game(&self, gid: &str) -> Result<(), ApiError> {
        with_lobby!(self, |l| l.cancel_open_game(gid.trim()))
    }

    /// The permanent id of finished game `gid`.
    async fn find_game_id(&self, gid: &str) -> Result<String, ApiError> {
        with_lobby!(self, |l| l.find_game_id(gid))
    }

    /// Opens game `gid`: a viewer seat at a live game, or a finished game
    /// whole.
    async fn open(&self, gid: &str) -> Result<Opened, ApiError> {
        with_lobby!(self, |l| l.open(gid, Color::Gold))
    }

    /// Takes the user's seat as `side` at game `gid`.
    async fn sit(&self, gid: &str, side: Color) -> Result<GameServer, ApiError> {
        with_lobby!(self, |l| l.play(gid, side))
    }

    /// The players whose username or real name contains `text`.
    pub async fn search_players(&self, text: &str) -> Result<Vec<PlayerMatchView>, ApiError> {
        if text.trim().is_empty() {
            return Err(ApiError::illegal("enter part of a username or name"));
        }
        let found = with_lobby!(self, |l| l.search_players(text))?;
        Ok(found
            .into_iter()
            .map(|p| PlayerMatchView { id: p.id, username: p.username, name: p.name })
            .collect())
    }

    /// The postal games being played (the live list leaves them out).
    pub async fn postal_games(&self) -> Result<Vec<PostalGameView>, ApiError> {
        let games = with_lobby!(self, |l| l.postal_games())?;
        Ok(games
            .into_iter()
            .map(|g| {
                let [gold, silver] = g.players;
                let [gold_rating, silver_rating] = g.ratings;
                PostalGameView {
                    gid: g.gid,
                    gold,
                    silver,
                    gold_rating,
                    silver_rating,
                    time_control: g.time_control,
                    rated: g.rated,
                }
            })
            .collect())
    }

    /// A player's finished games, newest first, 50 from `offset` on.
    pub async fn player_games(&self, player_id: &str, offset: u32) -> Result<PlayerGamesView, ApiError> {
        let page = with_lobby!(self, |l| l.player_games(player_id, offset))?;
        Ok(PlayerGamesView { games: page.games.into_iter().map(past_view).collect(), next: page.next })
    }
}

fn invitation_view(i: Invitation, incoming: bool) -> InvitationView {
    InvitationView {
        incoming,
        opponent: i.opponent,
        opponent_rating: i.opponent_rating,
        side: if incoming { i.inviter_side.opponent() } else { i.inviter_side },
        time_control: i.time_control,
        rated: i.rated,
        message: i.message,
        other_id: if incoming { i.inviter_id } else { i.invited_id },
        created: i.created,
    }
}

fn game_view(g: GameInfo) -> LiveGameView {
    let [gold, silver] = g.players;
    LiveGameView {
        gid: g.gid,
        gold,
        silver,
        time_control: g.time_control,
        rated: g.rated,
        postal: g.postal,
        turn: g.turn,
    }
}

fn past_view(g: PastGame) -> PastGameView {
    let [gold, silver] = g.players;
    let [gold_rating, silver_rating] = g.ratings;
    let result = match (g.winner, &g.reason) {
        (Some(w), Some(r)) => parse_result(&format!("{}{r}", if w == Color::Gold { 'w' } else { 'b' })),
        _ => None,
    };
    PastGameView {
        gid: g.id,
        gold,
        silver,
        gold_rating,
        silver_rating,
        time_control: g.time_control,
        rated: g.rated,
        result,
        moves: g.moves,
        finished: g.finished,
    }
}

fn recent_view(g: RecentGame) -> RecentGameView {
    let [gold, silver] = g.players;
    let [gold_rating, silver_rating] = g.ratings;
    let result = match (&g.result, &g.reason) {
        (Some(w), Some(r)) => parse_result(&format!("{w}{r}")),
        _ => None,
    };
    RecentGameView {
        gid: g.id,
        gold,
        silver,
        gold_rating,
        silver_rating,
        time_control: g.time_control,
        rated: g.rated,
        postal: g.postal,
        result,
        moves: g.moves,
        ended_ms: g.ended.and_then(|t| u64::try_from(t).ok()).map(|t| t * 1000),
    }
}

/// Whether logging in as `new_user` should log the previous login
/// (`old_user`'s) out. Not for the same user: an arimaa.com logout ends
/// every lobby login of the account, so it would end the new one too. The
/// old session is just dropped. Names are compared ignoring case, since a
/// wrong "different user" logs the user out.
fn ends_old_login(old_user: Option<&str>, new_user: &str) -> bool {
    old_user.is_some_and(|old| !old.eq_ignore_ascii_case(new_user))
}

/// A lobby that logs in with the computer's time zone, so the gameroom's
/// pages show times as local ("YLT" there).
fn new_lobby(http: &Http) -> Lobby {
    let mut lobby = Lobby::new(http.clone(), DEFAULT_GAMEROOM, Asip::V2);
    lobby.set_timezone(-chrono::Local::now().offset().local_minus_utc());
    lobby
}

/// A game opened by id.
pub enum Open {
    /// A live game, now followed in the session.
    Watching(Watch),
    /// A finished game, to load into the session.
    Finished(GameRecord),
}

/// A session's followed (or played) game.
pub struct Watch {
    view: Arc<Mutex<WatchView>>,
    task: Option<tauri::async_runtime::JoinHandle<()>>,
    /// Sends the user's moves, when playing.
    sender: Option<tauri::async_runtime::JoinHandle<()>>,
    wake: Arc<Notify>,
    /// The seat's actions, when playing (shared with the tasks, which
    /// replace them when the seat is).
    actions: Option<Arc<Mutex<Actions>>>,
}

impl Watch {
    pub fn view(&self) -> WatchView {
        lock(&self.view).clone()
    }

    /// Resigns the game the user plays: the request to await, which
    /// holds no lock. The result comes back with the server's next update.
    pub fn resign(&self) -> Result<impl Future<Output = Result<(), ApiError>> + use<>, ApiError> {
        let actions = self.actions.as_ref().ok_or_else(|| ApiError::state("you're not playing this game"))?;
        if lock(&self.view).state == WatchState::Ended {
            return Err(ApiError::state("the game is over"));
        }
        let act = lock(actions).clone();
        Ok(async move { act.act("resign", &[]).await.map(drop).map_err(net_error) })
    }

    /// Sends `text` to the game's chat as the user: the request to await.
    /// The line comes back with the server's next update. Newlines become
    /// spaces, and an empty message is refused (the server would add an
    /// empty line).
    pub fn chat(&self, text: &str) -> Result<impl Future<Output = Result<(), ApiError>> + use<>, ApiError> {
        let actions = self.actions.as_ref().ok_or_else(|| ApiError::state("only players can chat"))?;
        if !lock(&self.view).chat_open {
            return Err(ApiError::state("the game's chat has closed"));
        }
        let text = text.split_whitespace().collect::<Vec<_>>().join(" ");
        if text.is_empty() {
            return Err(ApiError::illegal("type a message first"));
        }
        let act = lock(actions).clone();
        Ok(async move { act.act("chat", &[("chat", text)]).await.map(drop).map_err(net_error) })
    }

    /// Tells the game's tasks the session changed (a move to send, say).
    pub fn poke(&self) {
        self.wake.notify_one();
    }

    /// Stops following, and returns the final view.
    pub fn stop(&mut self) -> WatchView {
        for task in [self.task.take(), self.sender.take()].into_iter().flatten() {
            task.abort();
        }
        let mut view = lock(&self.view);
        view.state = WatchState::Stopped;
        view.detail = None;
        view.chat_open = false;
        view.clone()
    }
}

impl Drop for Watch {
    fn drop(&mut self) {
        for task in [self.task.take(), self.sender.take()].into_iter().flatten() {
            task.abort();
        }
    }
}

/// What a session needs to follow a game.
#[derive(Clone)]
pub struct Target {
    pub session: SharedSession,
    pub controller: Controller,
    pub events: Events,
}

/// Opens game `gid`. A finished game comes back as its record; a live
/// one gets a viewer seat, becomes `target`'s session's game, and is
/// followed.
pub async fn open(gameroom: &Arc<Gameroom>, gid: &str, target: Target) -> Result<Open, ApiError> {
    let gid = gid.trim();
    let (server, how) = match gameroom.open(gid).await? {
        Opened::Live(server, how) => (server, how),
        Opened::Finished(game) => {
            return game.record().map(Open::Finished).map_err(|e| {
                ApiError::state(format!("arimaa.com game {gid} doesn't read as a game: {}", e.error))
            });
        }
    };
    begin(gameroom, gid, server, Seat::Viewer(how), target).await.map(Open::Watching)
}

/// Accepts an invitation (see [`Gameroom::accept_invite`]) and plays the
/// game it makes in `target`'s session.
pub async fn accept(
    gameroom: &Arc<Gameroom>,
    inviter_id: &str,
    created: &str,
    target: Target,
) -> Result<Watch, ApiError> {
    let (gid, side) = gameroom.accept_invite(inviter_id, created).await?;
    play(gameroom, &gid, side, target).await
}

/// Takes the user's seat as `side` at live game `gid`, and plays it in
/// `target`'s session: the user is that side's human player, and the
/// moves they play there are sent to the server.
pub async fn play(
    gameroom: &Arc<Gameroom>,
    gid: &str,
    side: Color,
    target: Target,
) -> Result<Watch, ApiError> {
    let gid = gid.trim();
    let server = gameroom.sit(gid, side).await?;
    begin(gameroom, gid, server, Seat::Player(side), target).await
}

/// Creates a game with the user as `side` and plays it in `target`'s
/// session, as [`play`] does. The user's seat waits for an opponent
/// (`WatchView.waiting`); their first move is held until one sits.
pub async fn create(
    gameroom: &Arc<Gameroom>,
    side: Color,
    time_control: &str,
    rated: bool,
    target: Target,
) -> Result<Watch, ApiError> {
    let gid = gameroom.create_game(side, time_control, rated).await?;
    // The browser's way to the seat, as for joining; `newgame`'s own
    // reservation goes unused.
    play(gameroom, &gid, side, target).await.map_err(|e| {
        ApiError::state(format!(
            "created game {gid}, but couldn't take your seat: {} (it's under your games, to resume or cancel)",
            e.message
        ))
    })
}

/// The user's seat at a game.
#[derive(Clone, Debug)]
enum Seat {
    Viewer(ViewerSeat),
    Player(Color),
}

/// Makes the game at `server` the session's and starts following it (and,
/// at a player's seat, sending the user's moves).
async fn begin(
    gameroom: &Arc<Gameroom>,
    gid: &str,
    mut server: GameServer,
    seat: Seat,
    target: Target,
) -> Result<Watch, ApiError> {
    let state = server.game_state().await.map_err(net_error)?;
    let actions = match seat {
        Seat::Player(side) => {
            if state.role != Some(Role::Player(side)) {
                return Err(ApiError::state(format!("arimaa.com didn't seat you as {side:?} in game {gid}")));
            }
            let actions =
                server.actions().ok_or_else(|| ApiError::state("arimaa.com gave no seat to play"))?;
            Some(Arc::new(Mutex::new(actions)))
        }
        Seat::Viewer(_) => None,
    };
    let generation = {
        let mut s = lock(&target.session);
        start(&mut s, &state);
        s.generation()
    };
    let view = Arc::new(Mutex::new(WatchView {
        gid: gid.to_string(),
        state: WatchState::Following,
        detail: None,
        delayed: false,
        finished_id: None,
        event: state.raw.nonempty("event"),
        side: None,
        postal: state.postal,
        time_control: state.time_control.clone(),
        rated: state.rated,
        away: [false; 2],
        waiting: false,
        refused: None,
        chat: Vec::new(),
        chat_open: false,
    }));
    set_seat(&mut lock(&view), &seat);
    let next = update(&target, generation, &state, &view);
    emit(&target.events, WATCH_UPDATE, lock(&view).clone());
    let wake = Arc::new(Notify::new());
    let sender = actions.clone().map(|actions| {
        let run = send_moves(target.clone(), generation, view.clone(), actions, wake.clone());
        tauri::async_runtime::spawn(run)
    });
    let gameroom = gameroom.clone();
    let view2 = view.clone();
    let task = match next {
        Next::Follow => {
            let sender = actions.clone().map(|actions| Sender { actions, wake: wake.clone() });
            let run = follow(gameroom, server, seat, sender, target, generation, view2);
            tauri::async_runtime::spawn(run)
        }
        Next::Stop => tauri::async_runtime::spawn(after_end(gameroom, server, target, generation, view2)),
    };
    // A move entered before the seat was ready goes out now.
    wake.notify_one();
    Ok(Watch { view, task: Some(task), sender, wake, actions })
}

/// Starts a match between the game's players, as the server reports them.
/// At a player's seat (the state's role), that side is the user's, a
/// human player.
fn start(s: &mut Session, state: &GameState) {
    let name = |i: usize, side: &str| state.players[i].clone().unwrap_or_else(|| side.to_string());
    let mut players = [Player::Remote { name: name(0, "Gold") }, Player::Remote { name: name(1, "Silver") }];
    if let Some(Role::Player(side)) = state.role {
        players[side.index()] = Player::Human;
    }
    // The gameroom offers `0/0/0/0/0` as "No time limit" (a postal choice);
    // by the time control rules it would leave no time at all.
    let tc: Option<TimeControl> = state
        .time_control
        .as_deref()
        .and_then(|t| t.parse().ok())
        .filter(|t: &TimeControl| !(t.move_time().is_zero() && t.starting_reserve().is_zero()));
    s.start_match(players, [tc; 2], false);
    // In rated games the server answers `ok` to takeback requests and
    // ignores them (seen in game 539472); don't ask.
    if state.rated {
        s.forbid_takeback_requests();
    }
    // As the arimaa.com archive has them.
    let event = state.raw.nonempty("event").unwrap_or_else(|| "Casual game".into());
    let mut tags = vec![("Event".to_string(), event), ("Site".into(), "Over the Net".into())];
    for (i, side) in ["Gold", "Silver"].into_iter().enumerate() {
        if let Some(p) = &state.players[i] {
            tags.push((side.into(), p.clone()));
        }
        // An unrated player can show as 0.
        let key = ["wrating", "brating"][i];
        if let Some(r) = state.raw.nonempty(key).filter(|r| r.trim() != "0") {
            tags.push((format!("{side}Rating"), r));
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

/// What a server state did to the session.
struct Applied {
    /// The animation for a single new move.
    animation: Vec<AnimStep>,
    outcome: Outcome,
    /// What came of a takeback request the server stopped showing.
    takeback: Option<TakebackEnd>,
}

/// Brings the session up to the server's state: its moves, takeback
/// request, clocks and result.
fn apply(s: &mut Session, generation: u64, state: &GameState) -> Applied {
    let (animation, mut outcome) = match s.sync_remote(generation, &state.moves, state.last_move_used) {
        Ok(a) => (a, Outcome::Playing),
        Err(e) => (Vec::new(), Outcome::Failed(e.message)),
    };
    let takeback = s.sync_takeback(generation, state.takeback).ok().flatten();
    // An opponent who sat down after the game was opened.
    for (i, side) in [Color::Gold, Color::Silver].into_iter().enumerate() {
        if let Some(name) = &state.players[i]
            && s.set_remote_name(side, name)
        {
            s.set_tag(["Gold", "Silver"][i], name);
            if let Some(r) = state.raw.nonempty(["wrating", "brating"][i]).filter(|r| r.trim() != "0") {
                s.set_tag(["GoldRating", "SilverRating"][i], &r);
            }
        }
    }
    if let Some(c) = state.clock {
        // On the user's own turn, the clock shows the time left for a
        // move sent now, which reaches the server a little later.
        let users_turn = state.turn.is_some_and(|t| state.role == Some(Role::Player(t)));
        let turn_started =
            c.turn_started_at.map(|t| if users_turn { t.checked_sub(c.one_way).unwrap_or(t) } else { t });
        let reported = RemoteClock {
            reserves: c.reserves,
            turn_elapsed: c.turn_elapsed,
            game_elapsed: c.game_elapsed,
            running: c.running,
            turn_started,
            game_started: c.game_started_at,
        };
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
    Applied { animation, outcome, takeback }
}

/// Brings the session and the watch view up to the server's state, and
/// sends out what changed.
fn update(target: &Target, generation: u64, state: &GameState, view: &Mutex<WatchView>) -> Next {
    let (outcome, takeback) = {
        let mut s = lock(&target.session);
        if s.generation() != generation {
            drop(s);
            set_state(target, view, WatchState::Stopped, None);
            return Next::Stop;
        }
        let Applied { animation, outcome, takeback } = apply(&mut s, generation, state);
        emit_session(&target.events, &s, animation, None);
        (outcome, takeback)
    };
    target.controller.poke();
    let mut v = lock(view);
    let before = v.clone();
    v.waiting = waiting(v.side, state);
    v.away = away(state);
    v.chat = kept_chat(&v.chat, chat_view(&state.chat));
    let next = match outcome {
        Outcome::Playing => {
            if v.state == WatchState::Reconnecting {
                v.state = WatchState::Following;
                v.detail = None;
            }
            if takeback == Some(TakebackEnd::Declined) {
                v.detail = Some("your takeback request was declined".into());
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
            v.chat_open = false;
            Next::Stop
        }
    };
    if *v != before {
        emit(&target.events, WATCH_UPDATE, v.clone());
    }
    next
}

/// Whether each player is away from the table: they've left (`0`), or,
/// when the state reports the other's presence, theirs is missing although
/// the seat is theirs (a player who accepted an invitation and hasn't sat
/// down: game 539507).
fn away(state: &GameState) -> [bool; 2] {
    let reported = state.present.iter().any(Option::is_some);
    [0, 1].map(|i| match state.present[i] {
        Some(present) => !present,
        None => reported && state.players[i].is_some(),
    })
}

/// Whether the user's seat (at `side`) waits for an opponent: the game
/// hasn't started and the other seat is empty.
fn waiting(side: Option<Color>, state: &GameState) -> bool {
    side.is_some_and(|side| {
        !state.started && state.result.is_none() && state.players[side.opponent().index()].is_none()
    })
}

/// Shows the seat: the user's side, or how a viewer gets the moves.
fn set_seat(view: &mut WatchView, seat: &Seat) {
    view.side = match seat {
        Seat::Player(side) => Some(*side),
        Seat::Viewer(_) => None,
    };
    view.chat_open = view.side.is_some();
    view.delayed = matches!(seat, Seat::Viewer(ViewerSeat::Asip { .. }));
    view.detail = match seat {
        Seat::Viewer(ViewerSeat::Asip { why }) => Some(format!("moves arrive about every 10 s ({why})")),
        _ => None,
    };
}

/// Shows (or clears) why the server refused the user's move.
fn set_refused(target: &Target, view: &Mutex<WatchView>, refused: Option<String>) {
    let mut v = lock(view);
    if v.refused != refused {
        v.refused = refused;
        emit(&target.events, WATCH_UPDATE, v.clone());
    }
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

/// What [`follow`] shares with [`send_moves`] at a player's seat.
struct Sender {
    /// The seat's actions, replaced when the seat is.
    actions: Arc<Mutex<Actions>>,
    /// Wakes the sender (once an opponent sits).
    wake: Arc<Notify>,
}

/// Notices the computer sleeping. On Linux and macOS the monotonic clock
/// stops during sleep, so a long poll started before it would wait out
/// its whole timeout afterwards on a connection that's likely dead; the
/// wall clock doesn't stop, so a gap between them shows a sleep. (On
/// Windows the monotonic clock runs on, and the poll's timeout already
/// fires in time.)
struct SleepWatch {
    mono: Instant,
    wall: std::time::SystemTime,
}

impl SleepWatch {
    fn new() -> SleepWatch {
        SleepWatch { mono: Instant::now(), wall: std::time::SystemTime::now() }
    }

    /// Whether the wall clock ran ahead of the monotonic one since the last
    /// check (a sleep). A wall clock set back isn't one.
    fn slept(&mut self, mono: Instant, wall: std::time::SystemTime) -> bool {
        let mono_gap = mono.saturating_duration_since(self.mono);
        let wall_gap = wall.duration_since(self.wall).unwrap_or_default();
        (self.mono, self.wall) = (mono, wall);
        wall_gap > mono_gap + SLEEP_GAP
    }

    /// Resolves once a check finds that the computer slept.
    async fn woke(&mut self) {
        loop {
            tokio::time::sleep(SLEEP_CHECK).await;
            if self.slept(Instant::now(), std::time::SystemTime::now()) {
                return;
            }
        }
    }
}

/// Long-polls the game server until the game ends or the session moves on.
/// Failed polls are retried with a growing pause; the server may also
/// close a waiting poll with no reply at all.
async fn follow(
    gameroom: Arc<Gameroom>,
    mut server: GameServer,
    seat: Seat,
    sender: Option<Sender>,
    target: Target,
    generation: u64,
    view: Arc<Mutex<WatchView>>,
) {
    let gid = lock(&view).gid.clone();
    // Once an opponent sits, the sender sends the move it held.
    let update = |state: &GameState| {
        let was_waiting = lock(&view).waiting;
        let next = update(&target, generation, state, &view);
        if let Some(sender) = &sender
            && was_waiting
            && !lock(&view).waiting
        {
            sender.wake.notify_one();
        }
        next
    };
    let mut backoff = Duration::from_secs(1);
    let mut reseats = 0;
    let mut sleep_watch = SleepWatch::new();
    // After a sleep, the full state instead of waiting on the old poll.
    let mut resync = false;
    loop {
        let polled = if std::mem::take(&mut resync) {
            server.game_state().await
        } else {
            tokio::select! {
                polled = server.update(MAXWAIT) => polled,
                () = sleep_watch.woke() => {
                    set_state(&target, &view, WatchState::Reconnecting, Some("the computer slept; catching up".into()));
                    resync = true;
                    // The sender checks its move too.
                    if let Some(sender) = &sender {
                        sender.wake.notify_one();
                    }
                    continue;
                }
            }
        };
        // Network errors retry sooner than the server's own.
        let mut cap = MAX_BACKOFF;
        match polled {
            Ok(state) => {
                backoff = Duration::from_secs(1);
                reseats = 0;
                if let Next::Stop = update(&state) {
                    after_end(gameroom, server, target, generation, view).await;
                    return;
                }
                continue;
            }
            Err(Error::Empty) => {}
            Err(Error::Network(_)) => {
                cap = NETWORK_BACKOFF;
                set_state(&target, &view, WatchState::Reconnecting, Some(CONNECTION_LOST.into()));
            }
            Err(e @ (Error::Status(_) | Error::BadReply(_))) => {
                set_state(&target, &view, WatchState::Reconnecting, Some(e.to_string()));
            }
            Err(e @ (Error::Server(_) | Error::Refused | Error::Expired)) if reseats < MAX_RESEATS => {
                // The server dropped the seat (an expired game server
                // session, say), or the table is gone (`No Game Data` once
                // a finished game is cleared away): take a new seat, or
                // get the game whole if it has ended.
                reseats += 1;
                set_state(&target, &view, WatchState::Reconnecting, Some(format!("{e}; taking a new seat")));
                match reseat(&gameroom, &gid, &seat).await {
                    Ok(Reseat::Live(new, how)) => {
                        server = new;
                        set_seat(&mut lock(&view), &how);
                        if let Ok(state) = server.game_state().await {
                            // The sender acts at the new seat.
                            if let (Some(sender), Some(new)) = (&sender, server.actions()) {
                                *lock(&sender.actions) = new;
                            }
                            if let Next::Stop = update(&state) {
                                after_end(gameroom, server, target, generation, view).await;
                                return;
                            }
                            continue;
                        }
                    }
                    Ok(Reseat::Finished(state)) => {
                        // The table is gone, and its chat with it.
                        update(&state);
                        close_chat(&target, &view);
                        return;
                    }
                    Err(e) => {
                        set_state(&target, &view, WatchState::Reconnecting, Some(e.message));
                    }
                }
            }
            Err(e) => {
                set_state(&target, &view, WatchState::Failed, Some(e.to_string()));
                return;
            }
        }
        if lock(&target.session).generation() != generation {
            set_state(&target, &view, WatchState::Stopped, None);
            return;
        }
        tokio::time::sleep(backoff.min(cap)).await;
        backoff = (backoff.min(cap) * 2).min(cap);
    }
}

/// The game's chat lines in a state's `chat` field.
fn chat_view(chat: &str) -> Vec<ChatLineView> {
    parse_chat(chat)
        .into_iter()
        .map(|l| ChatLineView { side: l.side, label: l.label, text: l.text })
        .collect()
}

/// The chat to show after a state with `new`: chat only grows, so a state
/// with fewer lines (a finished game's page, which has none, or a table
/// being cleared away) keeps the lines already shown.
fn kept_chat(old: &[ChatLineView], new: Vec<ChatLineView>) -> Vec<ChatLineView> {
    if new.len() < old.len() { old.to_vec() } else { new }
}

/// Marks the chat closed to the user.
fn close_chat(target: &Target, view: &Mutex<WatchView>) {
    let mut v = lock(view);
    if v.chat_open {
        v.chat_open = false;
        emit(&target.events, WATCH_UPDATE, v.clone());
    }
}

/// Once the game has ended: its permanent id, and, at the user's seat,
/// the chat (players often stay at the table to talk).
async fn after_end(
    gameroom: Arc<Gameroom>,
    server: GameServer,
    target: Target,
    generation: u64,
    view: Arc<Mutex<WatchView>>,
) {
    let chat = {
        let v = lock(&view);
        v.state == WatchState::Ended && v.chat_open
    };
    if !chat {
        close_chat(&target, &view);
        return find_id(gameroom, target, generation, view).await;
    }
    let id = find_id(gameroom, target.clone(), generation, view.clone());
    tokio::join!(id, follow_chat(server, target, generation, view));
}

/// Long-polls a finished game's table for chat, until the session moves
/// on or the server stops answering for it. The server clears finished
/// tables away (sooner, perhaps, for event games), so an error there
/// closes the chat rather than taking a new seat.
async fn follow_chat(mut server: GameServer, target: Target, generation: u64, view: Arc<Mutex<WatchView>>) {
    let mut backoff = Duration::from_secs(1);
    loop {
        if lock(&target.session).generation() != generation {
            return;
        }
        match server.update(MAXWAIT).await {
            Ok(state) => {
                backoff = Duration::from_secs(1);
                let mut v = lock(&view);
                let (chat, away) = (kept_chat(&v.chat, chat_view(&state.chat)), away(&state));
                if (&v.chat, v.away) != (&chat, away) {
                    v.chat = chat;
                    v.away = away;
                    emit(&target.events, WATCH_UPDATE, v.clone());
                }
                continue;
            }
            // A dropped connection: try again.
            Err(Error::Empty | Error::Network(_)) => {}
            Err(_) => {
                close_chat(&target, &view);
                return;
            }
        }
        tokio::time::sleep(backoff).await;
        backoff = (backoff * 2).min(NETWORK_BACKOFF);
    }
}

/// A followed game, opened again.
enum Reseat {
    Live(GameServer, Seat),
    /// It ended: the final state.
    Finished(GameState),
}

/// Opens followed game `gid` again after its seat was dropped, at the same
/// kind of `seat`. If it has ended, its final state comes from the
/// finished game. The gameroom id alone can't open that: ids of live games
/// and permanent ids are separate, so the same number may be an older
/// finished game.
async fn reseat(gameroom: &Gameroom, gid: &str, seat: &Seat) -> Result<Reseat, ApiError> {
    if let Ok(id) = gameroom.find_game_id(gid).await {
        return match gameroom.open(&id).await? {
            Opened::Finished(game) => Ok(Reseat::Finished(game.state())),
            Opened::Live(..) => {
                Err(ApiError::state(format!("game {gid} ended as {id}, which isn't finished")))
            }
        };
    }
    if let Seat::Player(side) = seat {
        return Ok(Reseat::Live(gameroom.sit(gid, *side).await?, seat.clone()));
    }
    match gameroom.open(gid).await? {
        Opened::Live(server, how) => Ok(Reseat::Live(server, Seat::Viewer(how))),
        Opened::Finished(_) => Err(ApiError::state(format!("game {gid} is no longer being played"))),
    }
}

/// Sends the user's moves ([`Session::outgoing_move`]) from their seat,
/// until the session moves on. It wakes when the session changes, and,
/// with a move out, when it's time to check on it.
///
/// The server's `ok` doesn't mean the move was played (after refusing a
/// move it drops the next ones for a few seconds, answering `ok`), and
/// a failed request may or may not have reached it. So a sent move that
/// hasn't come back in the poll's updates after [`CONFIRM_WAIT`] is
/// checked against the full state and sent again if it's still missing.
/// An error reply is a refusal: the move goes back to being a plan, and
/// the watch shows the server's message.
async fn send_moves(
    target: Target,
    generation: u64,
    view: Arc<Mutex<WatchView>>,
    actions: Arc<Mutex<Actions>>,
    wake: Arc<Notify>,
) {
    // The move last sent, how many times, and when to check on it.
    let mut sent: Option<(OutgoingMove, u32, Instant)> = None;
    // When the user's takeback request was sent, until the server shows it.
    let mut asked: Option<Instant> = None;
    loop {
        let move_due = sent.as_ref().map(|(_, _, due)| *due);
        match move_due.into_iter().chain(asked.map(|at| at + CONFIRM_WAIT)).min() {
            None => wake.notified().await,
            Some(due) => {
                let _ = tokio::time::timeout_at(due.into(), wake.notified()).await;
            }
        }
        if !send_takeback(&target, generation, &view, &actions, &mut asked).await {
            return;
        }
        let out = {
            let s = lock(&target.session);
            if s.generation() != generation {
                return;
            }
            s.outgoing_move()
        };
        let Some(out) = out else {
            sent = None;
            continue;
        };
        // Held until an opponent sits (`follow` wakes this then).
        if lock(&view).waiting {
            continue;
        }
        let tries = match &sent {
            Some((last, tries, due)) if *last == out => {
                if Instant::now() < *due {
                    continue;
                }
                // Not back yet: dropped, or the update hasn't come. The
                // full state settles which. Without it (the network is
                // down), sending again could post a move the server already
                // has, which it would refuse; check again soon instead.
                let tries = *tries;
                let act = lock(&actions).clone();
                match act.game_state().await {
                    Ok(state) => {
                        update(&target, generation, &state, &view);
                        if lock(&target.session).outgoing_move().as_ref() != Some(&out) {
                            sent = None;
                            continue;
                        }
                    }
                    Err(_) => {
                        set_state(&target, &view, WatchState::Reconnecting, Some(CONNECTION_LOST.into()));
                        sent = Some((out, tries, Instant::now() + NETWORK_BACKOFF));
                        continue;
                    }
                }
                tries
            }
            _ => 0,
        };
        let act = lock(&actions).clone();
        match act.act("move", &[("move", out.text.clone())]).await {
            Ok(_) => {
                let wait = (CONFIRM_WAIT * (tries + 1)).min(MAX_BACKOFF);
                sent = Some((out, tries + 1, Instant::now() + wait));
                set_state(&target, &view, WatchState::Following, None);
                set_refused(&target, &view, None);
            }
            Err(Error::Server(message)) => {
                {
                    let mut s = lock(&target.session);
                    if s.move_refused(generation, out.node).is_ok() {
                        emit_session(&target.events, &s, Vec::new(), None);
                    }
                }
                target.controller.poke();
                let label = notation::move_label(out.ply);
                set_state(&target, &view, WatchState::Following, None);
                set_refused(
                    &target,
                    &view,
                    Some(format!("arimaa.com refused {label} {}: {message}", out.text)),
                );
                sent = None;
            }
            Err(_) => {
                // It may have arrived; the state is checked before sending
                // it again, soon.
                set_state(&target, &view, WatchState::Reconnecting, Some(CONNECTION_LOST.into()));
                sent = Some((out, tries + 1, Instant::now() + Duration::from_secs(2)));
            }
        }
    }
}

/// Sends the user's takeback request or answer
/// ([`Session::outgoing_takeback`]), for [`send_moves`]. A request the
/// server hasn't shown [`CONFIRM_WAIT`] after it was sent (`asked`) is
/// checked against the full state, and dropped if it's still missing: the
/// user can ask again. Returns false once the session has moved on.
async fn send_takeback(
    target: &Target,
    generation: u64,
    view: &Mutex<WatchView>,
    actions: &Mutex<Actions>,
    asked: &mut Option<Instant>,
) -> bool {
    let (out, unconfirmed) = {
        let s = lock(&target.session);
        if s.generation() != generation {
            return false;
        }
        (s.outgoing_takeback(), s.takeback_unconfirmed())
    };
    if !unconfirmed {
        *asked = None;
    } else if asked.is_some_and(|at| at.elapsed() >= CONFIRM_WAIT) {
        *asked = None;
        let act = lock(actions).clone();
        if let Ok(state) = act.game_state().await {
            update(target, generation, &state, view);
        }
        let mut s = lock(&target.session);
        if s.takeback_unconfirmed() && s.takeback_failed(generation, TakebackAction::Request).is_ok() {
            emit_session(&target.events, &s, Vec::new(), None);
            drop(s);
            let detail = "arimaa.com didn't take the takeback request".to_string();
            set_state(target, view, WatchState::Following, Some(detail));
        }
    }
    let Some(out) = out else { return true };
    let (name, value, what) = match out.action {
        TakebackAction::Request => ("takeback", "req", "request"),
        TakebackAction::Reply(true) => ("takebackreply", "yes", "answer"),
        TakebackAction::Reply(false) => ("takebackreply", "no", "answer"),
    };
    let act = lock(actions).clone();
    let reply = act.act(name, &[(name, value.into())]).await;
    let mut s = lock(&target.session);
    let detail = match reply {
        Ok(_) => {
            if s.takeback_sent(generation, out.action).is_ok() && out.action == TakebackAction::Request {
                *asked = Some(Instant::now());
            }
            None
        }
        Err(Error::Server(message)) => {
            let _ = s.takeback_failed(generation, out.action);
            Some(format!("arimaa.com refused the takeback {what}: {message}"))
        }
        Err(e) => {
            let _ = s.takeback_failed(generation, out.action);
            Some(format!("sending the takeback {what} failed ({e})"))
        }
    };
    emit_session(&target.events, &s, Vec::new(), None);
    drop(s);
    // A note from before (a declined request, say) is out of date.
    set_state(target, view, WatchState::Following, detail);
    true
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

    use crate::dto::PlayerKind;

    use super::*;

    const SETUPS: &str = "1w Ra1 Rb1 Rc1 Dd1 Re1 Rf1 Dg1 Rh1 Ra2 Mb2 Cc2 Hd2 Ee2 Cf2 Hg2 Rh2%13\
        1b rh7 ra7 rh8 rg8 rf8 rc8 rb8 ra8 cf7 cc7 de8 dd8 hg7 hb7 me7 ed7%13";

    #[test]
    fn chat_history_survives_a_state_without_it() {
        let line = |t: &str| ChatLineView { side: Some(Color::Silver), label: None, text: t.into() };
        let shown = vec![line("gg"), line("thanks")];
        // A finished game's page has no chat; the lines shown stay.
        assert_eq!(kept_chat(&shown, chat_view("")), shown);
        assert_eq!(kept_chat(&shown, vec![line("gg")]), shown);
        let more = vec![line("gg"), line("thanks"), line("bye")];
        assert_eq!(kept_chat(&shown, more.clone()), more);
    }

    /// A `gamestate` reply in ASIP 1.0's `key=value` form.
    fn state(moves: &str, extra: &str) -> GameState {
        let text = format!(
            "wplayer=* bot_a\nbplayer=bot_b\nwrating=1500\nbrating=0\ntimecontrol=1/0/0/0/0\nturn=w\nmoves={SETUPS}{moves}\n\
             tcwreserve=0\ntcbreserve=0\ntimeonserver=1000\nwstartmove=990\n{extra}"
        );
        GameState::from_record(Record::decode(&text).unwrap())
    }

    #[test]
    fn a_sleep_shows_as_the_wall_clock_running_ahead() {
        let mut w = SleepWatch::new();
        let (mono, wall) = (w.mono, w.wall);
        let secs = Duration::from_secs;
        // Awake: both clocks advance together (a little jitter is fine).
        assert!(!w.slept(mono + secs(5), wall + secs(5) + Duration::from_millis(300)));
        // Asleep for a minute: the monotonic clock stood still.
        assert!(w.slept(mono + secs(10), wall + secs(70)));
        // The wall clock set back an hour isn't a sleep.
        assert!(!w.slept(mono + secs(15), wall - secs(3600)));
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
    fn a_new_login_logs_out_only_another_users_login() {
        assert!(ends_old_login(Some("alice"), "bob"));
        assert!(!ends_old_login(Some("alice"), "alice"));
        assert!(!ends_old_login(Some("Alice"), "alice"));
        assert!(!ends_old_login(None, "alice"));
    }

    #[test]
    fn the_session_becomes_the_servers_game() {
        let mut s = Session::new();
        let first = state("2w Ee2n", "");
        start(&mut s, &first);
        let g = s.generation();
        assert_eq!(apply(&mut s, g, &first).outcome, Outcome::Playing);
        let v = s.view();
        assert_eq!(v.ply, 3);
        let players = v.players.unwrap();
        assert_eq!((players.gold.name.as_str(), players.silver.name.as_str()), ("bot_a", "bot_b"));
        assert!(!v.can_take_back && !v.plays_live);
        let clock = v.clock.unwrap();
        assert_eq!(clock.turn_elapsed_ms / 1000, 10, "the turn's time comes from the server");
        assert!(s.export(false).contains("bot_a"), "the players are in the record's tags");
        assert_eq!(v.tag_ratings, [Some("1500".to_string()), None], "a 0 rating isn't one");

        // A new move animates; the same state again changes nothing.
        let next = state("2w Ee2n%132b ed7s", "");
        let Applied { animation, outcome, .. } = apply(&mut s, g, &next);
        assert!(!animation.is_empty() && outcome == Outcome::Playing);
        assert!(apply(&mut s, g, &next).animation.is_empty());
    }

    #[test]
    fn a_players_seat_makes_the_user_that_side() {
        let mut s = Session::new();
        let seated = state("2w", "role=w\n");
        start(&mut s, &seated);
        let g = s.generation();
        apply(&mut s, g, &seated);
        let players = s.view().players.unwrap();
        assert_eq!((players.gold.kind, players.silver.kind), (PlayerKind::Human, PlayerKind::Remote));
        assert_eq!(players.gold.name, "bot_a", "the user plays under their username");
        assert!(s.plays_live(), "gold's move is the user's");
        assert!(s.export(false).contains("bot_a"), "the record keeps the server's names");
    }

    #[test]
    fn a_created_game_waits_for_an_opponent() {
        let before = |extra: &str| {
            let text = format!("wplayer=me\ntimecontrol=1/0/0/0/0\nturn=w\nmoves=1w\nrole=w\n{extra}");
            GameState::from_record(Record::decode(&text).unwrap())
        };
        let open = before("");
        assert!(waiting(Some(Color::Gold), &open), "silver's seat is empty");
        assert!(!waiting(None, &open), "a viewer waits for nothing");
        let mut s = Session::new();
        start(&mut s, &open);
        let g = s.generation();
        apply(&mut s, g, &open);
        assert_eq!(s.view().players.unwrap().silver.name, "Silver");

        // The opponent sits: their name and rating come in.
        let joined = before("bplayer=them\nbrating=1600\n");
        assert!(!waiting(Some(Color::Gold), &joined));
        apply(&mut s, g, &joined);
        assert_eq!(s.view().players.unwrap().silver.name, "them");
        let record = s.export(false);
        assert!(
            record.contains("[Silver \"them\"]") && record.contains("[SilverRating \"1600\"]"),
            "{record}"
        );
    }

    #[test]
    fn players_away_from_the_table() {
        let st = |extra: &str| {
            let text = format!("wplayer=a\nbplayer=b\nturn=w\nmoves=1w\n{extra}");
            GameState::from_record(Record::decode(&text).unwrap())
        };
        assert_eq!(away(&st("wpresent=10\nbpresent=0\n")), [false, true], "left");
        assert_eq!(away(&st("wpresent=10\n")), [false, true], "never sat");
        assert_eq!(away(&st("")), [false, false], "not reported");
    }

    #[test]
    fn a_game_with_no_time_limit_has_no_clocks() {
        let text = "wplayer=a\nbplayer=b\ntimecontrol=0/0/0/0/0\npostal=1\nturn=w\nmoves=1w\n";
        let mut s = Session::new();
        start(&mut s, &GameState::from_record(Record::decode(text).unwrap()));
        assert!(s.view().clock.is_none());
    }

    #[test]
    fn server_takebacks_go_back() {
        let mut s = Session::new();
        let first = state("2w Ee2n%132b ed7s%133w", "");
        start(&mut s, &first);
        let g = s.generation();
        apply(&mut s, g, &first);
        assert_eq!(s.view().ply, 4);

        // Gold asks on its turn: silver's move and gold's go back.
        let back = state("2w Ee2n%132b ed7s%133w takeback%132b takeback%132w", "");
        assert_eq!(apply(&mut s, g, &back).outcome, Outcome::Playing);
        assert_eq!(s.view().ply, 2);
        let again = state("2w Ee2n%132b ed7s%133w takeback%132b takeback%132w Ee2n Ee3n%132b", "");
        apply(&mut s, g, &again);
        assert_eq!(s.view().ply, 3);
        let record = s.export(false);
        assert!(record.contains("Ee3n") && record.contains("ed7s"), "the old moves stay: {record}");
    }

    #[test]
    fn takeback_requests_follow_the_server() {
        let mut s = Session::new();
        let first = state("2w Ee2n%132b ed7s%133w", "role=w\n");
        start(&mut s, &first);
        let g = s.generation();
        apply(&mut s, g, &first);
        s.request_takeback().unwrap();
        assert_eq!(s.outgoing_takeback().unwrap().action, TakebackAction::Request);
        s.takeback_sent(g, TakebackAction::Request).unwrap();
        let asked = state("2w Ee2n%132b ed7s%133w", "role=w\ntakeback=w 3w\n");
        assert_eq!(apply(&mut s, g, &asked).takeback, None);
        assert!(s.view().takeback.unwrap().shown);
        // Silver says no: the request is gone, and the moves stay.
        let applied = apply(&mut s, g, &state("2w Ee2n%132b ed7s%133w", "role=w\ndenied_w=1\n"));
        assert_eq!(applied.takeback, Some(TakebackEnd::Declined));
        let v = s.view();
        assert_eq!((v.takeback, v.live_ply), (None, Some(4)));

        // Silver asks, and the user agrees.
        let asked = state("2w Ee2n%132b ed7s%133w", "role=w\ntakeback=b 3w\n");
        apply(&mut s, g, &asked);
        s.answer_takeback(true).unwrap();
        assert_eq!(s.outgoing_takeback().unwrap().action, TakebackAction::Reply(true));
        s.takeback_sent(g, TakebackAction::Reply(true)).unwrap();
        let back = state("2w Ee2n%132b ed7s%133w takeback%132b", "role=w\nturn=b\n");
        assert_eq!(apply(&mut s, g, &back).takeback, None, "a takeback, not a refusal");
        let v = s.view();
        assert_eq!((v.takeback, v.live_ply), (None, Some(3)));
    }

    #[test]
    fn the_servers_result_ends_the_game() {
        let mut s = Session::new();
        let ended = state("2w Ee2n", "result=b\nreason=t\nfinishedId=671437\n");
        start(&mut s, &ended);
        let g = s.generation();
        assert_eq!(apply(&mut s, g, &ended).outcome, Outcome::Ended);
        assert!(s.export(false).contains("[GameId \"671437\"]"), "{}", s.export(false));
        let result = GameResult { winner: Color::Silver, reason: WinReason::Timeout };
        assert_eq!(s.view().result, Some(result));
    }

    #[test]
    fn recent_games_are_shown() {
        let r = Record::decode(
            r#"{"recentgames":[{"id":"671441","wusername":"a","busername":"b","result":"b",
                "termination":"t","plycount":"6","endts":"1791110910","rated":"1"}]}"#,
        )
        .unwrap();
        let game = RecentGame::from_record(&r.list("recentgames")[0]).unwrap();
        let v = recent_view(game);
        assert_eq!(v.result, Some(GameResult { winner: Color::Silver, reason: WinReason::Timeout }));
        assert_eq!((v.moves, v.ended_ms, v.rated), (Some(6), Some(1_791_110_910_000), true));
    }

    #[test]
    fn a_state_that_doesnt_fit_fails() {
        let mut s = Session::new();
        let first = state("2w Ee2n", "");
        start(&mut s, &first);
        let g = s.generation();
        apply(&mut s, g, &first);
        let illegal = state("2w Ee2n%132b Ee3n", "");
        assert!(matches!(apply(&mut s, g, &illegal).outcome, Outcome::Failed(_)));
        let unknown = state("2w Ee2n", "result=wz\n");
        assert!(matches!(apply(&mut s, g, &unknown).outcome, Outcome::Failed(e) if e.contains("\"wz\"")));
    }
}
