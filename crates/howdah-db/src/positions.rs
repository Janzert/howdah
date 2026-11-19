//! Data on positions (`position_data`): engine evals and the like, keyed
//! by a position's Zobrist hash (side to move included), so every game and
//! line that reaches the position shares it.

use rusqlite::{OptionalExtension, params};

use crate::{Database, Result, now};

/// One datum: the position's hash, its author (an engine's name and
/// version, for evals) and the payload, as stored.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PositionDatum {
    pub hash: u64,
    pub author: String,
    pub payload: Vec<u8>,
}

/// Hashes are `u64`; SQLite's integers are signed, so they're stored as
/// the same bits.
fn key(hash: u64) -> i64 {
    hash as i64
}

impl Database {
    /// Writes data of `kind`, replacing what the same author had for the
    /// same position (keeping when it was first written).
    pub fn put_position_data(&mut self, kind: &str, data: &[PositionDatum]) -> Result<()> {
        let tx = self.writable()?.transaction()?;
        {
            let mut upsert = tx.prepare_cached(
                "INSERT INTO position_data (hash, kind, author, origin, created_at, modified_at, payload)
                 VALUES (?1, ?2, ?3, 1, ?4, ?4, ?5)
                 ON CONFLICT (hash, kind, author) DO UPDATE SET
                   modified_at = CASE WHEN payload = excluded.payload THEN modified_at
                                      ELSE excluded.modified_at END,
                   origin = CASE WHEN payload = excluded.payload THEN origin ELSE 1 END,
                   payload = excluded.payload",
            )?;
            let t = now();
            for d in data {
                upsert.execute(params![key(d.hash), kind, d.author, t, d.payload])?;
            }
        }
        tx.commit()?;
        Ok(())
    }

    /// The data of `kind` on the positions `hashes`, by any of `authors`.
    pub fn position_data(
        &self,
        kind: &str,
        hashes: &[u64],
        authors: &[String],
    ) -> Result<Vec<PositionDatum>> {
        let mut q = self
            .connection()
            .prepare_cached("SELECT author, payload FROM position_data WHERE hash = ?1 AND kind = ?2")?;
        let mut out = Vec::new();
        for &hash in hashes {
            let rows = q.query_map(params![key(hash), kind], |r| Ok((r.get::<_, String>(0)?, r.get(1)?)))?;
            for row in rows {
                let (author, payload) = row?;
                if authors.contains(&author) {
                    out.push(PositionDatum { hash, author, payload });
                }
            }
        }
        Ok(out)
    }

    /// The authors with data of `kind` (for evals, the engines), most
    /// data first.
    pub fn position_authors(&self, kind: &str) -> Result<Vec<(String, i64)>> {
        let mut q = self.connection().prepare(
            "SELECT author, count(*) FROM position_data WHERE kind = ?1 GROUP BY author ORDER BY 2 DESC, 1",
        )?;
        Ok(q.query_map([kind], |r| Ok((r.get(0)?, r.get(1)?)))?.collect::<rusqlite::Result<_>>()?)
    }

    /// The app's engines and the authors their data is kept under.
    pub fn local_engines(&self) -> Result<Vec<(String, String)>> {
        let mut q = self.connection().prepare("SELECT engine_id, author FROM local_engines")?;
        Ok(q.query_map([], |r| Ok((r.get(0)?, r.get(1)?)))?.collect::<rusqlite::Result<_>>()?)
    }

    /// Records the author engine `engine_id` (its id in the app's engine
    /// list) writes as.
    pub fn set_local_engine(&mut self, engine_id: &str, author: &str) -> Result<()> {
        let known: Option<String> = self
            .connection()
            .query_row("SELECT author FROM local_engines WHERE engine_id = ?1", [engine_id], |r| r.get(0))
            .optional()?;
        if known.as_deref() != Some(author) {
            self.writable()?.execute(
                "INSERT INTO local_engines (engine_id, author) VALUES (?1, ?2)
                 ON CONFLICT (engine_id) DO UPDATE SET author = excluded.author",
                (engine_id, author),
            )?;
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn data_by_position_and_author() {
        let mut db = Database::open_in_memory().unwrap();
        let datum = |hash, author: &str, p: &str| PositionDatum {
            hash,
            author: author.into(),
            payload: p.as_bytes().to_vec(),
        };
        // A hash with the top bit set survives the signed column.
        let big = u64::MAX - 5;
        db.put_position_data("eval", &[datum(big, "Sharp 1", "a"), datum(7, "OpFor 2", "b")]).unwrap();
        db.put_position_data("eval", &[datum(big, "Sharp 1", "c")]).unwrap();
        let sharp = ["Sharp 1".to_string()];
        assert_eq!(db.position_data("eval", &[big, 7], &sharp).unwrap(), [datum(big, "Sharp 1", "c")]);
        let both = ["Sharp 1".to_string(), "OpFor 2".to_string()];
        assert_eq!(db.position_data("eval", &[7], &both).unwrap(), [datum(7, "OpFor 2", "b")]);
        assert!(db.position_data("other", &[7], &both).unwrap().is_empty());

        db.set_local_engine("sharp", "Sharp 1").unwrap();
        db.set_local_engine("sharp", "Sharp 2").unwrap();
        assert_eq!(db.local_engines().unwrap(), [("sharp".to_string(), "Sharp 2".to_string())]);
    }
}
