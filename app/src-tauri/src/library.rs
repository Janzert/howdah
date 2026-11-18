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

use howdah_arimaa::GameRecord;
use howdah_db::{ARIMAA_COM, Database, GameData, SaveMode};

use crate::session::Session;

/// How often a session's game is checked for changes to save.
pub const SAVE_EVERY: Duration = Duration::from_secs(2);

/// The file's name in the app's data directory.
pub const FILE_NAME: &str = "games.db";

pub struct Library {
    db: Mutex<Option<Database>>,
    path: Option<PathBuf>,
}

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
        Library { db: Mutex::new(db), path }
    }

    /// A library in memory, for tests.
    #[cfg(test)]
    pub fn in_memory() -> Library {
        Library {
            db: Mutex::new(Some(Database::open_in_memory().expect("an in-memory database"))),
            path: None,
        }
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

/// Saves `session`'s game if it changed since `saved`, and links the
/// session to the saved game.
pub fn save_session(library: &Library, session: &Mutex<Session>, saved: &Mutex<Saved>) {
    fn lock(m: &Mutex<Session>) -> MutexGuard<'_, Session> {
        m.lock().unwrap_or_else(|e| e.into_inner())
    }
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
