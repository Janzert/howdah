//! The app's game database (`howdah-db`; plan in `docs/DATABASE.md`).
//!
//! Every session's game is saved as it changes (a session's
//! [`Session::saveable`] record, checked every [`SAVE_EVERY`] and when the
//! session closes), into the game it was loaded from or a new one. A
//! finished arimaa.com game is looked for here before it's fetched, and
//! kept once fetched, so the database is also a cache of arimaa.com.
//!
//! The database is a convenience: when it can't be opened or written,
//! the app goes on without it and says so on stderr.

use std::path::{Path, PathBuf};
use std::sync::{Mutex, MutexGuard};
use std::time::Duration;

use std::collections::HashMap;

use howdah_arimaa::{Color, GameRecord, NodeId};
use howdah_db::{
    ARIMAA_COM, Database, Filter, GameData, Origin, Outcome, PositionDatum, SaveMode, SetupLevel,
};
use serde::{Deserialize, Serialize};

use crate::dto::{
    AnalysisLine, ApiError, Eval, LibraryFilter, LibraryGameView, LibraryPageView, PvTurn, SetupStatView,
    SetupStatsView,
};
use crate::session::Session;

/// How often a session's game is checked for changes to save.
pub const SAVE_EVERY: Duration = Duration::from_secs(2);

/// The file's name in the app's data directory.
pub const FILE_NAME: &str = "games.db";

pub struct Library {
    db: Mutex<Option<Database>>,
    path: Option<PathBuf>,
    /// The app's engines (their ids in the engine list) and the authors
    /// their evals are kept under.
    authors: Mutex<HashMap<String, String>>,
}

/// The kind of position data engine lines are kept as.
const EVAL: &str = "eval";

impl Library {
    /// The database at `path` (`None`: none, as in tests).
    pub fn open(path: Option<PathBuf>) -> Library {
        let db = path.as_deref().and_then(|p| match open_at(p) {
            Ok(db) => Some(db),
            Err(e) => {
                eprintln!("game database {}: {e}", p.display());
                None
            }
        });
        Library::with_db(db, path)
    }

    fn with_db(db: Option<Database>, path: Option<PathBuf>) -> Library {
        let authors = db.as_ref().and_then(|db| db.local_engines().ok()).unwrap_or_default();
        Library { db: Mutex::new(db), path, authors: Mutex::new(authors.into_iter().collect()) }
    }

    /// A fact about the game database (`meta`).
    pub fn meta(&self, key: &str) -> Option<String> {
        self.with("reading a fact", |db| db.meta(key)).flatten()
    }

    /// Runs `f` on the database, for callers that report its errors.
    pub fn with_db_result<T>(
        &self,
        f: impl FnOnce(&mut Database) -> howdah_db::Result<T>,
    ) -> howdah_db::Result<T> {
        let mut db = self.db();
        let db = db.as_mut().ok_or(howdah_db::Error::Archive("there's no game database".into()))?;
        f(db)
    }

    pub fn is_open(&self) -> bool {
        self.db().is_some()
    }

    fn authors(&self) -> MutexGuard<'_, HashMap<String, String>> {
        self.authors.lock().unwrap_or_else(|e| e.into_inner())
    }

    /// Records the authors engines write as, as sessions learn them.
    pub fn learn_authors(&self, learned: &HashMap<String, String>) {
        let new: Vec<(String, String)> = {
            let known = self.authors();
            learned
                .iter()
                .filter(|(e, a)| known.get(*e) != Some(a))
                .map(|(e, a)| (e.clone(), a.clone()))
                .collect()
        };
        for (engine, author) in new {
            if self.with("recording an engine", |db| db.set_local_engine(&engine, &author)).is_some() {
                self.authors().insert(engine, author);
            }
        }
    }

    /// Writes engine lines.
    pub fn put_evals(&self, data: &[PositionDatum]) {
        if !data.is_empty() {
            self.with("writing evals", |db| db.put_position_data(EVAL, data));
        }
    }

    /// The lines kept for positions `hashes`, by the app's engines: each
    /// with the engine's id in the engine list.
    pub fn get_evals(&self, hashes: &[u64]) -> Vec<(String, u64, StoredEval)> {
        let authors = self.authors().clone();
        let names: Vec<String> = authors.values().cloned().collect();
        if names.is_empty() {
            return Vec::new();
        }
        let found =
            self.with("reading evals", |db| db.position_data(EVAL, hashes, &names)).unwrap_or_default();
        let mut out = Vec::new();
        for d in found {
            let Some(stored) = StoredEval::decode(&d.payload) else { continue };
            for (engine, _) in authors.iter().filter(|(_, a)| **a == d.author) {
                out.push((engine.clone(), d.hash, stored.clone()));
            }
        }
        out
    }

    /// A library in memory, for tests.
    #[cfg(test)]
    pub fn in_memory() -> Library {
        Library::with_db(Some(Database::open_in_memory().expect("an in-memory database")), None)
    }

    pub fn path(&self) -> Option<&Path> {
        self.path.as_deref()
    }

    fn db(&self) -> MutexGuard<'_, Option<Database>> {
        self.db.lock().unwrap_or_else(|e| e.into_inner())
    }

    fn with<T>(&self, what: &str, f: impl FnOnce(&mut Database) -> howdah_db::Result<T>) -> Option<T> {
        let mut db = self.db();
        let db = db.as_mut()?;
        match f(db) {
            Ok(v) => Some(v),
            Err(e) => {
                eprintln!("game database, {what}: {e}");
                None
            }
        }
    }

    /// [`Library::with`] for the game list's commands, whose errors go to
    /// the user.
    fn try_with<T>(&self, f: impl FnOnce(&mut Database) -> howdah_db::Result<T>) -> Result<T, ApiError> {
        let mut db = self.db();
        let db = db.as_mut().ok_or_else(|| ApiError::state("there's no game database"))?;
        f(db).map_err(|e| ApiError::state(format!("game database: {e}")))
    }

    /// A page of the game list.
    pub fn page(&self, filter: &LibraryFilter, offset: u32, limit: u32) -> Result<LibraryPageView, ApiError> {
        if self.db().is_none() {
            return Ok(LibraryPageView { games: Vec::new(), total: 0, available: false });
        }
        let filter = to_filter(filter);
        let page = self.try_with(|db| db.search(&filter, offset.into(), limit.into()))?;
        let letter = |c: Option<char>| c.map(String::from);
        Ok(LibraryPageView {
            games: page
                .games
                .into_iter()
                .map(|g| LibraryGameView {
                    id: g.id,
                    arimaa_id: g.arimaa_id,
                    gold: g.gold,
                    silver: g.silver,
                    gold_rating: g.gold_rating,
                    silver_rating: g.silver_rating,
                    date: g.date,
                    event: g.event,
                    time_control: g.time_control,
                    rated: g.rated,
                    postal: g.postal,
                    winner: letter(g.winner),
                    termination: letter(g.termination),
                    plies: g.plies,
                    header_only: g.header_only,
                    set_position: g.set_position,
                    variations: g.variations,
                })
                .collect(),
            total: page.total,
            available: true,
        })
    }

    /// The setup explorer: the games `filter` lets through grouped by
    /// gold's setup, silver's (`level` `silver`), or silver's replies to
    /// gold setup `gold` (`replies`), the `limit` most played.
    pub fn setups(
        &self,
        filter: &LibraryFilter,
        level: &str,
        gold: Option<&str>,
        limit: usize,
    ) -> Result<SetupStatsView, ApiError> {
        if self.db().is_none() {
            return Ok(SetupStatsView { rows: Vec::new(), games: 0, setups: 0, available: false });
        }
        let level = match (level, gold) {
            ("gold", _) => SetupLevel::Gold,
            ("silver", _) => SetupLevel::Silver,
            ("replies", Some(g)) => SetupLevel::Replies(g.to_string()),
            _ => return Err(ApiError::state(format!("no setup level {level:?}"))),
        };
        let filter = to_filter(filter);
        let stats = self.try_with(|db| db.setup_stats(&filter, &level, limit))?;
        Ok(SetupStatsView {
            rows: stats
                .rows
                .into_iter()
                .map(|r| SetupStatView {
                    setup: r.setup,
                    games: r.games,
                    gold_wins: r.gold_wins,
                    silver_wins: r.silver_wins,
                    rated_games: r.rated_games,
                    rated_gold_wins: r.rated_gold_wins,
                    expected_gold_wins: r.expected_gold_wins,
                    rating: r.rating,
                    last_date: r.last_date,
                })
                .collect(),
            games: stats.games,
            setups: stats.setups,
            available: true,
        })
    }

    /// Game `id` as a record, or its arimaa.com id when only its header
    /// is here.
    pub fn record(&self, id: i64) -> Result<Result<GameRecord, String>, ApiError> {
        self.try_with(|db| {
            let game = db.load(id)?;
            Ok(match (&game.tree, &game.source) {
                (None, Some(s)) if s.source == ARIMAA_COM => Err(s.id.clone()),
                _ => Ok(game.to_record()),
            })
        })
    }

    pub fn delete(&self, id: i64) -> Result<(), ApiError> {
        self.try_with(|db| db.delete(id))
    }

    /// Finished arimaa.com game `gid` (its permanent id) as a record, if
    /// it's here with its moves, and its id here.
    pub fn arimaa_game(&self, gid: &str) -> Option<(i64, GameRecord)> {
        self.with("reading a game", |db| {
            let Some(id) = db.find_source(ARIMAA_COM, gid)? else { return Ok(None) };
            if !db.has_moves(id)? {
                return Ok(None);
            }
            Ok(Some((id, db.load(id)?.to_record())))
        })
        .flatten()
    }

    /// Keeps a game fetched from arimaa.com (a record with its `GameId`)
    /// and returns its id here. The user's analysis of it, if it's here
    /// already with the same moves, stays.
    pub fn keep_fetched(&self, record: &GameRecord) -> Option<i64> {
        self.with("keeping a fetched game", |db| {
            let mut game = GameData::from_record(record);
            if let Some(id) =
                game.source.as_ref().map(|s| db.find_source(&s.source, &s.id)).transpose()?.flatten()
            {
                let mut stored = db.load_header(id)?;
                stored.merge_record(record);
                game = stored;
            }
            game.fetched_at = Some(howdah_now());
            db.save_as(&game, SaveMode::Source)
        })
    }

    /// Saves a session's game over game `id` (or as a new game), returning
    /// its id.
    pub fn save(&self, id: Option<i64>, record: &GameRecord) -> Option<i64> {
        self.with("saving a game", |db| db.save_record(id, record))
    }
}

fn to_filter(f: &LibraryFilter) -> Filter {
    let text = |s: &Option<String>| s.as_deref().map(str::trim).filter(|s| !s.is_empty()).map(String::from);
    Filter {
        player: text(&f.player),
        opponent: text(&f.opponent),
        side: f.side.map(Color::letter),
        from: f.from,
        to: f.to,
        origin: match f.origin.as_deref() {
            Some("arimaa") => Some(Origin::ArimaaCom),
            Some("local") => Some(Origin::Local),
            _ => None,
        },
        rated: f.rated,
        outcome: match f.outcome.as_deref() {
            Some("won") => Some(Outcome::Won),
            Some("lost") => Some(Outcome::Lost),
            _ => None,
        },
        termination: f.termination.as_deref().and_then(|t| t.chars().next()),
        gold_setup: text(&f.gold_setup),
        silver_setup: text(&f.silver_setup),
    }
}

fn open_at(path: &Path) -> howdah_db::Result<Database> {
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir)?;
    }
    Database::open(path)
}

fn howdah_now() -> i64 {
    std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map_or(0, |d| d.as_secs() as i64)
}

/// What a session last saved: the session's generation and the record.
pub type Saved = Option<(u64, GameRecord)>;

fn lock(m: &Mutex<Session>) -> MutexGuard<'_, Session> {
    m.lock().unwrap_or_else(|e| e.into_inner())
}

/// Saves `session`'s game if it changed since `saved` and links the
/// session to the saved game, writes the analysis found since last time,
/// and brings in what the database has on positions not looked up yet.
/// Returns whether that changed what the session shows.
pub fn save_session(library: &Library, session: &Mutex<Session>, saved: &Mutex<Saved>) -> bool {
    save_game(library, session, saved);
    sync_evals(library, session)
}

fn save_game(library: &Library, session: &Mutex<Session>, saved: &Mutex<Saved>) {
    let (record, generation, id) = {
        let s = lock(session);
        match s.saveable() {
            Some(r) => (r, s.generation(), s.db_id()),
            None => return,
        }
    };
    let mut saved = saved.lock().unwrap_or_else(|e| e.into_inner());
    if let Some((g, r)) = saved.as_ref()
        && *g == generation
        && *r == record
        && id.is_some()
    {
        return;
    }
    let Some(new_id) = library.save(id, &record) else { return };
    {
        let mut s = lock(session);
        if s.generation() == generation {
            s.set_db_id(Some(new_id));
        }
    }
    *saved = Some((generation, record));
}

/// Writes new analysis and reads stored analysis for new positions.
fn sync_evals(library: &Library, session: &Mutex<Session>) -> bool {
    if !library.is_open() {
        return false;
    }
    let (authors, unsaved, lookups, generation) = {
        let mut s = lock(session);
        (s.engine_authors().clone(), s.take_unsaved_evals(), s.take_eval_lookups(), s.generation())
    };
    library.learn_authors(&authors);
    let writes: Vec<PositionDatum> = unsaved
        .into_iter()
        .filter_map(|(engine, hash, line)| {
            let author = authors.get(&engine)?.clone();
            Some(PositionDatum { hash, author, payload: StoredEval::from_line(&line).encode() })
        })
        .collect();
    library.put_evals(&writes);
    if lookups.is_empty() {
        return false;
    }
    let hashes: Vec<u64> = lookups.iter().map(|&(_, h)| h).collect();
    let found = library.get_evals(&hashes);
    if found.is_empty() {
        return false;
    }
    let mut s = lock(session);
    if s.generation() != generation {
        return false;
    }
    let mut changed = false;
    for (engine, hash, stored) in found {
        for &(node, _) in lookups.iter().filter(|&&(_, h)| h == hash) {
            changed |= s.merge_stored_eval(&engine, node, &stored);
        }
    }
    changed
}

/// The author an engine's evals are kept under: its `id name` (else the
/// name it has in the engine list) and its version (`id version`, else
/// the release it was installed from), so each version's evals are its
/// own.
pub fn engine_author(id_name: Option<&str>, id_version: Option<&str>, listed: &str) -> String {
    let name = id_name.map(str::trim).filter(|n| !n.is_empty()).unwrap_or(listed.trim());
    match id_version.map(str::trim).filter(|v| !v.is_empty()) {
        Some(v) if !name.contains(v) => format!("{name} {v}"),
        _ => name.to_string(),
    }
}

/// An engine's line as kept in the database (kind `eval`), as compact JSON
/// with its fields in a fixed order, so the same line is always the same
/// bytes. The PV is the turns in notation, checked again when read.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct StoredEval {
    /// The format's version.
    pub v: u32,
    pub depth: Option<String>,
    pub eval: Option<StoredScore>,
    pub nodes: Option<u64>,
    pub pv: Vec<String>,
    #[serde(rename = "timeMs")]
    pub time_ms: Option<u64>,
}

/// A score from gold's side: centi-rabbits (`cr`, with gold's chance of
/// winning in tenths of a percent when the engine gave one), or a proven
/// win (`win`: `g` or `s`, with the winner's turns to it).
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum StoredScore {
    CentiRabbits { cr: i32, chance: Option<u16> },
    Decided { win: char, turns: Option<u32> },
}

impl StoredEval {
    pub fn from_line(line: &AnalysisLine) -> StoredEval {
        StoredEval {
            v: 1,
            depth: line.depth.clone(),
            eval: line.eval.map(|e| match e {
                Eval::CentiRabbits { value, chance } => StoredScore::CentiRabbits { cr: value, chance },
                Eval::Decided { winner, turns } => StoredScore::Decided { win: winner.letter(), turns },
            }),
            nodes: line.nodes,
            pv: line.pv.iter().map(|t| t.notation.clone()).collect(),
            time_ms: line.time_ms,
        }
    }

    pub fn encode(&self) -> Vec<u8> {
        serde_json::to_vec(self).expect("plain data")
    }

    pub fn decode(bytes: &[u8]) -> Option<StoredEval> {
        serde_json::from_slice::<StoredEval>(bytes).ok().filter(|e| e.v == 1)
    }

    /// The line at `node`, with `pv` as checked from there.
    pub fn line(&self, node: NodeId, pv: Vec<PvTurn>) -> AnalysisLine {
        AnalysisLine {
            node,
            depth: self.depth.clone(),
            eval: self.eval.as_ref().and_then(|e| match *e {
                StoredScore::CentiRabbits { cr, chance } => Some(Eval::CentiRabbits { value: cr, chance }),
                StoredScore::Decided { win, turns } => {
                    Color::from_letter(win).map(|winner| Eval::Decided { winner, turns })
                }
            }),
            pv,
            nodes: self.nodes,
            time_ms: self.time_ms,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn saves_a_session_once_per_change() {
        let library = Library::in_memory();
        let session = Mutex::new(Session::new());
        let saved = Mutex::new(None);
        save_session(&library, &session, &saved);
        assert_eq!(session.lock().unwrap().db_id(), None, "an empty board isn't saved");

        let record = GameRecord::parse(
            "1g Ra1 Rb1 Rc1 Rd1 Re1 Rf1 Rg1 Rh1 Da2 Hb2 Cc2 Ed2 Me2 Hf2 Cg2 Dh2\n\
             1s ra8 rb8 rc8 rd8 re8 rf8 rg8 rh8 da7 hb7 cc7 ed7 me7 hf7 cg7 dh7\n2g Ed2n\n",
        )
        .unwrap();
        session.lock().unwrap().load_record(record);
        save_session(&library, &session, &saved);
        let id = session.lock().unwrap().db_id().expect("saved");
        save_session(&library, &session, &saved);
        assert_eq!(library.with("count", |db| db.game_count()), Some(1));

        // Another game loaded is another game saved.
        let mut other = session.lock().unwrap().saveable().unwrap();
        let end = *other.tree.main_line().last().unwrap();
        other.tree.add_notation(end, "ed7s").unwrap();
        session.lock().unwrap().load_record(other);
        assert_eq!(session.lock().unwrap().db_id(), None);
        save_session(&library, &session, &saved);
        assert_ne!(session.lock().unwrap().db_id(), Some(id));
        assert_eq!(library.with("count", |db| db.game_count()), Some(2));
    }

    #[test]
    fn authors_name_the_version() {
        assert_eq!(engine_author(Some("OpFor"), Some("2026.10.6"), "OpFor (mine)"), "OpFor 2026.10.6");
        assert_eq!(engine_author(Some("bot_Sharp 2026"), Some("2026"), "Sharp"), "bot_Sharp 2026");
        assert_eq!(engine_author(None, None, "My engine"), "My engine");
    }

    #[test]
    fn evals_are_kept_by_position_and_found_from_another_game() {
        use crate::dto::Eval;
        let library = Library::in_memory();
        let game = |extra: &str| {
            GameRecord::parse(&format!(
                "1g Ra1 Rb1 Rc1 Rd1 Re1 Rf1 Rg1 Rh1 Da2 Hb2 Cc2 Ed2 Me2 Hf2 Cg2 Dh2\n\
                 1s ra8 rb8 rc8 rd8 re8 rf8 rg8 rh8 da7 hb7 cc7 ed7 me7 hf7 cg7 dh7\n2g Ed2n\n{extra}"
            ))
            .unwrap()
        };
        let session = Mutex::new(Session::new());
        let saved = Mutex::new(None);
        session.lock().unwrap().load_record(game(""));
        let (node, generation) = {
            let s = session.lock().unwrap();
            (*s.saveable().unwrap().tree.main_line().last().unwrap(), s.generation())
        };
        let line = AnalysisLine {
            node,
            depth: Some("12+".into()),
            eval: Some(Eval::CentiRabbits { value: -35, chance: None }),
            pv: Vec::new(),
            nodes: Some(1000),
            time_ms: Some(5000),
        };
        {
            let mut s = session.lock().unwrap();
            s.store_analysis(generation, "sharp", &line);
            // Nothing is written until the engine's author is known.
            assert!(s.take_unsaved_evals().is_empty());
            s.set_engine_author("sharp", "bot_Sharp 2026".into());
        }
        save_session(&library, &session, &saved);
        assert_eq!(library.authors().get("sharp").map(String::as_str), Some("bot_Sharp 2026"));

        // Another board, another game through the same position: the line
        // comes back, without the engine having run there.
        let other = Mutex::new(Session::new());
        other.lock().unwrap().load_record(game("2s ed7s\n"));
        assert!(save_session(&library, &other, &Mutex::new(None)));
        let s = other.lock().unwrap();
        let tree = s.saveable().unwrap().tree;
        let at = tree.main_line()[3];
        let found = s.stored_analysis(s.generation(), "sharp", at).expect("found by position");
        assert_eq!((found.depth.as_deref(), found.eval, found.time_ms), (Some("12+"), line.eval, Some(5000)));
    }

    #[test]
    fn fetched_games_are_found_again() {
        let library = Library::in_memory();
        let mut record = GameRecord::parse(
            "1g Ra1 Rb1 Rc1 Rd1 Re1 Rf1 Rg1 Rh1 Da2 Hb2 Cc2 Ed2 Me2 Hf2 Cg2 Dh2\n\
             1s ra8 rb8 rc8 rd8 re8 rf8 rg8 rh8 da7 hb7 cc7 ed7 me7 hf7 cg7 dh7\n2g Ed2n\n",
        )
        .unwrap();
        record.set_tag("GameId", "93252");
        assert!(library.arimaa_game("93252").is_none());
        let id = library.keep_fetched(&record).unwrap();
        let (found, back) = library.arimaa_game("93252").unwrap();
        assert_eq!(found, id);
        assert_eq!(back.tag("GameId"), Some("93252"));

        // A watched game saved without its id first, then given it,
        // merges into the kept game.
        let mut live = record.clone();
        live.tags.retain(|(n, _)| n != "GameId");
        let own = library.save(None, &live).unwrap();
        assert_ne!(own, id);
        assert_eq!(library.save(Some(own), &record), Some(id));
        assert_eq!(library.with("count", |db| db.game_count()), Some(1));
    }
}
