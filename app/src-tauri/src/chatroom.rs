//! The gameroom's chat room (`howdah_gameroom::chat`): entering, a task
//! that keeps asking the room for new events while the user is in it,
//! saying something, and leaving.
//!
//! The room times out a user who hasn't asked for a minute, so the task
//! asks at least every [`BACKGROUND_POLL`] while in the room, more often
//! while its panel is shown ([`ChatRoom::set_active`]) and the room is
//! busy. The room's own page asks every second, slowing to ten; this asks
//! less often, to spare the server.
//!
//! Community norm on arimaa.com is to leave the chat room while playing a
//! rated game between people that isn't postal, or any event game. With
//! auto-leave on (the default; the frontend sets it from the settings),
//! the user leaves the room when one of their games turns out to be such
//! a game ([`ChatRoom::game_seen`]), once per game: entering again during
//! the game is the user's call.

use std::collections::{HashSet, VecDeque};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use howdah_gameroom::{
    ChatClient, ChatEvent, ChatKind, Fetched, GameState, MessagePart, Role, format_message,
};
use tokio::sync::Notify;

use crate::backend::{Events, emit};
use crate::chat_archive::SharedArchive;
use crate::dto::{ApiError, ChatEventKind, ChatEventView, ChatPartView, ChatRoomState, ChatRoomView};

/// Event carrying a [`ChatRoomView`].
pub const CHAT_UPDATE: &str = "chatroom://changed";

/// How many of the log's events are kept.
const MAX_EVENTS: usize = 400;

/// How often the room is asked while its panel is shown and something
/// happened there in the last [`BUSY_FOR`], while it's shown otherwise,
/// and while it isn't. The room times a user out after about a minute
/// without a request.
const BUSY_POLL: Duration = Duration::from_secs(5);
const ACTIVE_POLL: Duration = Duration::from_secs(10);
const BACKGROUND_POLL: Duration = Duration::from_secs(30);
const BUSY_FOR: Duration = Duration::from_secs(120);

/// The longest pause after failed requests, kept under the room's timeout.
const MAX_BACKOFF: Duration = Duration::from_secs(40);

/// The longest message sent; the room sets no limit, but nobody needs more.
const MAX_MESSAGE: usize = 2000;

fn lock<T>(m: &Mutex<T>) -> std::sync::MutexGuard<'_, T> {
    m.lock().unwrap_or_else(|e| e.into_inner())
}

/// Whether the community norm calls for leaving the chat room for game
/// `state` at the user's seat: an event game, or a rated game between two
/// people that isn't postal. The opponent has to have sat down to tell.
/// A player is a bot by the `bot_` name arimaa.com's bot accounts have,
/// or when `bots` (the lobby's flagged ones, lowercase) has them.
pub fn leaves_chat_for(state: &GameState, bots: &HashSet<String>) -> bool {
    if !matches!(state.role, Some(Role::Player(_))) || state.result.is_some() {
        return false;
    }
    let event = state.raw.nonempty("event").filter(|e| !e.trim().eq_ignore_ascii_case("casual game"));
    let people = state.players.iter().all(|p| p.as_deref().is_some_and(|name| !is_bot(name, bots)));
    event.is_some() || (state.rated && !state.postal && people)
}

fn is_bot(name: &str, bots: &HashSet<String>) -> bool {
    name.get(..4).is_some_and(|p| p.eq_ignore_ascii_case("bot_")) || bots.contains(&name.to_lowercase())
}

struct Inner {
    state: ChatRoomState,
    client: Option<ChatClient>,
    task: Option<tauri::async_runtime::JoinHandle<()>>,
    /// Bumped on every enter and leave, so a stale task's results are
    /// dropped.
    generation: u64,
    pointer: Option<String>,
    events: VecDeque<ChatEventView>,
    next_id: u32,
    users: Vec<String>,
    count: Option<u32>,
    note: Option<String>,
    active: bool,
    last_activity: Instant,
    auto_leave: bool,
    /// The games already checked for leaving, by gameroom id.
    games_seen: HashSet<String>,
}

/// The user's place in the gameroom's chat room.
pub struct ChatRoom {
    events: Events,
    /// Where what the user sees in the room is kept.
    archive: SharedArchive,
    inner: Mutex<Inner>,
    wake: Arc<Notify>,
}

impl ChatRoom {
    pub fn new(events: Events, archive: SharedArchive) -> ChatRoom {
        ChatRoom {
            events,
            archive,
            inner: Mutex::new(Inner {
                state: ChatRoomState::Out,
                client: None,
                task: None,
                generation: 0,
                pointer: None,
                events: VecDeque::new(),
                next_id: 0,
                users: Vec::new(),
                count: None,
                note: None,
                active: false,
                last_activity: Instant::now(),
                auto_leave: true,
                games_seen: HashSet::new(),
            }),
            wake: Arc::new(Notify::new()),
        }
    }

    pub fn view(&self) -> ChatRoomView {
        view(&lock(&self.inner))
    }

    fn changed(&self, inner: &Inner) {
        emit(&self.events, CHAT_UPDATE, view(inner));
    }

    /// Enters the room with `client`, unless the user is in it already.
    pub fn enter(self: &Arc<Self>, client: ChatClient) -> ChatRoomView {
        let mut inner = lock(&self.inner);
        if inner.state != ChatRoomState::Out
            && inner.client.as_ref().is_some_and(|c| c.user() == client.user())
        {
            return view(&inner);
        }
        if let Some(task) = inner.task.take() {
            task.abort();
        }
        inner.generation += 1;
        inner.state = ChatRoomState::Entering;
        inner.pointer = None;
        inner.events.clear();
        inner.users.clear();
        inner.note = None;
        inner.last_activity = Instant::now();
        inner.client = Some(client.clone());
        let task = tauri::async_runtime::spawn(run(self.clone(), client, inner.generation));
        inner.task = Some(task);
        self.changed(&inner);
        view(&inner)
    }

    /// Leaves the room (saying so to it), with `note` saying why when the
    /// user didn't ask to.
    pub async fn leave(&self, note: Option<String>) -> Result<(), ApiError> {
        let client = {
            let mut inner = lock(&self.inner);
            if let Some(task) = inner.task.take() {
                task.abort();
            }
            inner.generation += 1;
            let was_in = inner.state != ChatRoomState::Out;
            inner.state = ChatRoomState::Out;
            inner.users.clear();
            if was_in || note.is_none() {
                inner.note = note;
            }
            self.changed(&inner);
            inner.client.take().filter(|_| was_in)
        };
        match client {
            Some(c) => {
                c.leave().await.map_err(|e| ApiError::state(format!("couldn't tell the chat room: {e}")))
            }
            None => Ok(()),
        }
    }

    /// Says `text` in `color` (`rrggbb`). The line comes back with the
    /// fetch that follows at once.
    pub async fn say(&self, text: &str, color: &str) -> Result<(), ApiError> {
        let text = text.split_whitespace().collect::<Vec<_>>().join(" ");
        if text.is_empty() {
            return Err(ApiError::illegal("type a message first"));
        }
        if text.chars().count() > MAX_MESSAGE {
            return Err(ApiError::illegal(format!(
                "that's too long for the chat room ({MAX_MESSAGE} at most)"
            )));
        }
        let client = {
            let inner = lock(&self.inner);
            match (&inner.client, inner.state) {
                (Some(c), ChatRoomState::In) => c.clone(),
                _ => return Err(ApiError::state("enter the chat room first")),
            }
        };
        client.say(color, &text).await.map_err(|e| ApiError::state(format!("couldn't send it: {e}")))?;
        lock(&self.inner).last_activity = Instant::now();
        self.wake.notify_one();
        Ok(())
    }

    /// Whether the room's panel is in view, which makes the task ask more
    /// often.
    pub fn set_active(&self, active: bool) {
        let mut inner = lock(&self.inner);
        if inner.active != active {
            inner.active = active;
            if active {
                self.wake.notify_one();
            }
        }
    }

    /// Whether to leave the room for games that call for it.
    pub fn set_auto_leave(&self, on: bool) {
        let mut inner = lock(&self.inner);
        if inner.auto_leave != on {
            inner.auto_leave = on;
            self.changed(&inner);
        }
    }

    /// The lobby's count of people in the room.
    pub fn set_count(&self, count: Option<u32>) {
        let mut inner = lock(&self.inner);
        if inner.count != count {
            inner.count = count;
            self.changed(&inner);
        }
    }

    /// A state of game `gid` at one of the user's seats: leaves the room
    /// if the game calls for it ([`leaves_chat_for`]) and auto-leave is
    /// on, the first time the game does.
    pub fn game_seen(self: &Arc<Self>, gid: &str, state: &GameState, bots: &HashSet<String>) {
        if !leaves_chat_for(state, bots) {
            return;
        }
        let leave = {
            let mut inner = lock(&self.inner);
            inner.games_seen.insert(gid.to_string()) && inner.auto_leave && inner.state != ChatRoomState::Out
        };
        if leave {
            let opponent = match state.role {
                Some(Role::Player(side)) => state.players[side.opponent().index()].clone(),
                _ => None,
            };
            let what = match state.raw.nonempty("event").filter(|e| !e.eq_ignore_ascii_case("casual game")) {
                Some(event) => format!("your {event} game"),
                None => "your rated game".into(),
            };
            let note = match opponent {
                Some(o) => format!("Left the chat room for {what} against {o}."),
                None => format!("Left the chat room for {what}."),
            };
            let room = self.clone();
            tauri::async_runtime::spawn(async move {
                let _ = room.leave(Some(note)).await;
            });
        }
    }

    /// When the task should ask next, from now.
    fn interval(&self) -> Duration {
        let inner = lock(&self.inner);
        match (inner.active, inner.last_activity.elapsed() < BUSY_FOR) {
            (true, true) => BUSY_POLL,
            (true, false) => ACTIVE_POLL,
            (false, _) => BACKGROUND_POLL,
        }
    }
}

fn view(inner: &Inner) -> ChatRoomView {
    ChatRoomView {
        state: inner.state,
        user: inner
            .client
            .as_ref()
            .filter(|_| inner.state != ChatRoomState::Out)
            .map(|c| c.user().to_string()),
        users: inner.users.clone(),
        count: inner.count,
        events: inner.events.iter().cloned().collect(),
        note: inner.note.clone(),
        auto_leave: inner.auto_leave,
    }
}

/// Keeps asking the room for new events, as `generation`'s visit.
async fn run(room: Arc<ChatRoom>, client: ChatClient, generation: u64) {
    let mut backoff = Duration::from_secs(5);
    // This visit in the local archive: the log arrives unbroken from the
    // first reply's backlog on, so the visit covers until its last fetch.
    let mut visit: Option<u64> = None;
    loop {
        let pointer = {
            let inner = lock(&room.inner);
            if inner.generation != generation {
                return;
            }
            inner.pointer.clone()
        };
        let fetched = client.fetch(pointer.as_deref()).await;
        let mut wait = room.interval();
        match fetched {
            Ok(fetched) => {
                backoff = Duration::from_secs(5);
                let (ask_users, taken) = {
                    let mut inner = lock(&room.inner);
                    if inner.generation != generation {
                        return;
                    }
                    let entered = inner.state == ChatRoomState::Entering;
                    let before = (inner.next_id, inner.note.take());
                    inner.state = ChatRoomState::In;
                    let (comings, taken) = add_events(&mut inner, fetched);
                    // Most fetches bring nothing new.
                    if entered || before != (inner.next_id, None) {
                        room.changed(&inner);
                    }
                    (entered || comings, taken)
                };
                match visit {
                    Some(id) => room.archive.live_extend(id, &taken),
                    None if !taken.is_empty() => visit = Some(room.archive.live_begin(&taken)),
                    None => {}
                }
                if ask_users && let Ok(users) = client.users().await {
                    let mut inner = lock(&room.inner);
                    if inner.generation != generation {
                        return;
                    }
                    if inner.users != users {
                        inner.users = users;
                        room.changed(&inner);
                    }
                }
            }
            Err(e) => {
                let mut inner = lock(&room.inner);
                if inner.generation != generation {
                    return;
                }
                if inner.state == ChatRoomState::Entering {
                    // The first request failed: the user isn't in.
                    inner.state = ChatRoomState::Out;
                    inner.client = None;
                    inner.task = None;
                    inner.note = Some(format!("Couldn't enter the chat room: {e}"));
                    room.changed(&inner);
                    return;
                }
                inner.note = Some(format!("Can't reach the chat room ({e}); retrying"));
                room.changed(&inner);
                wait = backoff;
                backoff = (backoff * 2).min(MAX_BACKOFF);
            }
        }
        tokio::select! {
            () = tokio::time::sleep(wait) => {}
            () = room.wake.notified() => {}
        }
    }
}

/// Adds a fetch's events to the log, unless its pointer is no newer than
/// the last (as the room's own client does). Returns whether anyone came
/// or went, and the events taken.
fn add_events(inner: &mut Inner, fetched: Fetched) -> (bool, Vec<ChatEvent>) {
    let Fetched::Events { pointer, events } = fetched else { return (false, Vec::new()) };
    let newer =
        match (inner.pointer.as_deref().and_then(|p| p.parse::<u64>().ok()), pointer.parse::<u64>().ok()) {
            (Some(old), Some(new)) => new > old,
            _ => true,
        };
    if !newer {
        return (false, Vec::new());
    }
    inner.pointer = Some(pointer);
    let mut comings = false;
    if !events.is_empty() {
        inner.last_activity = Instant::now();
    }
    for e in &events {
        comings |= !matches!(e.kind, ChatKind::Message { .. });
        let id = inner.next_id;
        inner.next_id += 1;
        inner.events.push_back(event_view(id, e.clone()));
    }
    while inner.events.len() > MAX_EVENTS {
        inner.events.pop_front();
    }
    (comings, events)
}

pub(crate) fn event_view(id: u32, e: ChatEvent) -> ChatEventView {
    let (kind, color, emote, parts) = match e.kind {
        ChatKind::In => (ChatEventKind::In, None, false, Vec::new()),
        ChatKind::Out => (ChatEventKind::Out, None, false, Vec::new()),
        ChatKind::Timeout => (ChatEventKind::Timeout, None, false, Vec::new()),
        ChatKind::Message { color, text } => {
            let f = format_message(&text);
            (ChatEventKind::Message, color, f.emote, f.parts.into_iter().map(part_view).collect())
        }
    };
    ChatEventView { id, time: e.time, user: e.user, kind, color, emote, parts }
}

fn part_view(p: MessagePart) -> ChatPartView {
    match p {
        MessagePart::Text(text) => ChatPartView::Text { text },
        MessagePart::Link(url) => ChatPartView::Link { url },
        MessagePart::Face { name, symbol } => ChatPartView::Face { name: name.into(), symbol: symbol.into() },
        MessagePart::Piece { color, kind } => ChatPartView::Piece { color, piece: kind },
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use howdah_gameroom::Record;

    fn archive() -> SharedArchive {
        Arc::new(crate::chat_archive::ChatArchive::new(Arc::new(NullSink)))
    }

    struct NullSink;
    impl crate::backend::EventSink for NullSink {
        fn emit(&self, _: &str, _: serde_json::Value) {}
    }

    fn state(json: &str) -> GameState {
        GameState::from_record(Record::decode(json).unwrap())
    }

    #[test]
    fn games_to_leave_the_room_for() {
        let bots = HashSet::from(["sharpie".to_string()]);
        let leaves = |s: &GameState| leaves_chat_for(s, &bots);
        let game = |fields: &str| {
            state(&format!(
                r#"{{"role":"w","wplayer":"Janzert","bplayer":"Fritzlein","timecontrol":"30s/5m","rated":"1","postal":"0",{fields}"seat":"1"}}"#
            ))
        };
        // A rated game between people.
        assert!(leaves(&game("")));
        // Unrated, postal, against a bot, or not yet with an opponent.
        assert!(!leaves(&game(r#""rated":"0","#)));
        assert!(!leaves(&game(r#""postal":"1","#)));
        assert!(!leaves(&game(r#""bplayer":"bot_Sharp","#)));
        assert!(!leaves(&game(r#""bplayer":"Sharpie","#)));
        assert!(!leaves(&game(r#""bplayer":"","#)));
        // Watching.
        assert!(!leaves(&game(r#""role":"v","#)));
        // Any event game, postal and unrated ones too; "Casual game" isn't one.
        assert!(leaves(&game(r#""rated":"0","postal":"1","event":"Postal Mixer","#)));
        assert!(!leaves(&game(r#""rated":"0","event":"Casual game","#)));
    }

    /// As the app's exit hook leaves the room: from outside any runtime,
    /// with the timeout made inside the one `block_on` uses.
    #[test]
    fn leaving_on_exit_runs_outside_a_runtime() {
        let room = ChatRoom::new(Arc::new(NullSink), archive());
        let left = tauri::async_runtime::block_on(async {
            tokio::time::timeout(Duration::from_secs(2), room.leave(None)).await
        });
        assert!(matches!(left, Ok(Ok(()))));
    }

    #[test]
    fn events_arrive_once_and_in_order() {
        let room = ChatRoom::new(Arc::new(NullSink), archive());
        let mut inner = lock(&room.inner);
        let fetched = |pointer: &str, lines: &str| {
            howdah_gameroom::chat::parse_fetch(&format!("OK\n{pointer}\n{lines}")).unwrap()
        };
        assert!(add_events(&mut inner, fetched("100", "1\ta\tin\n2\ta\tmsg\t000000\t/me waves :)\n")).0);
        assert!(!add_events(&mut inner, fetched("200", "3\ta\tmsg\tffffff\tspoiler\n")).0);
        // A reply no newer than the last is dropped.
        assert_eq!(add_events(&mut inner, fetched("150", "9\tb\tin\n")), (false, Vec::new()));
        let ids: Vec<u32> = inner.events.iter().map(|e| e.id).collect();
        assert_eq!(ids, [0, 1, 2]);
        let wave = &inner.events[1];
        assert!(wave.emote);
        assert_eq!(
            wave.parts,
            [
                ChatPartView::Text { text: " waves ".into() },
                ChatPartView::Face { name: "smiley".into(), symbol: ":)".into() }
            ]
        );
        assert_eq!(inner.events[2].color.as_deref(), Some("ffffff"));
    }
}
