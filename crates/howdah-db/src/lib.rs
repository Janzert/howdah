//! Howdah's game database (design in `docs/DATABASE.md`).
//!
//! One SQLite file holds games (headers as columns, the main line as a
//! compact blob, [`codec`]), their variations and annotations, players,
//! and data on positions. [`Database`] opens or creates one, migrating it
//! to this build's schema ([`schema`]); [`GameData`] is a game as the
//! database stores it, converted to and from a [`howdah_arimaa::GameRecord`].
//! [`archive`] imports arimaa.com's game archive files.

pub mod archive;
pub mod base;
pub mod codec;
mod game;
mod merge;
mod positions;
pub mod schema;
mod search;

use std::path::Path;
use std::time::{SystemTime, UNIX_EPOCH};

use rusqlite::{Connection, OpenFlags, OptionalExtension};

pub use game::{ARIMAA_COM, GameData, PlayerInfo, SaveMode, SourceKey, StoredResult, gid_for};
pub use merge::MergeReport;
pub use positions::PositionDatum;
pub use search::{Filter, GameSummary, Origin, Outcome, SearchPage};

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error(transparent)]
    Sqlite(#[from] rusqlite::Error),
    #[error("not a Howdah database")]
    NotHowdah,
    #[error("the database is version {version}, which needs Howdah's schema {min_reader} or newer")]
    TooNew { version: i64, min_reader: i64 },
    #[error("the database is newer than this version of Howdah, so it's open read-only")]
    ReadOnly,
    #[error("migration {name}: {source}")]
    Migration { name: &'static str, source: rusqlite::Error },
    #[error("game {0}: {1}")]
    Moves(i64, codec::CodecError),
    #[error(transparent)]
    Codec(#[from] codec::CodecError),
    #[error("no game {0}")]
    NoGame(i64),
    #[error(transparent)]
    Io(#[from] std::io::Error),
    #[error("archive: {0}")]
    Archive(String),
}

pub type Result<T, E = Error> = std::result::Result<T, E>;

/// A game database.
pub struct Database {
    conn: Connection,
    read_only: bool,
}

impl Database {
    /// Opens the database at `path`, creating it if there's none, and
    /// brings it up to this build's schema. A database newer than this
    /// build that it can still read opens read-only.
    pub fn open(path: impl AsRef<Path>) -> Result<Database> {
        Database::setup(Connection::open(path)?)
    }

    /// Opens a database for reading only. It isn't migrated, so it must be
    /// this build's version or one it can read.
    pub fn open_read_only(path: impl AsRef<Path>) -> Result<Database> {
        let conn = Connection::open_with_flags(path, OpenFlags::SQLITE_OPEN_READ_ONLY)?;
        match schema::check(&conn)? {
            schema::Compat::Older(_) => Err(Error::ReadOnly),
            _ => Ok(Database { conn, read_only: true }),
        }
    }

    pub fn open_in_memory() -> Result<Database> {
        Database::setup(Connection::open_in_memory()?)
    }

    fn setup(mut conn: Connection) -> Result<Database> {
        conn.pragma_update(None, "foreign_keys", true)?;
        conn.pragma_update(None, "journal_mode", "WAL")?;
        let compat = schema::check(&conn)?;
        schema::migrate(&mut conn)?;
        Ok(Database { conn, read_only: matches!(compat, schema::Compat::NewerReadable(_)) })
    }

    pub fn connection(&self) -> &Connection {
        &self.conn
    }

    pub fn is_read_only(&self) -> bool {
        self.read_only
    }

    fn writable(&mut self) -> Result<&mut Connection> {
        if self.read_only {
            return Err(Error::ReadOnly);
        }
        Ok(&mut self.conn)
    }

    /// A fact about this database (`meta`).
    pub fn meta(&self, key: &str) -> Result<Option<String>> {
        Ok(self.conn.query_row("SELECT value FROM meta WHERE key = ?1", [key], |r| r.get(0)).optional()?)
    }

    pub fn set_meta(&mut self, key: &str, value: &str) -> Result<()> {
        self.writable()?.execute(
            "INSERT INTO meta (key, value) VALUES (?1, ?2) ON CONFLICT (key) DO UPDATE SET value = excluded.value",
            (key, value),
        )?;
        Ok(())
    }

    /// This database's id.
    pub fn uuid(&self) -> Result<String> {
        Ok(self.conn.query_row("SELECT value FROM meta WHERE key = 'uuid'", [], |r| r.get(0))?)
    }

    /// Saves a game: a new one if it has no `id` and no game has its
    /// `gid` or source key, otherwise over the one that does. Returns its
    /// id.
    pub fn save(&mut self, game: &GameData) -> Result<i64> {
        self.save_as(game, SaveMode::User)
    }

    /// Saves a game as [`Database::save`] does, as the user's copy or a
    /// source's ([`SaveMode`]).
    pub fn save_as(&mut self, game: &GameData, mode: SaveMode) -> Result<i64> {
        let tx = self.writable()?.transaction()?;
        let id = game::save(&tx, game, &mut game::PlayerCache::default(), mode)?;
        tx.commit()?;
        Ok(id)
    }

    /// Saves a record over game `id` (or as a new game): its tags and
    /// moves replace what the record has, and headers it doesn't carry
    /// (player ids, start and end times, ratings' uncertainty) stay as
    /// stored. Returns the game's id.
    ///
    /// A record whose source key (an arimaa.com `GameId`) belongs to
    /// another game is saved over that one, and game `id` deleted: the two
    /// were the same game.
    pub fn save_record(&mut self, id: Option<i64>, record: &howdah_arimaa::GameRecord) -> Result<i64> {
        let mut id = id;
        if let Some(source) = GameData::from_record(record).source
            && let Some(other) = self.find_source(&source.source, &source.id)?
            && Some(other) != id
        {
            if let Some(old) = id {
                self.delete(old)?;
            }
            id = Some(other);
        }
        // A game deleted while a board still had it comes back as a new one.
        let stored = match id {
            Some(id) => match self.load_header(id) {
                Ok(g) => Some(g),
                Err(Error::NoGame(_)) => None,
                Err(e) => return Err(e),
            },
            None => None,
        };
        let mut game = stored.unwrap_or_else(|| GameData::from_record(record));
        game.merge_record(record);
        self.save(&game)
    }

    /// A game's headers and tags, without its moves (`tree` is `None`).
    pub fn load_header(&self, id: i64) -> Result<GameData> {
        game::load_header(&self.conn, id)
    }

    /// Loads a game with its moves, variations and annotations.
    pub fn load(&self, id: i64) -> Result<GameData> {
        game::load(&self.conn, id)
    }

    /// The game with a source key (an arimaa.com permanent id).
    pub fn find_source(&self, source: &str, source_id: &str) -> Result<Option<i64>> {
        Ok(self
            .conn
            .query_row(
                "SELECT id FROM games WHERE source = ?1 AND source_id = ?2",
                (source, source_id),
                |r| r.get(0),
            )
            .optional()?)
    }

    /// Whether a game has its moves (not only its header).
    pub fn has_moves(&self, id: i64) -> Result<bool> {
        Ok(self.conn.query_row("SELECT moves IS NOT NULL FROM games WHERE id = ?1", [id], |r| r.get(0))?)
    }

    pub fn game_count(&self) -> Result<i64> {
        Ok(self.conn.query_row("SELECT count(*) FROM games", [], |r| r.get(0))?)
    }

    pub fn delete(&mut self, id: i64) -> Result<()> {
        self.writable()?.execute("DELETE FROM games WHERE id = ?1", [id])?;
        Ok(())
    }
}

pub(crate) fn now() -> i64 {
    SystemTime::now().duration_since(UNIX_EPOCH).map_or(0, |d| d.as_secs() as i64)
}

pub(crate) fn random_id() -> [u8; 16] {
    let mut id = [0u8; 16];
    getrandom::fill(&mut id).expect("the system's random numbers");
    id
}

pub(crate) fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}
