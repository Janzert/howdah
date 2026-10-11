//! Engine tests and tournaments: games between AEI engines, played in
//! colour-swapped pairs, with their results kept on disk and SPRT to decide
//! when a test is done. The plan is in `docs/TOURNAMENTS.md`.
//!
//! - [`TestSpec`]: what a test plays (two engines, the time control, the
//!   stopping rule), read from `test.toml`.
//! - [`TestDir`]: a test's directory: its spec, snapshots of its engines,
//!   the results log and the game records.
//! - [`summarize`]: score, Elo, LOS and the SPRT from the results.
//! - [`run`]: plays a test's remaining games, resuming from its results.
//!
//! No UI dependencies; the command line (`howdah-tourney`), and later the
//! daemon and the app, share this crate.

mod error;
mod results;
mod runner;
mod spec;
mod stats;
mod testdir;

pub use error::TourneyError;
pub use results::{GameEntry, ResultsLog, moves_hash, read_results};
pub use runner::{RunEnd, RunEvent, RunOptions, StopHandle, run};
pub use spec::{EngineSpec, OptionValue, SprtSpec, TestSpec};
pub use stats::{Decision, Summary, elo_from_score, score_from_elo, summarize};
pub use testdir::TestDir;
