//! A game as the database stores it, and saving and loading one.

use std::collections::{HashMap, HashSet};
use std::time::Duration;

use howdah_arimaa::{
    Color, GameRecord, GameResult, GameTree, Glyph, NodeId, Position, WinReason, split_commands,
};
use rusqlite::{Connection, OptionalExtension, Transaction, params};
use sha2::{Digest, Sha256};

use crate::codec::{self, Reader};
use crate::{Error, Result, now};

/// arimaa.com's name as a source and a players' site.
pub const ARIMAA_COM: &str = "arimaa.com";

/// Where a game is from: a server and its id there.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SourceKey {
    pub source: String,
    pub id: String,
}

impl SourceKey {
    pub fn arimaa_com(id: impl Into<String>) -> SourceKey {
        SourceKey { source: ARIMAA_COM.into(), id: id.into() }
    }
}

/// One side's player as the game had them.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct PlayerInfo {
    pub name: Option<String>,
    /// The player's id on the game's server.
    pub site_id: Option<String>,
    /// 'h' (human) or 'b' (bot).
    pub kind: Option<char>,
    pub country: Option<String>,
    pub title: Option<String>,
    pub rating: Option<i64>,
    pub rating_k: Option<i64>,
}

/// A result as stored: `winner` is 'g' or 's', or old arimaa.com's 'd'
/// (draw) and 'u' (unknown); `termination` the ending's letter.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct StoredResult {
    pub winner: char,
    pub termination: Option<char>,
}

impl StoredResult {
    pub fn from_result(r: GameResult) -> StoredResult {
        StoredResult { winner: r.winner.letter(), termination: Some(r.reason.letter()) }
    }

    /// The result in the rules' terms, if it is one.
    pub fn to_result(self) -> Option<GameResult> {
        let winner = Color::from_letter(self.winner)?;
        let reason = WinReason::from_letter(self.termination?)?;
        Some(GameResult { winner, reason })
    }
}

/// A game: its headers, other tags, and its moves with variations and
/// annotations (`tree`; `None` for a game known only by its header).
#[derive(Clone, Debug, PartialEq)]
pub struct GameData {
    /// The row id, once saved.
    pub id: Option<i64>,
    pub gid: [u8; 16],
    pub source: Option<SourceKey>,
    /// Gold's and silver's players.
    pub players: [PlayerInfo; 2],
    pub event: Option<String>,
    pub site: Option<String>,
    pub time_control: Option<String>,
    pub rated: Option<bool>,
    pub postal: Option<bool>,
    /// `YYYY.MM.DD`.
    pub date: Option<String>,
    pub start_ts: Option<i64>,
    pub end_ts: Option<i64>,
    pub result: Option<StoredResult>,
    /// Tags with no column, in order.
    pub tags: Vec<(String, String)>,
    pub tree: Option<GameTree>,
    pub chat: Option<String>,
    /// When the game was fetched from its source.
    pub fetched_at: Option<i64>,
    /// Why the moves stop early, when a move couldn't be replayed; the
    /// move list as the source had it is then in `movelist`.
    pub replay_error: Option<String>,
    pub movelist: Option<String>,
}

/// Tags that have columns.
const COLUMN_TAGS: [&str; 11] = [
    "Event",
    "Site",
    "Date",
    "Gold",
    "Silver",
    "GoldRating",
    "SilverRating",
    "TimeControl",
    "Rated",
    "Postal",
    "GameId",
];

/// Tags the record writer works out from the tree.
const TREE_TAGS: [&str; 4] = ["PlyCount", "ResultCode", "Result", "Position"];

impl GameData {
    pub fn new() -> GameData {
        GameData {
            id: None,
            gid: crate::random_id(),
            source: None,
            players: Default::default(),
            event: None,
            site: None,
            time_control: None,
            rated: None,
            postal: None,
            date: None,
            start_ts: None,
            end_ts: None,
            result: None,
            tags: Vec::new(),
            tree: Some(GameTree::new()),
            chat: None,
            fetched_at: None,
            replay_error: None,
            movelist: None,
        }
    }

    /// A game from a record: the tags with columns go to them (`GameId`
    /// is an arimaa.com permanent id), the rest stay tags, and the result
    /// is the main line's.
    pub fn from_record(record: &GameRecord) -> GameData {
        let mut g = GameData::new();
        let tag = |name: &str| record.tag(name).map(str::trim).filter(|v| !v.is_empty()).map(String::from);
        let int = |name: &str| tag(name).and_then(|v| v.parse().ok());
        let flag = |name: &str| tag(name).map(|v| v != "0");
        g.event = tag("Event");
        g.site = tag("Site");
        g.date = tag("Date").map(|d| d.replace('-', "."));
        g.time_control = tag("TimeControl");
        g.rated = flag("Rated");
        g.postal = flag("Postal");
        g.source = tag("GameId").map(SourceKey::arimaa_com);
        if let Some(s) = &g.source {
            g.gid = gid_for(s);
        }
        g.players[0].name = tag("Gold");
        g.players[1].name = tag("Silver");
        g.players[0].rating = int("GoldRating");
        g.players[1].rating = int("SilverRating");
        g.tags = record
            .tags
            .iter()
            .filter(|(n, _)| !COLUMN_TAGS.contains(&n.as_str()) && !TREE_TAGS.contains(&n.as_str()))
            .cloned()
            .collect();
        let tree = &record.tree;
        let end = *tree.main_line().last().expect("the root at least");
        g.result = tree[end].result().map(StoredResult::from_result);
        g.tree = Some(tree.clone());
        g
    }

    /// The game as a record. A result the tree doesn't have (a timeout, a
    /// resignation) ends the main line.
    pub fn to_record(&self) -> GameRecord {
        let mut record = GameRecord::new();
        let int = |v: Option<i64>| v.map(|n| n.to_string());
        let flag = |v: Option<bool>| v.map(|b| if b { "1" } else { "0" }.to_string());
        let date = self.date.clone().or_else(|| self.start_ts.map(utc_date));
        let tags = [
            ("Event", self.event.clone()),
            ("Site", self.site.clone()),
            ("Date", date),
            ("Gold", self.players[0].name.clone()),
            ("Silver", self.players[1].name.clone()),
            ("GoldRating", int(self.players[0].rating)),
            ("SilverRating", int(self.players[1].rating)),
            ("TimeControl", self.time_control.clone()),
            ("Rated", flag(self.rated)),
            ("Postal", flag(self.postal)),
            ("GameId", self.source.as_ref().filter(|s| s.source == ARIMAA_COM).map(|s| s.id.clone())),
        ];
        for (name, value) in tags {
            if let Some(value) = value {
                record.set_tag(name, value);
            }
        }
        record.tags.extend(self.tags.iter().cloned());
        if let Some(tree) = &self.tree {
            record.tree = tree.clone();
            let end = *record.tree.main_line().last().expect("the root at least");
            if let Some(result) = self.result.and_then(StoredResult::to_result)
                && record.tree[end].result().is_none()
            {
                let _ = record.tree.end_line(end, result);
            }
        }
        record
    }
}

impl GameData {
    /// Takes what `record` says over what's stored: its tags (those with
    /// columns, when present, and all the others), its moves, and the main
    /// line's result when it has one. Headers a record doesn't carry stay.
    pub fn merge_record(&mut self, record: &GameRecord) {
        let r = GameData::from_record(record);
        fn take<T>(to: &mut Option<T>, from: Option<T>) {
            if from.is_some() {
                *to = from;
            }
        }
        take(&mut self.event, r.event);
        take(&mut self.site, r.site);
        take(&mut self.time_control, r.time_control);
        take(&mut self.rated, r.rated);
        take(&mut self.postal, r.postal);
        take(&mut self.date, r.date);
        take(&mut self.result, r.result);
        if self.source.is_none() {
            self.source = r.source;
        }
        for (to, from) in self.players.iter_mut().zip(r.players) {
            if from.name.is_some() && from.name != to.name {
                *to = PlayerInfo::default();
            }
            take(&mut to.name, from.name);
            take(&mut to.rating, from.rating);
        }
        self.tags = r.tags;
        self.tree = r.tree;
    }
}

impl Default for GameData {
    fn default() -> Self {
        GameData::new()
    }
}

/// The `gid` of a game with a source key, the same wherever it's
/// imported: the first 16 bytes of a SHA-256 of `source:id`.
pub fn gid_for(source: &SourceKey) -> [u8; 16] {
    let digest = Sha256::digest(format!("{}:{}", source.source, source.id).as_bytes());
    digest[..16].try_into().expect("16 bytes")
}

/// `YYYY.MM.DD` in UTC for Unix seconds.
pub(crate) fn utc_date(ts: i64) -> String {
    // Days to a civil date (Howard Hinnant's algorithm).
    let z = ts.div_euclid(86_400) + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z - era * 146_097;
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let day = doy - (153 * mp + 2) / 5 + 1;
    let month = if mp < 10 { mp + 3 } else { mp - 9 };
    let year = yoe + era * 400 + i64::from(month <= 2);
    format!("{year:04}.{month:02}.{day:02}")
}

/// Players already found or made in this transaction, by site key or name.
#[derive(Default)]
pub(crate) struct PlayerCache {
    ids: HashMap<(Option<String>, String), i64>,
    names: HashSet<(i64, String)>,
}

fn player(
    tx: &Transaction,
    cache: &mut PlayerCache,
    site: Option<&str>,
    p: &PlayerInfo,
) -> Result<Option<i64>> {
    let Some(name) = p.name.as_deref() else { return Ok(None) };
    let kind = p.kind.map(String::from);
    let id = match (site, p.site_id.as_deref()) {
        (Some(site), Some(sid)) => {
            let key = (Some(site.to_string()), format!("#{sid}"));
            let id = match cache.ids.get(&key) {
                Some(&id) => Some(id),
                None => tx
                    .query_row("SELECT id FROM players WHERE site = ?1 AND site_id = ?2", (site, sid), |r| {
                        r.get(0)
                    })
                    .optional()?
                    .or(tx
                        // A player first seen without an id (a game from
                        // the viewer) takes it now.
                        .query_row(
                            "UPDATE players SET site_id = ?3
                             WHERE id = (SELECT id FROM players WHERE site = ?1 AND site_id IS NULL
                                         AND name = ?2 ORDER BY id DESC LIMIT 1)
                             RETURNING id",
                            (site, name, sid),
                            |r| r.get(0),
                        )
                        .optional()?),
            };
            let id = match id {
                Some(id) => {
                    tx.prepare_cached(
                        "UPDATE players SET name = ?2, kind = coalesce(?3, kind), country = coalesce(?4, country),
                         title = coalesce(?5, title) WHERE id = ?1",
                    )?
                    .execute(params![id, name, kind, p.country, p.title])?;
                    id
                }
                None => tx
                    .prepare_cached(
                        "INSERT INTO players (site, site_id, name, kind, country, title)
                         VALUES (?1, ?2, ?3, ?4, ?5, ?6) RETURNING id",
                    )?
                    .query_row(params![site, sid, name, kind, p.country, p.title], |r| r.get(0))?,
            };
            cache.ids.insert(key, id);
            id
        }
        _ => {
            let key = (site.map(String::from), name.to_string());
            match cache.ids.get(&key) {
                Some(&id) => id,
                None => {
                    let found: Option<i64> = tx
                        .prepare_cached(
                            "SELECT p.id FROM player_names n JOIN players p ON p.id = n.player
                             WHERE n.name = ?1 AND p.site IS ?2 ORDER BY p.id DESC LIMIT 1",
                        )?
                        .query_row((name, site), |r| r.get(0))
                        .optional()?;
                    let id = match found {
                        Some(id) => id,
                        None => tx
                            .prepare_cached(
                                "INSERT INTO players (site, name, kind, country, title)
                                 VALUES (?1, ?2, ?3, ?4, ?5) RETURNING id",
                            )?
                            .query_row(params![site, name, kind, p.country, p.title], |r| r.get(0))?,
                    };
                    cache.ids.insert(key, id);
                    id
                }
            }
        }
    };
    if cache.names.insert((id, name.to_string())) {
        tx.prepare_cached("INSERT OR IGNORE INTO player_names (player, name) VALUES (?1, ?2)")?
            .execute((id, name))?;
    }
    Ok(Some(id))
}

/// The main line's moves as a blob, and the variation nodes as rows
/// (`id`, parent reference, place among siblings, move), in an order that
/// adds parents before children and siblings in order.
struct Encoded {
    main: Vec<NodeId>,
    moves: Vec<u8>,
    variations: Vec<(i64, i64, i64, Vec<u8>)>,
    /// Every node's reference.
    refs: HashMap<NodeId, i64>,
}

fn encode(tree: &GameTree) -> Result<Encoded> {
    let main = tree.main_line();
    let moves = codec::encode_line(main[1..].iter().map(|&n| tree[n].mv().expect("a move")))?;
    let mut refs: HashMap<NodeId, i64> = main.iter().enumerate().map(|(i, &n)| (n, i as i64)).collect();
    let mut variations = Vec::new();
    let last = main.len() - 1;
    // Depth first from each main-line node, so a variation's id comes
    // after its parent's and before its later siblings'.
    let mut next = 1i64;
    for (i, &node) in main.iter().enumerate() {
        let skip = usize::from(i < last);
        let mut stack: Vec<(NodeId, i64, usize)> = tree[node]
            .children()
            .iter()
            .enumerate()
            .skip(skip)
            .rev()
            .map(|(ord, &c)| (c, i as i64, ord))
            .collect();
        while let Some((n, parent, ord)) = stack.pop() {
            let mut bytes = Vec::new();
            codec::encode_move(&mut bytes, tree[n].mv().expect("a move"))?;
            let id = next;
            next += 1;
            refs.insert(n, -id);
            variations.push((id, parent, ord as i64, bytes));
            stack.extend(tree[n].children().iter().enumerate().rev().map(|(o, &c)| (c, -id, o)));
        }
    }
    Ok(Encoded { main, moves, variations, refs })
}

fn fingerprint(start_ply: usize, board: Option<&str>, moves: &[u8]) -> Vec<u8> {
    let mut h = Sha256::new();
    h.update((start_ply as u64).to_le_bytes());
    h.update(board.unwrap_or("").as_bytes());
    h.update([0]);
    h.update(moves);
    h.finalize()[..16].to_vec()
}

/// A comment without its `%emt`, and the time it gave, for main-line moves
/// (the times are kept as one blob for the game).
fn split_elapsed(comment: &str) -> (Option<String>, Option<u64>) {
    let (text, commands) = split_commands(comment);
    let mut ms = None;
    let mut kept = Vec::new();
    for (name, args) in commands {
        if name == "emt" && ms.is_none() {
            let mut a = howdah_arimaa::Annotation::default();
            a.set_command("emt", &args);
            if let Some(d) = a.elapsed() {
                ms = Some(d.as_millis() as u64);
                continue;
            }
        }
        kept.push((name, args));
    }
    if ms.is_none() {
        return (Some(comment.to_string()), None);
    }
    let mut parts: Vec<String> = Vec::new();
    if !text.is_empty() {
        parts.push(text);
    }
    parts.extend(
        kept.iter().map(|(n, a)| if a.is_empty() { format!("[%{n}]") } else { format!("[%{n} {a}]") }),
    );
    ((!parts.is_empty()).then(|| parts.join(" ")), ms)
}

/// A comment with `%emt` put back first among its commands, where moves
/// played in the app get it.
fn with_elapsed(comment: Option<&str>, ms: u64) -> Option<String> {
    let mut a = howdah_arimaa::Annotation::default();
    let d = Duration::from_millis(ms);
    if ms.is_multiple_of(1000) {
        a.set_elapsed(d)
    } else {
        a.set_elapsed_ms(d)
    }
    let emt = a.comment.expect("set");
    let Some(comment) = comment else { return Some(emt) };
    let (text, commands) = split_commands(comment);
    let mut parts: Vec<String> = Vec::new();
    if !text.is_empty() {
        parts.push(text);
    }
    parts.push(emt);
    parts.extend(
        commands.iter().map(|(n, a)| if a.is_empty() { format!("[%{n}]") } else { format!("[%{n} {a}]") }),
    );
    Some(parts.join(" "))
}

/// Whose copy of a game is being saved.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SaveMode {
    /// The user's: it replaces the game's moves, variations and the
    /// user's annotations.
    User,
    /// A source's (an archive import, a fetch from arimaa.com): it
    /// updates the headers, and the kinds of data a source gives (move
    /// times, chat, an unreplayable move list). If the main line is the
    /// one stored, the user's variations and annotations stay; if it
    /// differs, the game is replaced as for [`SaveMode::User`].
    Source,
}

/// The annotation kinds a source gives.
const SOURCE_KINDS: [&str; 3] = ["times", "chat", "movelist"];

pub(crate) fn save(tx: &Transaction, g: &GameData, cache: &mut PlayerCache, mode: SaveMode) -> Result<i64> {
    let site = g.source.as_ref().map(|s| s.source.as_str());
    let gold = player(tx, cache, site, &g.players[0])?;
    let silver = player(tx, cache, site, &g.players[1])?;
    let existing: Option<i64> = match g.id {
        Some(id) => Some(id),
        None => {
            let by_gid = tx
                .prepare_cached("SELECT id FROM games WHERE gid = ?1")?
                .query_row([g.gid.as_slice()], |r| r.get(0))
                .optional()?;
            match (by_gid, &g.source) {
                (Some(id), _) => Some(id),
                (None, Some(s)) => tx
                    .prepare_cached("SELECT id FROM games WHERE source = ?1 AND source_id = ?2")?
                    .query_row((&s.source, &s.id), |r| r.get(0))
                    .optional()?,
                (None, None) => None,
            }
        }
    };
    let encoded = g.tree.as_ref().map(encode).transpose()?;
    let (start_ply, board) = match &g.tree {
        Some(t) => (t.start_ply(), t.start_position().map(Position::to_short_string)),
        None => (0, None),
    };
    let rules = g.tree.as_ref().is_some_and(GameTree::old_rules).then_some("old");
    let moves = encoded.as_ref().map(|e| e.moves.as_slice());
    let plies = encoded.as_ref().map_or(0, |e| e.main.len() as i64 - 1);
    let fp = moves.map(|m| fingerprint(start_ply, board.as_deref(), m));
    // A source's copy over the same moves leaves the user's work alone.
    let keep_user = mode == SaveMode::Source
        && match existing {
            Some(id) => {
                let stored: Option<Vec<u8>> = tx
                    .prepare_cached("SELECT moves FROM games WHERE id = ?1")?
                    .query_row([id], |r| r.get(0))
                    .optional()?
                    .flatten();
                stored.is_some() && stored.as_deref() == moves
            }
            None => false,
        };
    let t = now();
    let [p0, p1] = &g.players;
    let values = params![
        gold,
        silver,
        p0.name,
        p1.name,
        p0.rating,
        p1.rating,
        p0.rating_k,
        p1.rating_k,
        g.event,
        g.site,
        g.time_control,
        g.rated,
        g.postal,
        g.date,
        g.start_ts,
        g.end_ts,
        g.result.map(|r| r.winner.to_string()),
        g.result.and_then(|r| r.termination).map(String::from),
        plies,
        start_ply as i64,
        board,
        moves,
        fp,
        g.replay_error,
        t,
        g.fetched_at,
        g.source.as_ref().map(|s| &s.source),
        g.source.as_ref().map(|s| &s.id),
        rules,
    ];
    let id: i64 = match existing {
        Some(id) => {
            let n = tx
                .prepare_cached(
                    "UPDATE games SET gold = ?1, silver = ?2, gold_name = ?3, silver_name = ?4,
                     gold_rating = ?5, silver_rating = ?6, gold_rating_k = ?7, silver_rating_k = ?8,
                     event = ?9, site = ?10, time_control = ?11, rated = ?12, postal = ?13, date = ?14,
                     start_ts = ?15, end_ts = ?16, winner = ?17, termination = ?18, plies = ?19,
                     start_ply = ?20, start_board = ?21, moves = ?22, fingerprint = ?23,
                     replay_error = ?24, modified_at = ?25, fetched_at = coalesce(?26, fetched_at),
                     source = ?27, source_id = ?28, rules = ?29
                     WHERE id = ?30",
                )?
                .execute(rusqlite::params_from_iter(
                    values.iter().copied().chain([&id as &dyn rusqlite::ToSql]),
                ))?;
            if n == 0 {
                return Err(Error::NoGame(id));
            }
            id
        }
        None => tx
            .prepare_cached(
                "INSERT INTO games (gold, silver, gold_name, silver_name, gold_rating, silver_rating,
                 gold_rating_k, silver_rating_k, event, site, time_control, rated, postal, date,
                 start_ts, end_ts, winner, termination, plies, start_ply, start_board, moves,
                 fingerprint, replay_error, modified_at, fetched_at, source, source_id, rules, gid,
                 origin, added_at)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15, ?16, ?17,
                 ?18, ?19, ?20, ?21, ?22, ?23, ?24, ?25, ?26, ?27, ?28, ?29, ?30, 1, ?25)
                 RETURNING id",
            )?
            .query_row(
                rusqlite::params_from_iter(
                    values.iter().copied().chain([&g.gid.as_slice() as &dyn rusqlite::ToSql]),
                ),
                |r| r.get(0),
            )?,
    };
    if existing.is_some() {
        tx.prepare_cached("DELETE FROM game_tags WHERE game = ?1")?.execute([id])?;
        if !keep_user {
            tx.prepare_cached("DELETE FROM nodes WHERE game = ?1")?.execute([id])?;
        }
    }
    {
        let mut insert =
            tx.prepare_cached("INSERT INTO game_tags (game, ord, name, value) VALUES (?1, ?2, ?3, ?4)")?;
        for (i, (name, value)) in g.tags.iter().enumerate() {
            insert.execute(params![id, i as i64, name, value])?;
        }
    }
    let mut notes: Vec<(i64, &str, Vec<u8>)> = Vec::new();
    if let (Some(tree), Some(e)) = (&g.tree, &encoded) {
        if !keep_user {
            let mut insert = tx.prepare_cached(
                "INSERT INTO nodes (game, id, parent, ord, move) VALUES (?1, ?2, ?3, ?4, ?5)",
            )?;
            for (vid, parent, ord, bytes) in &e.variations {
                insert.execute(params![id, vid, parent, ord, bytes])?;
            }
        }
        let main_moves: HashSet<NodeId> = e.main[1..].iter().copied().collect();
        let mut times = vec![None; e.main.len() - 1];
        let mut nodes: Vec<(&NodeId, &i64)> = e.refs.iter().collect();
        nodes.sort_by_key(|(_, r)| (r.is_negative(), r.abs()));
        for (&node, &r) in nodes {
            let a = tree[node].annotation();
            if let Some(c) = &a.comment {
                let (comment, ms) =
                    if main_moves.contains(&node) { split_elapsed(c) } else { (Some(c.clone()), None) };
                if let Some(ms) = ms {
                    times[r as usize - 1] = Some(ms);
                }
                if let Some(c) = comment {
                    notes.push((r, "comment", c.into_bytes()));
                }
            }
            if let Some(intro) = &a.intro {
                notes.push((r, "intro", intro.clone().into_bytes()));
            }
            if !a.glyphs.is_empty() {
                let text: Vec<String> = a.glyphs.iter().map(|g| g.0.to_string()).collect();
                notes.push((r, "glyphs", text.join(" ").into_bytes()));
            }
        }
        if times.iter().any(Option::is_some) {
            notes.push((0, "times", codec::encode_times(&times)));
        }
    }
    if let Some(chat) = &g.chat {
        notes.push((0, "chat", chat.clone().into_bytes()));
    }
    if let Some(m) = &g.movelist {
        notes.push((0, "movelist", m.clone().into_bytes()));
    }
    if keep_user {
        notes.retain(|(_, kind, _)| SOURCE_KINDS.contains(kind));
    }
    let scope: &[&str] = if keep_user { &SOURCE_KINDS } else { &[] };
    write_annotations(tx, id, existing.is_some(), &notes, scope, t)?;
    Ok(id)
}

/// Writes the user's own annotations on a game (author ''), keeping the
/// creation time of the ones that stay and dropping the ones gone: of
/// every kind, or only of the kinds in `scope` when it isn't empty.
fn write_annotations(
    tx: &Transaction,
    game: i64,
    existed: bool,
    notes: &[(i64, &str, Vec<u8>)],
    scope: &[&str],
    t: i64,
) -> Result<()> {
    let mut upsert = tx.prepare_cached(
        "INSERT INTO annotations (game, node, kind, author, origin, created_at, modified_at, payload)
         VALUES (?1, ?2, ?3, '', 1, ?4, ?4, ?5)
         ON CONFLICT (game, node, kind, author) DO UPDATE SET
           modified_at = CASE WHEN payload = excluded.payload THEN modified_at ELSE excluded.modified_at END,
           origin = CASE WHEN payload = excluded.payload THEN origin ELSE 1 END,
           payload = excluded.payload",
    )?;
    for (node, kind, payload) in notes {
        upsert.execute(params![game, node, kind, t, payload])?;
    }
    if existed {
        let kept: HashSet<(i64, &str)> = notes.iter().map(|(n, k, _)| (*n, *k)).collect();
        let mut stale = Vec::new();
        {
            let mut q =
                tx.prepare_cached("SELECT node, kind FROM annotations WHERE game = ?1 AND author = ''")?;
            let rows = q.query_map([game], |r| Ok((r.get::<_, i64>(0)?, r.get::<_, String>(1)?)))?;
            for row in rows {
                let (n, k) = row?;
                if !kept.contains(&(n, k.as_str())) && (scope.is_empty() || scope.contains(&k.as_str())) {
                    stale.push((n, k));
                }
            }
        }
        let mut delete = tx.prepare_cached(
            "DELETE FROM annotations WHERE game = ?1 AND node = ?2 AND kind = ?3 AND author = ''",
        )?;
        for (n, k) in stale {
            delete.execute(params![game, n, k])?;
        }
    }
    Ok(())
}

fn text(bytes: Vec<u8>) -> String {
    String::from_utf8(bytes).unwrap_or_else(|e| String::from_utf8_lossy(e.as_bytes()).into_owned())
}

pub(crate) fn load(conn: &Connection, id: i64) -> Result<GameData> {
    load_with(conn, id, true)
}

pub(crate) fn load_header(conn: &Connection, id: i64) -> Result<GameData> {
    load_with(conn, id, false)
}

fn load_with(conn: &Connection, id: i64, with_moves: bool) -> Result<GameData> {
    let g = conn
        .prepare_cached(
            "SELECT gid, source, source_id, gold_name, silver_name, gold_rating, silver_rating,
             gold_rating_k, silver_rating_k, event, site, time_control, rated, postal, date, start_ts,
             end_ts, winner, termination, start_ply, start_board, moves, replay_error, fetched_at,
             (SELECT site_id FROM players WHERE id = gold), (SELECT kind FROM players WHERE id = gold),
             (SELECT country FROM players WHERE id = gold), (SELECT title FROM players WHERE id = gold),
             (SELECT site_id FROM players WHERE id = silver), (SELECT kind FROM players WHERE id = silver),
             (SELECT country FROM players WHERE id = silver), (SELECT title FROM players WHERE id = silver),
             rules
             FROM games WHERE id = ?1",
        )?
        .query_row([id], |r| {
            let gid: Vec<u8> = r.get(0)?;
            let source: Option<String> = r.get(1)?;
            let source_id: Option<String> = r.get(2)?;
            let ch = |s: Option<String>| s.and_then(|s| s.chars().next());
            let player =
                |name: usize, rating: usize, k: usize, extra: usize| -> rusqlite::Result<PlayerInfo> {
                    Ok(PlayerInfo {
                        name: r.get(name)?,
                        rating: r.get(rating)?,
                        rating_k: r.get(k)?,
                        site_id: r.get(extra)?,
                        kind: ch(r.get(extra + 1)?),
                        country: r.get(extra + 2)?,
                        title: r.get(extra + 3)?,
                    })
                };
            let winner: Option<String> = r.get(17)?;
            let start_ply: i64 = r.get(19)?;
            let board: Option<String> = r.get(20)?;
            let moves: Option<Vec<u8>> = r.get(21)?;
            let old_rules = r.get::<_, Option<String>>(32)?.as_deref() == Some("old");
            Ok((
                GameData {
                    id: Some(id),
                    gid: gid.try_into().unwrap_or([0; 16]),
                    source: source.zip(source_id).map(|(source, id)| SourceKey { source, id }),
                    players: [player(3, 5, 7, 24)?, player(4, 6, 8, 28)?],
                    event: r.get(9)?,
                    site: r.get(10)?,
                    time_control: r.get(11)?,
                    rated: r.get(12)?,
                    postal: r.get(13)?,
                    date: r.get(14)?,
                    start_ts: r.get(15)?,
                    end_ts: r.get(16)?,
                    result: ch(winner)
                        .map(|w| StoredResult { winner: w, termination: ch(r.get(18).ok().flatten()) }),
                    tags: Vec::new(),
                    tree: None,
                    chat: None,
                    fetched_at: r.get(23)?,
                    replay_error: r.get(22)?,
                    movelist: None,
                },
                start_ply,
                board,
                moves,
                old_rules,
            ))
        })
        .optional()?
        .ok_or(Error::NoGame(id))?;
    let (mut data, start_ply, board, moves, old_rules) = g;
    {
        let mut q = conn.prepare_cached("SELECT name, value FROM game_tags WHERE game = ?1 ORDER BY ord")?;
        data.tags = q.query_map([id], |r| Ok((r.get(0)?, r.get(1)?)))?.collect::<rusqlite::Result<_>>()?;
    }
    let mut refs: Vec<NodeId> = Vec::new();
    let mut var_refs: HashMap<i64, NodeId> = HashMap::new();
    if let Some(blob) = moves.filter(|_| with_moves) {
        let mut tree = match board {
            Some(board) => {
                let side = if start_ply % 2 == 0 { Color::Gold } else { Color::Silver };
                let pos = Position::from_short_string(side, &board).map_err(|e| {
                    Error::Moves(id, codec::CodecError::Game(howdah_arimaa::GameError::Parse(e)))
                })?;
                GameTree::from_position(pos, (start_ply / 2 + 1) as u32)
                    .map_err(|e| Error::Moves(id, e.into()))?
            }
            None => GameTree::new(),
        };
        tree.set_old_rules(old_rules);
        let mut at = tree.root();
        refs.push(at);
        let mut r = Reader::line(&blob).map_err(|e| Error::Moves(id, e))?;
        while !r.is_empty() {
            at = r.add_next(&mut tree, at).map_err(|e| Error::Moves(id, e))?;
            refs.push(at);
        }
        let mut q = conn.prepare_cached("SELECT id, parent, move FROM nodes WHERE game = ?1 ORDER BY id")?;
        let rows =
            q.query_map([id], |r| Ok((r.get::<_, i64>(0)?, r.get::<_, i64>(1)?, r.get::<_, Vec<u8>>(2)?)))?;
        for row in rows {
            let (vid, parent, bytes) = row?;
            let parent = if parent >= 0 {
                refs.get(parent as usize).copied()
            } else {
                var_refs.get(&-parent).copied()
            };
            let Some(parent) = parent else { continue };
            let node = Reader::single(&bytes).add_next(&mut tree, parent).map_err(|e| Error::Moves(id, e))?;
            var_refs.insert(vid, node);
        }
        if let Some(result) = data.result.and_then(StoredResult::to_result) {
            let end = *refs.last().expect("the root");
            if tree[end].result().is_none() {
                let _ = tree.end_line(end, result);
            }
        }
        data.tree = Some(tree);
    } else {
        data.tree = None;
    }
    let node_of = |r: i64| if r >= 0 { refs.get(r as usize).copied() } else { var_refs.get(&-r).copied() };
    let mut q =
        conn.prepare_cached("SELECT node, kind, payload FROM annotations WHERE game = ?1 AND author = ''")?;
    let rows =
        q.query_map([id], |r| Ok((r.get::<_, i64>(0)?, r.get::<_, String>(1)?, r.get::<_, Vec<u8>>(2)?)))?;
    let mut times = None;
    for row in rows {
        let (r, kind, payload) = row?;
        match kind.as_str() {
            "chat" => data.chat = Some(text(payload)),
            "movelist" => data.movelist = Some(text(payload)),
            "times" => times = codec::decode_times(&payload).ok(),
            "comment" | "intro" | "glyphs" => {
                let (Some(tree), Some(node)) = (data.tree.as_mut(), node_of(r)) else { continue };
                let Ok(a) = tree.annotation_mut(node) else { continue };
                match kind.as_str() {
                    "comment" => a.comment = Some(text(payload)),
                    "intro" => a.intro = Some(text(payload)),
                    _ => {
                        for g in text(payload).split_whitespace().filter_map(|n| n.parse().ok()) {
                            a.glyphs.push(Glyph(g));
                        }
                    }
                }
            }
            _ => {}
        }
    }
    if let (Some(times), Some(tree)) = (times, data.tree.as_mut()) {
        for (i, ms) in times.into_iter().enumerate() {
            let (Some(ms), Some(&node)) = (ms, refs.get(i + 1)) else { continue };
            if let Ok(a) = tree.annotation_mut(node) {
                a.comment = with_elapsed(a.comment.as_deref(), ms);
            }
        }
    }
    Ok(data)
}
