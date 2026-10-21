//! The results log, `results.jsonl`: one line per finished game, appended
//! as it ends. With the spec it is the whole state of a test.

use std::fs::{File, OpenOptions};
use std::io::Write;
use std::path::{Path, PathBuf};

use howdah_arimaa::{Color, GameResult};
use serde::{Deserialize, Serialize};

use crate::error::TourneyError;

/// One finished game. Pairs count from 1; in game 1 of a pair engine 1
/// (index 0) is gold, in game 2 engine 2 is.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GameEntry {
    pub pair: u32,
    pub game: u8,
    /// The winning engine's index (0 or 1).
    pub winner: usize,
    /// How it ended: goal, elimination, immobilization, timeout,
    /// resignation, illegalMove, score, forfeit, or [`RESOURCE_LIMIT`] (a
    /// forfeit for going over a memory limit).
    pub reason: String,
    pub plies: u32,
    /// Why it ended, when that wasn't on the board.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub detail: Option<String>,
    /// Seconds each engine (by index) spent on its moves.
    pub used: [f64; 2],
    /// The game's record, relative to the test's directory.
    pub record: String,
    /// [`moves_hash`] of the game's moves, to find repeated games.
    pub moves_hash: String,
    /// When it ended, in seconds since the Unix epoch.
    pub finished: u64,
    /// Each engine's resident memory in MB (by index) as the game ended,
    /// if it could be read.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub memory_mb: Option<[u64; 2]>,
    /// Engines restarted after the game, and why.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub restarts: Vec<Restart>,
}

/// [`GameEntry::reason`] for a game an engine forfeited by going over a
/// memory limit.
pub const RESOURCE_LIMIT: &str = "resourceLimit";

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Restart {
    /// The engine's index.
    pub engine: usize,
    pub reason: String,
}

impl GameEntry {
    /// The index of the engine playing gold in game `game` of a pair.
    pub fn gold_of(game: u8) -> usize {
        usize::from(game == 2)
    }

    pub fn gold(&self) -> usize {
        GameEntry::gold_of(self.game)
    }

    /// How the game ended, by colour (None for a reason this version
    /// doesn't know).
    pub fn result(&self) -> Option<GameResult> {
        let winner = if self.winner == self.gold() { Color::Gold } else { Color::Silver };
        Some(GameResult { winner, reason: crate::runner::reason_from_name(&self.reason)? })
    }
}

/// A stable hash (64-bit FNV-1a) of a game's moves.
pub fn moves_hash<'a>(moves: impl IntoIterator<Item = &'a str>) -> String {
    let mut h: u64 = 0xcbf2_9ce4_8422_2325;
    for (i, m) in moves.into_iter().enumerate() {
        let sep: &[u8] = if i == 0 { b"" } else { b"\n" };
        for b in sep.iter().chain(m.as_bytes()) {
            h ^= u64::from(*b);
            h = h.wrapping_mul(0x0000_0100_0000_01b3);
        }
    }
    format!("{h:016x}")
}

/// Reads a results log. A missing file is an empty log, and a last line
/// cut short (the runner killed mid-write) is left out; any other bad line
/// is an error. A game logged twice keeps its first entry.
pub fn read_results(path: &Path) -> Result<Vec<GameEntry>, TourneyError> {
    let text = match std::fs::read_to_string(path) {
        Ok(t) => t,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(Vec::new()),
        Err(e) => return Err(TourneyError::io(path)(e)),
    };
    parse_results(&text, path)
}

/// [`read_results`] on a log already read; `path` is for errors.
pub(crate) fn parse_results(text: &str, path: &Path) -> Result<Vec<GameEntry>, TourneyError> {
    let complete = text.ends_with('\n');
    let lines: Vec<&str> = text.lines().collect();
    let mut entries: Vec<GameEntry> = Vec::new();
    for (i, line) in lines.iter().enumerate() {
        if line.trim().is_empty() {
            continue;
        }
        match serde_json::from_str::<GameEntry>(line) {
            Ok(e) => {
                if !entries.iter().any(|x| x.pair == e.pair && x.game == e.game) {
                    entries.push(e);
                }
            }
            Err(_) if i + 1 == lines.len() && !complete => {}
            Err(e) => {
                return Err(TourneyError::Parse {
                    path: path.into(),
                    message: format!("line {}: {e}", i + 1),
                });
            }
        }
    }
    Ok(entries)
}

/// Appends entries to a results log, each written and synced as a whole
/// line.
pub struct ResultsLog {
    path: PathBuf,
    file: File,
}

impl ResultsLog {
    pub fn open(path: &Path) -> Result<ResultsLog, TourneyError> {
        // A line cut short by a kill would run into the next one; drop it.
        if let Ok(bytes) = std::fs::read(path)
            && !bytes.is_empty()
            && !bytes.ends_with(b"\n")
        {
            let keep = bytes.iter().rposition(|b| *b == b'\n').map_or(0, |i| i + 1);
            let file = OpenOptions::new().write(true).open(path).map_err(TourneyError::io(path))?;
            file.set_len(keep as u64).map_err(TourneyError::io(path))?;
        }
        let file = OpenOptions::new().create(true).append(true).open(path).map_err(TourneyError::io(path))?;
        Ok(ResultsLog { path: path.into(), file })
    }

    pub fn append(&mut self, entry: &GameEntry) -> Result<(), TourneyError> {
        self.append_line(entry)
    }

    /// Appends any value as a line (the SPSA logs work the same way).
    pub(crate) fn append_line(&mut self, value: &impl Serialize) -> Result<(), TourneyError> {
        let mut line = serde_json::to_string(value).expect("a log line always serializes");
        line.push('\n');
        self.file.write_all(line.as_bytes()).map_err(TourneyError::io(&self.path))?;
        self.file.sync_data().map_err(TourneyError::io(&self.path))
    }
}

#[cfg(test)]
pub(crate) fn entry(pair: u32, game: u8, winner: usize) -> GameEntry {
    GameEntry {
        pair,
        game,
        winner,
        reason: "goal".into(),
        plies: 60,
        detail: None,
        used: [10.0, 11.0],
        record: format!("games/{pair:04}-{game}.txt"),
        moves_hash: format!("{pair}-{game}"),
        finished: 0,
        memory_mb: None,
        restarts: Vec::new(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn appends_and_reads_back() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("results.jsonl");
        assert!(read_results(&path).unwrap().is_empty());
        let mut log = ResultsLog::open(&path).unwrap();
        log.append(&entry(1, 1, 0)).unwrap();
        log.append(&entry(1, 2, 1)).unwrap();
        log.append(&entry(1, 2, 0)).unwrap();
        let read = read_results(&path).unwrap();
        assert_eq!(read, vec![entry(1, 1, 0), entry(1, 2, 1)]);
    }

    #[test]
    fn a_line_cut_short_is_dropped() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("results.jsonl");
        let full = serde_json::to_string(&entry(1, 1, 0)).unwrap();
        std::fs::write(&path, format!("{full}\n{}", &full[..20])).unwrap();
        assert_eq!(read_results(&path).unwrap(), vec![entry(1, 1, 0)]);
        let mut log = ResultsLog::open(&path).unwrap();
        log.append(&entry(1, 2, 0)).unwrap();
        assert_eq!(read_results(&path).unwrap(), vec![entry(1, 1, 0), entry(1, 2, 0)]);
    }

    #[test]
    fn moves_hash_is_stable() {
        assert_eq!(moves_hash([]), "cbf29ce484222325");
        assert_ne!(moves_hash(["ab", "c"]), moves_hash(["a", "bc"]));
        assert_eq!(moves_hash(["1g Ra1", "1s ra8"]), moves_hash(["1g Ra1", "1s ra8"]));
    }
}
