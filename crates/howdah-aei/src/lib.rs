//! Async controller for Arimaa Engine Interface (AEI) engines.
//!
//! - [`Engine`]: launch an engine, complete the handshake, send commands and
//!   read parsed [`EngineMessage`]s with timeouts.
//! - [`play_match`]: play a full engine-vs-engine game with clocks.
//! - [`Profile`]: what's known about particular engines (analysis options,
//!   score scales).
//!
//! The protocol is specified in AEI's `AEI_PROTOCOL.md`. Moves coming back
//! from engines are validated with `howdah-arimaa` before being used. No UI or
//! Tauri dependencies; the desktop app, a CLI and a tournament runner share
//! this crate.

mod engine;
mod error;
mod matchplay;
mod message;
mod profile;

pub use engine::{Direction, Engine, EngineConfig, EngineId};
pub use error::AeiError;
pub use matchplay::{MatchConfig, MatchEvent, MatchOutcome, play_match};
pub use message::{EngineMessage, Info, SearchEval, SearchLog, SearchLogKind, split_pv};
pub use profile::{Profile, Score};
