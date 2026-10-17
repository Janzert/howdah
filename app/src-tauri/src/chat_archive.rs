//! The gameroom chat room's log, kept locally: what the archive gave
//! (`archsrv.php`, no login needed) and what the user saw in the room.
//!
//! The store is a set of chunks, each a stretch of time it covers in full
//! (`start..end`, Unix seconds, end exclusive) and the events in it:
//!
//! - An archive request gives the 24 hours from any moment, so a day is
//!   filled by asking from the start of each stretch of it not yet
//!   covered ([`ChatArchive::fetch_day`]). Today is covered only up to
//!   when it was asked. Past days don't change, so a covered stretch is
//!   never asked for again.
//! - A visit to the room is a chunk too ([`ChatArchive::live_begin`],
//!   [`ChatArchive::live_extend`]): the room's first reply is the tail of
//!   its log (the last few days, cut to a size), and each later fetch
//!   brings everything since, so the visit covers from just after the
//!   backlog's first event until its last successful fetch.
//!
//! Where chunks overlap, an archive request is read over a visit (it's
//! the server's own record), and otherwise the one updated last;
//! events outside every chunk's span (the first second of a backlog) are
//! merged in without duplicates. Days are the computer's local days, as
//! on the archive's own page.
//!
//! On disk (the app's local data dir, `chat-archive/`): `index.json`
//! lists the chunks, and `chunks/<id>.tsv` holds each one's events as
//! log lines. A live chunk's file is appended to as events arrive.

use std::collections::HashSet;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use chrono::{Datelike, Local, NaiveDate, TimeZone};
use howdah_gameroom::{ChatEvent, DEFAULT_CHAT, Http, chat};
use serde::{Deserialize, Serialize};

use crate::backend::{Events, emit};
use crate::chatroom::event_view;
use crate::dto::{
    ApiError, ChatArchiveChanged, ChatArchiveDay, ChatArchiveMonth, ChatCoverage, ChatDaySummary,
};

/// Event carrying a [`ChatArchiveChanged`].
pub const ARCHIVE_CHANGED: &str = "chatarchive://changed";

/// A day of the archive: what one request gives.
const DAY: i64 = 86_400;

/// How far behind the computer's clock a fetch's coverage ends, for the
/// difference between it and the server's.
const CLOCK_MARGIN: i64 = 5;

/// The most archive requests one day may take (a day with a gap at each
/// end, or a 25-hour day).
const MAX_REQUESTS: usize = 3;

/// A gap at the end of today shorter than this isn't asked for.
const RECENT: i64 = 30;

/// How often a live chunk's end is saved when no events arrive.
const SAVE_EVERY: Duration = Duration::from_secs(60);

fn lock<T>(m: &Mutex<T>) -> std::sync::MutexGuard<'_, T> {
    m.lock().unwrap_or_else(|e| e.into_inner())
}

/// The chat room's directory: arimaa.com's, or in debug builds a
/// stand-in room's (`HOWDAH_CHAT_URL`), to check the app without a
/// public visit.
pub fn chat_base() -> String {
    let stand_in = if cfg!(debug_assertions) { std::env::var("HOWDAH_CHAT_URL").ok() } else { None };
    stand_in.unwrap_or_else(|| DEFAULT_CHAT.to_string())
}

fn now() -> i64 {
    chrono::Utc::now().timestamp()
}

#[derive(Clone, Debug, Serialize, Deserialize)]
struct ChunkInfo {
    id: u64,
    start: i64,
    end: i64,
    /// When it was last added to (Unix seconds): among chunks of one
    /// kind, the newest wins where they overlap.
    updated: i64,
    /// A visit to the room rather than an archive request. Where both
    /// cover a stretch, the archive (the server's own record) wins.
    #[serde(default)]
    live: bool,
}

#[derive(Default, Serialize, Deserialize)]
struct Index {
    next_id: u64,
    chunks: Vec<ChunkInfo>,
}

struct Chunk {
    info: ChunkInfo,
    /// In log order (by time).
    events: Vec<ChatEvent>,
}

impl Chunk {
    /// Its events with times in `a..b`.
    fn between(&self, a: i64, b: i64) -> &[ChatEvent] {
        let from = self.events.partition_point(|e| e.time < a);
        let to = self.events.partition_point(|e| e.time < b);
        &self.events[from..to]
    }
}

#[derive(Default)]
struct Store {
    dir: Option<PathBuf>,
    next_id: u64,
    chunks: Vec<Chunk>,
    saved: Option<Instant>,
}

impl Store {
    fn load(dir: &Path) -> Store {
        let index: Index = std::fs::read_to_string(dir.join("index.json"))
            .ok()
            .and_then(|t| serde_json::from_str(&t).ok())
            .unwrap_or_default();
        let chunks = index
            .chunks
            .into_iter()
            .filter_map(|info| {
                let text = std::fs::read_to_string(chunk_path(dir, info.id)).ok()?;
                let mut events: Vec<ChatEvent> = text.lines().filter_map(chat::parse_event).collect();
                events.sort_by_key(|e| e.time);
                Some(Chunk { info, events })
            })
            .collect();
        Store { dir: Some(dir.to_path_buf()), next_id: index.next_id, chunks, saved: None }
    }

    fn save_index(&mut self) {
        self.saved = Some(Instant::now());
        let Some(dir) = &self.dir else { return };
        let index =
            Index { next_id: self.next_id, chunks: self.chunks.iter().map(|c| c.info.clone()).collect() };
        let Ok(text) = serde_json::to_string_pretty(&index) else { return };
        // Written whole and renamed, so a crash leaves the old index.
        let tmp = dir.join("index.json.tmp");
        if std::fs::write(&tmp, text).is_ok() {
            let _ = std::fs::rename(tmp, dir.join("index.json"));
        }
    }

    fn append(&self, id: u64, events: &[ChatEvent]) {
        let Some(dir) = &self.dir else { return };
        if events.is_empty() {
            return;
        }
        let _ = std::fs::create_dir_all(dir.join("chunks"));
        let file = std::fs::OpenOptions::new().create(true).append(true).open(chunk_path(dir, id));
        if let Ok(mut f) = file {
            let text: String = events.iter().map(|e| e.to_line() + "\n").collect();
            let _ = f.write_all(text.as_bytes());
        }
    }

    fn add(&mut self, start: i64, end: i64, live: bool, events: Vec<ChatEvent>) -> u64 {
        let id = self.next_id;
        self.next_id += 1;
        self.append(id, &events);
        let mut events = events;
        events.sort_by_key(|e| e.time);
        self.chunks.push(Chunk { info: ChunkInfo { id, start, end, updated: now(), live }, events });
        self.save_index();
        id
    }

    /// The covered stretches of `a..b`, merged, in order.
    fn covered(&self, a: i64, b: i64) -> Vec<(i64, i64)> {
        let mut spans: Vec<(i64, i64)> = self
            .chunks
            .iter()
            .map(|c| (c.info.start.max(a), c.info.end.min(b)))
            .filter(|(s, e)| s < e)
            .collect();
        spans.sort();
        let mut merged: Vec<(i64, i64)> = Vec::new();
        for (s, e) in spans {
            match merged.last_mut() {
                Some(last) if s <= last.1 => last.1 = last.1.max(e),
                _ => merged.push((s, e)),
            }
        }
        merged
    }

    /// The stretches of `a..b` not covered.
    fn gaps(&self, a: i64, b: i64) -> Vec<(i64, i64)> {
        let mut gaps = Vec::new();
        let mut at = a;
        for (s, e) in self.covered(a, b) {
            if s > at {
                gaps.push((at, s));
            }
            at = at.max(e);
        }
        if at < b {
            gaps.push((at, b));
        }
        gaps
    }

    /// The events in `a..b`: each covered stretch from the chunk that
    /// covers it (an archive request over a visit, then the newest), the
    /// rest from every chunk without duplicates.
    fn events(&self, a: i64, b: i64) -> Vec<ChatEvent> {
        let mut cuts = vec![a, b];
        for c in &self.chunks {
            cuts.extend([c.info.start, c.info.end].into_iter().filter(|t| *t > a && *t < b));
        }
        cuts.sort_unstable();
        cuts.dedup();
        let mut out = Vec::new();
        for w in cuts.windows(2) {
            let (x, y) = (w[0], w[1]);
            let best = self
                .chunks
                .iter()
                .filter(|c| c.info.start <= x && c.info.end >= y)
                .max_by_key(|c| (!c.info.live, c.info.updated, c.info.id));
            match best {
                Some(c) => out.extend_from_slice(c.between(x, y)),
                None => {
                    let mut seen = HashSet::new();
                    let mut loose: Vec<&ChatEvent> = self
                        .chunks
                        .iter()
                        .flat_map(|c| c.between(x, y))
                        .filter(|e| seen.insert(e.to_line()))
                        .collect();
                    loose.sort_by_key(|e| e.time);
                    out.extend(loose.into_iter().cloned());
                }
            }
        }
        out
    }
}

fn chunk_path(dir: &Path, id: u64) -> PathBuf {
    dir.join("chunks").join(format!("{id}.tsv"))
}

/// A local day's start and end (Unix seconds); a day can be 23 or 25
/// hours long.
fn day_bounds(date: NaiveDate) -> (i64, i64) {
    let start = |d: NaiveDate| {
        let midnight = d.and_hms_opt(0, 0, 0).expect("midnight");
        Local
            .from_local_datetime(&midnight)
            .earliest()
            .map_or_else(|| midnight.and_utc().timestamp(), |t| t.timestamp())
    };
    (start(date), start(date.succ_opt().unwrap_or(date)))
}

fn today() -> NaiveDate {
    Local::now().date_naive()
}

/// The gameroom chat room's log as kept here.
pub struct ChatArchive {
    store: Mutex<Store>,
    http: Http,
    events: Events,
    /// One day's requests at a time.
    fetching: tokio::sync::Mutex<()>,
}

impl ChatArchive {
    /// An archive kept only in memory until [`set_dir`](Self::set_dir).
    pub fn new(events: Events) -> ChatArchive {
        let http = Http::new(&howdah_gameroom::user_agent(), None).expect("an HTTP client");
        ChatArchive {
            store: Mutex::new(Store::default()),
            http,
            events,
            fetching: tokio::sync::Mutex::new(()),
        }
    }

    /// Keeps the archive in `dir`, loading what's there.
    pub fn set_dir(&self, dir: Option<PathBuf>) {
        *lock(&self.store) = dir.as_deref().map(Store::load).unwrap_or_default();
    }

    fn changed(&self, start: i64, end: i64) {
        emit(&self.events, ARCHIVE_CHANGED, ChatArchiveChanged { start, end });
    }

    /// Starts keeping a visit to the room, from its first reply's events.
    pub fn live_begin(&self, backlog: &[ChatEvent]) -> u64 {
        let end = now() - CLOCK_MARGIN;
        let start = backlog.first().map_or(end, |e| e.time + 1).min(end);
        let id = lock(&self.store).add(start, end, true, backlog.to_vec());
        self.changed(backlog.first().map_or(start, |e| e.time), end);
        id
    }

    /// Adds a fetch's events to visit `id`, which now covers until now.
    pub fn live_extend(&self, id: u64, events: &[ChatEvent]) {
        let end = now() - CLOCK_MARGIN;
        let changed = {
            let mut store = lock(&self.store);
            store.append(id, events);
            let Some(chunk) = store.chunks.iter_mut().find(|c| c.info.id == id) else { return };
            let from = chunk.info.end;
            chunk.events.extend_from_slice(events);
            chunk.info.end = chunk.info.end.max(end);
            chunk.info.updated = now();
            if !events.is_empty() || store.saved.is_none_or(|t| t.elapsed() > SAVE_EVERY) {
                store.save_index();
            }
            (!events.is_empty()).then(|| (events.iter().map(|e| e.time).min().unwrap_or(from).min(from), end))
        };
        if let Some((start, end)) = changed {
            self.changed(start, end);
        }
    }

    /// How each day of `year`'s `month` stands in the store.
    pub fn month(&self, year: i32, month: u32) -> Result<ChatArchiveMonth, ApiError> {
        let first =
            NaiveDate::from_ymd_opt(year, month, 1).ok_or_else(|| ApiError::illegal("no such month"))?;
        let store = lock(&self.store);
        let (today, now) = (today(), now());
        let days = first
            .iter_days()
            .take_while(|d| d.month() == month)
            .map(|date| summary(&store, date, today, now))
            .collect();
        Ok(ChatArchiveMonth { year, month, days })
    }

    /// Day `date` (`YYYY-MM-DD`) from the store, after asking the archive
    /// for what it lacks when `fetch`.
    pub async fn day(&self, date: &str, fetch: bool) -> Result<ChatArchiveDay, ApiError> {
        let date = NaiveDate::parse_from_str(date, "%Y-%m-%d")
            .map_err(|_| ApiError::illegal(format!("{date:?} isn't a date (YYYY-MM-DD)")))?;
        if fetch && date <= today() {
            self.fetch_day(date).await?;
        }
        let store = lock(&self.store);
        let (start, end) = day_bounds(date);
        let s = summary(&store, date, today(), now());
        let events =
            store.events(start, end).into_iter().enumerate().map(|(i, e)| event_view(i as u32, e)).collect();
        Ok(ChatArchiveDay { summary: s, start, end, events })
    }

    /// Asks the archive for the stretches of `date` not yet covered.
    async fn fetch_day(&self, date: NaiveDate) -> Result<(), ApiError> {
        let _one = self.fetching.lock().await;
        let (start, end) = day_bounds(date);
        for _ in 0..MAX_REQUESTS {
            let until = end.min(now() - CLOCK_MARGIN);
            let gap = lock(&self.store).gaps(start, until).into_iter().next();
            let Some((from, to)) = gap else { return Ok(()) };
            if to - from < RECENT && to == until && until < end {
                return Ok(());
            }
            let events = chat::archive(&self.http, &chat_base(), from)
                .await
                .map_err(|e| ApiError::state(format!("couldn't get the archive from arimaa.com: {e}")))?;
            let covered_to = (from + DAY).min(now() - CLOCK_MARGIN);
            lock(&self.store).add(from, covered_to, false, events);
            self.changed(from, covered_to);
        }
        Ok(())
    }
}

/// A day's counts and coverage.
fn summary(store: &Store, date: NaiveDate, today: NaiveDate, now: i64) -> ChatDaySummary {
    let (start, end) = day_bounds(date);
    let future = date > today;
    let events = if future { Vec::new() } else { store.events(start, end) };
    let messages =
        events.iter().filter(|e| matches!(e.kind, howdah_gameroom::ChatKind::Message { .. })).count();
    let people = events.iter().map(|e| e.user.as_str()).collect::<HashSet<_>>().len();
    let until = end.min(now - CLOCK_MARGIN);
    let covered: i64 = store.covered(start, until).iter().map(|(a, b)| b - a).sum();
    let coverage = if future || (covered == 0 && events.is_empty()) {
        ChatCoverage::None
    } else if covered >= until - start - if date == today { RECENT } else { 0 } {
        ChatCoverage::Full
    } else {
        ChatCoverage::Partial
    };
    let covered = store.covered(start, until).into_iter().map(|(a, b)| [a, b]).collect();
    ChatDaySummary {
        date: date.format("%Y-%m-%d").to_string(),
        messages: messages as u32,
        people: people as u32,
        coverage,
        covered,
        today: date == today,
        future,
    }
}

/// Shared with the chat room, which adds its visits.
pub type SharedArchive = Arc<ChatArchive>;

#[cfg(test)]
mod tests {
    use super::*;
    use howdah_gameroom::ChatKind;

    fn ev(time: i64, user: &str) -> ChatEvent {
        ChatEvent {
            time,
            user: user.into(),
            kind: ChatKind::Message { color: None, text: format!("{time}") },
        }
    }

    fn times(events: &[ChatEvent]) -> Vec<i64> {
        events.iter().map(|e| e.time).collect()
    }

    #[test]
    fn coverage_and_gaps() {
        let mut s = Store::default();
        s.add(100, 200, false, vec![ev(150, "a")]);
        s.add(180, 300, false, vec![ev(190, "b")]);
        s.add(400, 500, false, vec![]);
        assert_eq!(s.covered(0, 1000), [(100, 300), (400, 500)]);
        assert_eq!(s.gaps(0, 1000), [(0, 100), (300, 400), (500, 1000)]);
        assert_eq!(s.gaps(120, 250), []);
    }

    #[test]
    fn overlapping_chunks_read_once() {
        let mut s = Store::default();
        // Two archive requests that overlap: the newer is read there.
        s.add(100, 200, false, vec![ev(110, "a"), ev(150, "a")]);
        let newer = s.add(140, 160, false, vec![ev(150, "a"), ev(150, "a"), ev(155, "b")]);
        s.chunks.iter_mut().find(|c| c.info.id == newer).unwrap().info.updated += 10;
        assert_eq!(times(&s.events(0, 1000)), [110, 150, 150, 155]);
        // A newer visit doesn't hide what the archive holds, but adds where
        // it alone covers.
        s.add(100, 260, true, vec![ev(150, "a"), ev(230, "d")]);
        s.chunks.last_mut().unwrap().info.updated += 100;
        assert_eq!(times(&s.events(0, 1000)), [110, 150, 150, 155, 230]);
        // Loose events outside every span are merged without duplicates.
        s.add(300, 300, false, vec![ev(270, "c")]);
        s.add(300, 300, false, vec![ev(270, "c")]);
        assert_eq!(times(&s.events(260, 300)), [270]);
    }

    #[test]
    fn saved_and_loaded() {
        let dir = std::env::temp_dir().join(format!("howdah-chat-archive-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let mut s = Store { dir: Some(dir.clone()), ..Store::default() };
        let id = s.add(100, 200, true, vec![ev(150, "a")]);
        s.append(id, &[ev(160, "b")]);
        let loaded = Store::load(&dir);
        assert_eq!(loaded.chunks.len(), 1);
        assert_eq!(times(&loaded.chunks[0].events), [150, 160]);
        assert_eq!((loaded.chunks[0].info.start, loaded.chunks[0].info.end), (100, 200));
        assert_eq!(loaded.next_id, 1);
        assert!(loaded.chunks[0].info.live);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn days_are_local() {
        let d = NaiveDate::from_ymd_opt(2026, 10, 18).unwrap();
        let (s, e) = day_bounds(d);
        assert!((DAY - 3600..=DAY + 3600).contains(&(e - s)));
        let summary = summary(&Store::default(), d.succ_opt().unwrap(), d, s + 10);
        assert!(summary.future);
        assert_eq!(summary.coverage, ChatCoverage::None);
    }
}
