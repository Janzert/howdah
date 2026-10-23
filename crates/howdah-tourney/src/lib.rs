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
//! - [`SpsaSpec`]: a tuning run instead of a test (`[spsa]` in the spec):
//!   one engine against itself with its parameters nudged apart, stepped
//!   by each iteration's games.
//!
//! - [`Daemon`]: a queue of tests sharing game slots, kept in a data
//!   directory; [`server`] serves it over HTTP and [`client`] talks to it.
//!
//! No UI dependencies; the command line (`howdah-tourney`), and later the
//! app, share this crate. The `server` and `client` features (both on by
//! default) add the HTTP side.

mod daemon;
mod error;
mod memory;
mod results;
mod runner;
mod spec;
mod spsa;
mod stats;
mod testdir;

#[cfg(feature = "client")]
pub mod client;
#[cfg(feature = "server")]
pub mod server;

pub use daemon::{
    ARCHIVE_DIR, ArchivedTest, DATA_DIR_ENV, Daemon, DaemonEvent, DaemonInfo, DaemonStatus, GameOutput,
    INFO_FILE, RunningGame, TestInfo, TestStatus, default_data_dir,
};
pub use error::TourneyError;
pub use memory::default_budget as default_memory_budget;
pub use results::{GameEntry, RESOURCE_LIMIT, Restart, ResultsLog, moves_hash, read_results};
pub use runner::{EngineLine, LiveGame, RunEnd, RunEvent, RunOptions, StopHandle, run};
pub use spec::{EngineSpec, MemorySpec, OptionValue, SprtSpec, TestSpec};
pub use spsa::{
    HISTORY_FILE as SPSA_HISTORY_FILE, POINTS_FILE as SPSA_POINTS_FILE, SpsaInfo, SpsaParam, SpsaPoint,
    SpsaSpec, SpsaStep, SpsaValue, draw_point as spsa_point, read_history as spsa_history,
    read_progress as spsa_progress, step as spsa_step,
};
pub use stats::{
    Decision, NO_LIMIT_HORIZON, Projection, Summary, decide_chance, elo_from_score, project, score_from_elo,
    summarize,
};
pub use testdir::TestDir;
