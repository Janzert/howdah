//! Client for the arimaa.com gameroom over ASIP, the Arimaa Server
//! Interface Protocol.
//!
//! The gameroom has a lobby (log in, list games, reserve a seat) and game
//! servers (sit at the reserved seat, read the game state, long-poll for
//! changes, act). ASIP 1.0 speaks `key=value` and ASIP 2.0 JSON; both
//! decode to a [`Record`]. No UI dependencies, so a CLI or tournament
//! runner can use it too.
//!
//! For a game played on the server, the server is the authority: its move
//! list, clocks and result are what the game is. [`GameState`] reports them
//! as the server gives them.
//!
//! - [`Lobby`]: login, the live-games list, seat reservation, and
//!   [`Lobby::watch`]: a viewer seat the browser client's way
//!   (`opengamewin.cgi`), since ASIP viewer seats get moves in ~10 s steps.
//! - [`GameServer`]: sit, `gamestate`, and the `updategamestate` long poll
//!   (keeping the moves and chat received so far).
//! - Requests carry a Referer (the server refuses requests under the
//!   gameroom without one), are spaced at least a second apart (except the
//!   long poll), and can be logged with passwords, `sid`, `auth` and `tid`
//!   redacted ([`NetLog`]).
//!
//! `examples/probe.rs` runs these against the live server by hand.

pub mod client;
pub mod state;
pub mod wire;

pub use client::{
    Asip, DEFAULT_GAMEROOM, Error, Exchange, GameInfo, GameServer, Http, Lobby, NetLog, Seat, ViewerSeat,
};
pub use state::{GameState, Role, ServerClock, parse_result, split_moves};
pub use wire::{Format, Record};

/// The User-Agent for requests: says what the client is.
pub fn user_agent() -> String {
    format!("Howdah/{} (Arimaa client)", env!("CARGO_PKG_VERSION"))
}
