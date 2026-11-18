//! arimaa.com's game archive files (`allgamesYYYYMM.zip`, and yearly
//! `allgamesYYYY.zip` to 2004): one tab-separated text file per zip, a
//! header row, then a game per row with newlines in fields escaped as
//! `\n` (format notes in `docs/VARIATIONS.md`).
//!
//! Each row becomes a game keyed by its permanent id. The moves are
//! replayed with the rules; a move that doesn't replay ends the game's
//! moves there, with the reason in `replay_error` and the row's move list
//! kept. Move times come from the server log's clock lines (`after 5w,
//! move used:N`), or for older games from when each move arrived. The log
//! itself isn't kept.

use std::collections::HashMap;
use std::io::Read;
use std::path::{Path, PathBuf};
use std::time::UNIX_EPOCH;

use howdah_arimaa::GameTree;
use rusqlite::OptionalExtension;

use crate::game::{self, PlayerCache, utc_date};
use crate::{Database, Error, GameData, PlayerInfo, Result, SourceKey, StoredResult, now};

/// One row of an archive file, by the header's column names.
pub struct Row<'a> {
    columns: &'a HashMap<String, usize>,
    fields: Vec<&'a str>,
}

impl<'a> Row<'a> {
    pub fn get(&self, name: &str) -> Option<&'a str> {
        let v = self.fields.get(*self.columns.get(name)?)?.trim();
        (!v.is_empty()).then_some(v)
    }

    fn int(&self, name: &str) -> Option<i64> {
        self.get(name)?.parse().ok()
    }

    fn flag(&self, name: &str) -> Option<bool> {
        self.get(name).map(|v| v != "0")
    }

    fn player(&self, side: char) -> PlayerInfo {
        let col = |c: &str| format!("{side}{c}");
        PlayerInfo {
            name: self.get(&col("username")).map(String::from),
            site_id: self.get(&col("playerid")).map(String::from),
            kind: self.get(&col("type")).and_then(|t| t.chars().next()),
            country: self.get(&col("country")).map(String::from),
            title: self.get(&col("title")).map(String::from),
            rating: self.int(&col("rating")),
            rating_k: self.int(&col("ratingk")),
        }
    }
}

/// The game in a row.
pub fn game_from_row(row: &Row) -> Option<GameData> {
    let id = row.get("id")?;
    let mut g = GameData::new();
    g.source = Some(SourceKey::arimaa_com(id));
    g.gid = game::gid_for(g.source.as_ref().expect("set"));
    g.players = [row.player('w'), row.player('b')];
    g.event = row.get("event").map(String::from);
    g.site = row.get("site").map(String::from);
    g.time_control = row.get("timecontrol").map(String::from);
    g.rated = row.flag("rated");
    g.postal = row.flag("postal");
    g.start_ts = row.int("startts").filter(|&t| t > 0);
    g.end_ts = row.int("endts").filter(|&t| t > 0);
    g.date = g.start_ts.or(g.end_ts).map(utc_date);
    g.result = row.get("result").and_then(|w| w.chars().next()).map(|w| StoredResult {
        winner: match w {
            'w' => 'g',
            'b' => 's',
            other => other,
        },
        termination: row.get("termination").and_then(|t| t.chars().next()),
    });
    if let Some(mode) = row.get("mode") {
        g.tags.push(("Mode".into(), mode.into()));
    }
    if row.flag("corrupt") == Some(true) {
        g.tags.push(("Corrupt".into(), "1".into()));
    }
    let movelist = row.get("movelist").unwrap_or("").replace("\\n", "\n");
    let old = g.end_ts.or(g.start_ts).is_some_and(|t| t < OLD_RULES_UNTIL);
    let (tree, error) = replay(&movelist, old);
    if let Some(events) = row.get("events") {
        let main = tree.main_line();
        let times = move_times(events, main.len() - 1);
        if times.iter().any(Option::is_some) {
            let mut tree = tree;
            for (node, ms) in main[1..].iter().zip(times) {
                if let (Some(ms), Ok(a)) = (ms, tree.annotation_mut(*node)) {
                    a.set_elapsed(std::time::Duration::from_millis(ms));
                }
            }
            g.tree = Some(tree);
        } else {
            g.tree = Some(tree);
        }
    } else {
        g.tree = Some(tree);
    }
    if let Some(e) = error {
        g.replay_error = Some(e);
        g.movelist = Some(movelist);
    }
    Some(g)
}

/// When arimaa.com's old rules stopped applying: the last game played on
/// past an elimination ended at 18:05 UTC on 2008-06-18, and the first
/// game ended by one at 18:56 that day. The rules page dates the change
/// 2008-07-01.
pub const OLD_RULES_UNTIL: i64 = 1_213_813_800;

/// Plays a move list (`1w Ra1 …` a line) until a move fails, giving the
/// tree and why it stopped; with `old_rules`, by the rules before
/// 2008.07.01 ([`GameTree::set_old_rules`]).
pub fn replay(movelist: &str, old_rules: bool) -> (GameTree, Option<String>) {
    let mut tree = GameTree::new();
    tree.set_old_rules(old_rules);
    let mut at = tree.root();
    for line in movelist.lines().map(str::trim).filter(|l| !l.is_empty()) {
        let (label, body) = line.split_once(char::is_whitespace).unwrap_or((line, ""));
        // A takeback undoes the move before it, which stays as a
        // variation (as the record reader keeps it).
        if label.eq_ignore_ascii_case("takeback") || body.trim().eq_ignore_ascii_case("takeback") {
            if let Some(parent) = tree[at].parent() {
                at = parent;
            }
            continue;
        }
        if body.trim().is_empty() || !label.starts_with(|c: char| c.is_ascii_digit()) {
            continue;
        }
        match tree.add_notation(at, body) {
            Ok(node) => {
                // The move played after a takeback is the game's.
                let _ = tree.make_first(node);
                at = node;
            }
            Err(e) => return (tree, Some(format!("{label}: {e}"))),
        }
    }
    (tree, None)
}

/// Each move's time in milliseconds from a server log: the clock lines
/// (`after 1w, move used:70 …`), else the gaps between the lines saying
/// when moves arrived (`move 1w received`, from `start move received`).
pub fn move_times(events: &str, plies: usize) -> Vec<Option<u64>> {
    let mut used = Vec::new();
    let mut received = Vec::new();
    let mut start = None;
    for line in events.split("\\n") {
        let Some((ts, rest)) = line.split_once(' ') else { continue };
        let Ok(ts) = ts.parse::<i64>() else { continue };
        let what = rest.split_once("] ").map_or(rest, |(_, w)| w);
        if let Some(r) = what.strip_prefix("after ") {
            if let Some(n) = r.split_once("move used:").and_then(|(_, n)| n.split_whitespace().next()) {
                used.push(n.parse::<u64>().ok());
            }
        } else if what.starts_with("start move received") {
            start.get_or_insert(ts);
        } else if what.starts_with("move ") && what.contains(" received from ") {
            received.push(ts);
        }
    }
    let mut times: Vec<Option<u64>> = if !used.is_empty() {
        used.into_iter().map(|s| s.map(|s| s * 1000)).collect()
    } else if let Some(start) = start.filter(|_| received.len() == plies) {
        let mut prev = start;
        received
            .into_iter()
            .map(|t| {
                let d = (t - prev).max(0) as u64 * 1000;
                prev = t;
                Some(d)
            })
            .collect()
    } else {
        Vec::new()
    };
    times.resize(plies, None);
    times
}

/// What an import did.
#[derive(Clone, Debug, Default)]
pub struct Report {
    pub files: usize,
    pub skipped_files: usize,
    pub games: usize,
    pub replay_errors: usize,
}

/// The archive files to import from `paths` (files, or directories holding
/// them), oldest first. Only the `allgames` files have moves.
pub fn archive_files(paths: &[PathBuf]) -> Result<Vec<PathBuf>> {
    let mut files = Vec::new();
    for p in paths {
        if p.is_dir() {
            for entry in std::fs::read_dir(p)? {
                files.push(entry?.path());
            }
        } else {
            files.push(p.clone());
        }
    }
    files.retain(|f| {
        f.file_name()
            .and_then(|n| n.to_str())
            .is_some_and(|n| n.starts_with("allgames") && n.ends_with(".zip"))
    });
    // `allgames2004.zip` sorts before `allgames200501.zip`.
    files.sort_by_key(|f| f.file_name().map(|n| n.to_owned()));
    Ok(files)
}

fn read_zip(path: &Path) -> Result<Vec<u8>> {
    let file = std::fs::File::open(path)?;
    let mut zip =
        zip::ZipArchive::new(file).map_err(|e| Error::Archive(format!("{}: {e}", path.display())))?;
    let mut out = Vec::new();
    for i in 0..zip.len() {
        let mut entry = zip.by_index(i).map_err(|e| Error::Archive(format!("{}: {e}", path.display())))?;
        if entry.name().ends_with(".txt") {
            entry.read_to_end(&mut out)?;
        }
    }
    Ok(out)
}

/// Reads an archive file's games, replaying them on up to `threads`
/// threads.
pub fn read_file(path: &Path, threads: usize) -> Result<Vec<GameData>> {
    let bytes = read_zip(path)?;
    let text = String::from_utf8_lossy(&bytes);
    let mut lines = text.split('\n').map(|l| l.trim_end_matches('\r'));
    let header = lines.next().unwrap_or("");
    let columns: HashMap<String, usize> =
        header.split('\t').enumerate().map(|(i, c)| (c.trim().to_string(), i)).collect();
    if !columns.contains_key("id") || !columns.contains_key("movelist") {
        return Err(Error::Archive(format!("{}: no id and movelist columns", path.display())));
    }
    let rows: Vec<&str> = lines.filter(|l| !l.trim().is_empty()).collect();
    let threads = threads.clamp(1, 8);
    let chunk = rows.len().div_ceil(threads).max(1);
    let games = std::thread::scope(|s| {
        let handles: Vec<_> = rows
            .chunks(chunk)
            .map(|part| {
                let columns = &columns;
                s.spawn(move || {
                    part.iter()
                        .filter_map(|line| {
                            game_from_row(&Row { columns, fields: line.split('\t').collect() })
                        })
                        .collect::<Vec<_>>()
                })
            })
            .collect();
        handles.into_iter().flat_map(|h| h.join().expect("a replay thread")).collect()
    });
    Ok(games)
}

fn file_stamp(path: &Path) -> Result<(i64, i64)> {
    let meta = std::fs::metadata(path)?;
    let mtime = meta.modified()?.duration_since(UNIX_EPOCH).map_or(0, |d| d.as_secs() as i64);
    Ok((meta.len() as i64, mtime))
}

impl Database {
    /// Imports archive files, skipping ones imported before and unchanged
    /// since. A game already in the database is replaced by the file's.
    /// `progress` hears each file as it's done.
    pub fn import_archive(
        &mut self,
        files: &[PathBuf],
        threads: usize,
        mut progress: impl FnMut(&Path, usize),
    ) -> Result<Report> {
        let mut report = Report::default();
        let mut cache = PlayerCache::default();
        for path in files {
            let name = path.file_name().and_then(|n| n.to_str()).unwrap_or_default().to_string();
            let (size, mtime) = file_stamp(path)?;
            let seen: Option<(i64, i64)> = self
                .connection()
                .query_row("SELECT size, mtime FROM archive_files WHERE name = ?1", [&name], |r| {
                    Ok((r.get(0)?, r.get(1)?))
                })
                .optional()?;
            if seen == Some((size, mtime)) {
                report.skipped_files += 1;
                continue;
            }
            let games = read_file(path, threads)?;
            let tx = self.writable()?.transaction()?;
            for g in &games {
                game::save(&tx, g, &mut cache, game::SaveMode::Source)?;
                report.replay_errors += usize::from(g.replay_error.is_some());
            }
            tx.execute(
                "INSERT INTO archive_files (name, size, mtime, games, imported_at) VALUES (?1, ?2, ?3, ?4, ?5)
                 ON CONFLICT (name) DO UPDATE SET size = excluded.size, mtime = excluded.mtime,
                 games = excluded.games, imported_at = excluded.imported_at",
                (&name, size, mtime, games.len() as i64, now()),
            )?;
            tx.commit()?;
            report.files += 1;
            report.games += games.len();
            progress(path, games.len());
        }
        Ok(report)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const EVENTS: &str = "1541048964 [Thu Nov  1 05:09:24 2018] start move received from w\\n\
        1541049034 [Thu Nov  1 05:10:34 2018] move 1w received from w [Ra1]\\n\
        1541049034 [Thu Nov  1 05:10:34 2018] after 1w, move used:70 wresv:120 bresv:120 game:70\\n\
        1541049035 [Thu Nov  1 05:10:35 2018] move 1b received from b [ra8]\\n\
        1541049035 [Thu Nov  1 05:10:35 2018] after 1b, move used:1 wresv:120 bresv:120 game:71\\n";

    #[test]
    fn times_from_clock_lines() {
        assert_eq!(move_times(EVENTS, 3), [Some(70_000), Some(1000), None]);
    }

    #[test]
    fn times_from_arrivals() {
        let old: String =
            EVENTS.split("\\n").filter(|l| !l.contains("after")).collect::<Vec<_>>().join("\\n");
        assert_eq!(move_times(&old, 2), [Some(70_000), Some(1000)]);
        // Arrivals that don't match the moves give nothing.
        assert_eq!(move_times(&old, 3), [None, None, None]);
    }

    #[test]
    fn replay_stops_at_a_bad_move() {
        let (tree, error) = replay(
            "1w Ra1 Rb1 Rc1 Rd1 Re1 Rf1 Rg1 Rh1 Da2 Hb2 Cc2 Ed2 Me2 Hf2 Cg2 Dh2\n\
             1b ch7 hg7 df7 me7 ed7 dc7 hb7 ca7 rh8 rg8 rf8 re8 rd8 rc8 rb8 ra8\n\
             2w Ed2n Ed3n Me2n Me3n\n2b ra7s\n3w\n",
            false,
        );
        assert_eq!(tree.main_line().len(), 4);
        assert!(error.unwrap().starts_with("2b:"));
    }

    #[test]
    fn takebacks_leave_variations() {
        let (tree, error) = replay(
            "1w Ra1 Rb1 Rc1 Rd1 Re1 Rf1 Rg1 Rh1 Da2 Hb2 Cc2 Ed2 Me2 Hf2 Cg2 Dh2\n\
             1b ch7 hg7 df7 me7 ed7 dc7 hb7 ca7 rh8 rg8 rf8 re8 rd8 rc8 rb8 ra8\n\
             2w Ed2n Ed3n Me2n Me3n\n2b takeback\n2w Ed2n Ed3n Ed4n Ed5n\n2b ca7s\n",
            false,
        );
        assert_eq!(error, None);
        let main = tree.main_line();
        assert_eq!(main.len(), 5);
        assert_eq!(tree[main[3]].mv().unwrap().notation(), "Ed2n Ed3n Ed4n Ed5n");
        assert_eq!(tree[main[2]].children().len(), 2);
    }
}
