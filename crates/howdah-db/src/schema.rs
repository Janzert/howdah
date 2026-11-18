//! Schema versions and migrations (`docs/DATABASE.md`, "Schema versions
//! and migrations").
//!
//! `PRAGMA application_id` marks the file as Howdah's and `PRAGMA
//! user_version` is its schema version. Opening a database runs the
//! migrations past its version, each in its own transaction that also
//! sets the new version. Migrations are never edited once released; a fix
//! is a new one.

use rusqlite::{Connection, Transaction};

use crate::{Error, Result, now};

/// "HwDb" in the file header, so a stray SQLite file isn't migrated.
pub const APPLICATION_ID: i32 = 0x4877_4462;

pub(crate) struct Migration {
    pub name: &'static str,
    pub step: Step,
    /// Whether a reader that knows only the versions before this one can
    /// still read the file after it (an added table or column: yes; a
    /// changed encoding: no).
    pub readable_before: bool,
}

pub(crate) enum Step {
    Sql(&'static str),
    #[allow(dead_code)]
    Rust(fn(&Transaction) -> rusqlite::Result<()>),
}

/// Version `n` is the database after the first `n` migrations.
pub(crate) const MIGRATIONS: &[Migration] = &[
    Migration {
        name: "0001_init",
        step: Step::Sql(include_str!("../migrations/0001_init.sql")),
        readable_before: false,
    },
    Migration {
        name: "0002_list_order",
        step: Step::Sql(include_str!("../migrations/0002_list_order.sql")),
        readable_before: true,
    },
    Migration {
        name: "0003_local_engines",
        step: Step::Sql(include_str!("../migrations/0003_local_engines.sql")),
        readable_before: true,
    },
];

/// The schema version this build writes.
pub const VERSION: i64 = MIGRATIONS.len() as i64;

pub(crate) fn version(conn: &Connection) -> rusqlite::Result<i64> {
    conn.pragma_query_value(None, "user_version", |r| r.get(0))
}

fn application_id(conn: &Connection) -> rusqlite::Result<i32> {
    conn.pragma_query_value(None, "application_id", |r| r.get(0))
}

/// The oldest schema version that can read the file.
pub(crate) fn min_reader(conn: &Connection) -> Result<i64> {
    let v: Option<String> =
        conn.query_row("SELECT value FROM meta WHERE key = 'min_reader'", [], |r| r.get(0)).ok();
    Ok(v.and_then(|v| v.parse().ok()).unwrap_or(1))
}

/// Whether the database can be read by this build, and whether it needs
/// migrating (or, if newer, may only be read).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Compat {
    Current,
    Older(i64),
    /// Newer than this build, but readable by it.
    NewerReadable(i64),
}

/// Checks the file is a Howdah database (or empty) and how its version
/// compares.
pub(crate) fn check(conn: &Connection) -> Result<Compat> {
    let id = application_id(conn)?;
    let v = version(conn)?;
    if id != APPLICATION_ID {
        let empty: i64 = conn.query_row("SELECT count(*) FROM sqlite_master", [], |r| r.get(0))?;
        if id != 0 || v != 0 || empty != 0 {
            return Err(Error::NotHowdah);
        }
        return Ok(Compat::Older(0));
    }
    if v == VERSION {
        Ok(Compat::Current)
    } else if v < VERSION {
        Ok(Compat::Older(v))
    } else if min_reader(conn)? <= VERSION {
        Ok(Compat::NewerReadable(v))
    } else {
        Err(Error::TooNew { version: v, min_reader: min_reader(conn)? })
    }
}

/// Runs the migrations past the database's version.
pub(crate) fn migrate(conn: &mut Connection) -> Result<()> {
    let from = match check(conn)? {
        Compat::Current | Compat::NewerReadable(_) => return Ok(()),
        Compat::Older(v) => v,
    };
    for (i, m) in MIGRATIONS.iter().enumerate().skip(from as usize) {
        let target = i as i64 + 1;
        let tx = conn.transaction()?;
        match m.step {
            Step::Sql(sql) => tx.execute_batch(sql),
            Step::Rust(f) => f(&tx),
        }
        .map_err(|source| Error::Migration { name: m.name, source })?;
        if target == 1 {
            tx.pragma_update(None, "application_id", APPLICATION_ID)?;
            let uuid = crate::random_id();
            tx.execute(
                "INSERT INTO meta (key, value) VALUES ('uuid', ?1), ('created', ?2)",
                (crate::hex(&uuid), now().to_string()),
            )?;
            tx.execute(
                "INSERT INTO origins (id, uuid, name, added_at) VALUES (1, ?1, '', ?2)",
                (uuid.as_slice(), now()),
            )?;
        }
        let min_reader = if m.readable_before { min_reader(&tx)? } else { target };
        tx.execute(
            "INSERT INTO meta (key, value) VALUES ('min_reader', ?1)
             ON CONFLICT (key) DO UPDATE SET value = excluded.value",
            [min_reader.to_string()],
        )?;
        tx.pragma_update(None, "user_version", target)?;
        tx.commit()?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fresh_database_is_current() {
        let mut conn = Connection::open_in_memory().unwrap();
        migrate(&mut conn).unwrap();
        assert_eq!(check(&conn).unwrap(), Compat::Current);
        assert_eq!(min_reader(&conn).unwrap(), 1, "every step since the first is readable by version 1");
        migrate(&mut conn).unwrap();
    }

    #[test]
    fn migrates_from_each_version() {
        // A database made at version 1 comes up to the newest, and ends up
        // as a fresh one does.
        let schema = |c: &Connection| -> Vec<String> {
            let mut q =
                c.prepare("SELECT sql FROM sqlite_master WHERE sql IS NOT NULL ORDER BY name").unwrap();
            q.query_map([], |r| r.get(0)).unwrap().collect::<rusqlite::Result<_>>().unwrap()
        };
        let mut fresh = Connection::open_in_memory().unwrap();
        migrate(&mut fresh).unwrap();
        let mut old = Connection::open_in_memory().unwrap();
        let tx = old.transaction().unwrap();
        tx.execute_batch(include_str!("../migrations/0001_init.sql")).unwrap();
        tx.pragma_update(None, "application_id", APPLICATION_ID).unwrap();
        tx.pragma_update(None, "user_version", 1).unwrap();
        tx.execute("INSERT INTO meta (key, value) VALUES ('min_reader', '1')", []).unwrap();
        tx.commit().unwrap();
        assert_eq!(check(&old).unwrap(), Compat::Older(1));
        migrate(&mut old).unwrap();
        assert_eq!(check(&old).unwrap(), Compat::Current);
        assert_eq!(schema(&old), schema(&fresh));
        // Adding an index leaves the file readable by version 1.
        assert_eq!(min_reader(&old).unwrap(), 1);
    }

    #[test]
    fn refuses_other_sqlite_files() {
        let mut conn = Connection::open_in_memory().unwrap();
        conn.execute_batch("CREATE TABLE t (x)").unwrap();
        assert!(matches!(migrate(&mut conn), Err(Error::NotHowdah)));
    }

    #[test]
    fn newer_versions_by_min_reader() {
        let mut conn = Connection::open_in_memory().unwrap();
        migrate(&mut conn).unwrap();
        conn.pragma_update(None, "user_version", VERSION + 1).unwrap();
        assert_eq!(check(&conn).unwrap(), Compat::NewerReadable(VERSION + 1));
        conn.execute("UPDATE meta SET value = ?1 WHERE key = 'min_reader'", [(VERSION + 1).to_string()])
            .unwrap();
        assert!(matches!(check(&conn), Err(Error::TooNew { .. })));
    }
}
