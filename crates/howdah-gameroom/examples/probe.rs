//! Probes the live arimaa.com gameroom by hand, logging every exchange
//! with secrets redacted. Read the "Rules for probing" in the private
//! notes before running it, and use a test account (a human one: bots
//! can't log in over ASIP).
//!
//! Credentials come from `ARIMAA_USERNAME` and `ARIMAA_PASSWORD`.
//!
//! ```text
//! probe live
//!     log in (ASIP 2.0), list the live games, log out
//! probe watch GID [--seat browser|1|2] [--server 1|2] [--polls N] [--maxwait SECS]
//!     log in the browser's way and watch as the browser client does
//!     (default), or log in over ASIP and use an ASIP viewer seat over
//!     --seat 1 or 2, sat and followed over ASIP --server (default 1); N
//!     long polls (default 3, each waiting up to --maxwait, default 30),
//!     log out.
//!     Each update shows how long the server held the latest move.
//! probe findgameid TID
//!     log in the browser's way, look up the permanent id of finished game
//!     TID (ASIP 1.0 `findgameid`), log out
//! ```
//! `--log FILE` also appends the exchanges to FILE.

use std::io::Write;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use howdah_arimaa::Color;
use howdah_gameroom::{
    Asip, DEFAULT_GAMEROOM, Error, Exchange, GameServer, GameState, Http, Lobby, Role, user_agent,
};

fn usage() -> ! {
    eprintln!(
        "usage: probe live | probe watch GID [--seat browser|1|2] [--server 1|2] [--polls N] [--maxwait SECS] | probe findgameid TID [--log FILE]"
    );
    std::process::exit(2)
}

fn asip(v: &str) -> Asip {
    match v {
        "1" => Asip::V1,
        "2" => Asip::V2,
        _ => usage(),
    }
}

fn show(state: &GameState) {
    let r = &state.raw;
    println!(
        "  role={:?} players={:?} tc={:?} started={} turn={:?} moves={} result={:?} reply={:?}",
        state.role,
        state.players,
        state.time_control,
        state.started,
        state.turn,
        state.moves.len(),
        state.result_code,
        r.format,
    );
    println!("  last move: {:?}", state.moves.last());
    println!("  clock: {:?}", state.clock);
    let start = if state.turn == Some(Color::Gold) { "wstartmove" } else { "bstartmove" };
    if let (Some(now), Some(start)) = (r.int("timeonserver"), r.int(start)) {
        println!("  the turn started {}s before this reply (server clock)", now - start);
    }
    let mut keys: Vec<&String> = r.fields.keys().collect();
    keys.sort();
    println!("  fields: {}", keys.iter().map(|k| k.as_str()).collect::<Vec<_>>().join(" "));
}

#[tokio::main]
async fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let mut positional = Vec::new();
    let (mut seat_asip, mut server_asip, mut polls, mut maxwait, mut log_file) =
        (None, Asip::V1, 3, 30, None);
    let mut it = args.iter();
    while let Some(a) = it.next() {
        let mut value = || it.next().cloned().unwrap_or_else(|| usage());
        match a.as_str() {
            "--seat" => {
                seat_asip = match value().as_str() {
                    "browser" => None,
                    v => Some(asip(v)),
                }
            }
            "--server" => server_asip = asip(&value()),
            "--polls" => polls = value().parse().unwrap_or_else(|_| usage()),
            "--maxwait" => maxwait = value().parse().unwrap_or_else(|_| usage()),
            "--log" => log_file = Some(value()),
            _ => positional.push(a.clone()),
        }
    }
    let (Ok(user), Ok(password)) = (std::env::var("ARIMAA_USERNAME"), std::env::var("ARIMAA_PASSWORD"))
    else {
        eprintln!("set ARIMAA_USERNAME and ARIMAA_PASSWORD (tools/accounts.py env USER)");
        std::process::exit(2)
    };

    let file = log_file.map(|p| {
        Mutex::new(std::fs::OpenOptions::new().create(true).append(true).open(p).expect("log file"))
    });
    let file = Arc::new(file);
    let log = Arc::new(move |x: &Exchange| {
        let text = format!(
            "--> {} {:?}\n<-- {:?} in {:.2}s\n{}\n",
            x.url,
            x.request,
            x.status,
            x.elapsed.as_secs_f64(),
            x.reply.trim_end()
        );
        eprintln!("{text}");
        if let Some(f) = file.as_ref() {
            let _ = writeln!(f.lock().unwrap(), "{text}");
        }
    });
    let http = Http::new(&user_agent(), Some(log)).expect("http client");
    let mut lobby = Lobby::new(http.clone(), DEFAULT_GAMEROOM, Asip::V2);

    let result = match positional.first().map(String::as_str) {
        Some("live") => live(&mut lobby, &user, &password).await,
        Some("findgameid") => {
            let tid = positional.get(1).cloned().unwrap_or_else(|| usage());
            find_game_id(&mut lobby, &user, &password, &tid).await
        }
        Some("watch") => {
            let gid = positional.get(1).cloned().unwrap_or_else(|| usage());
            let opts = (seat_asip, server_asip, polls, Duration::from_secs(maxwait));
            watch(&mut lobby, http, &user, &password, &gid, opts).await
        }
        _ => usage(),
    };
    if lobby.is_logged_in() {
        if let Err(e) = lobby.logout().await {
            println!("logout: {e}");
        }
    }
    if let Err(e) = result {
        println!("failed: {e}");
        std::process::exit(1);
    }
}

async fn live(lobby: &mut Lobby, user: &str, password: &str) -> Result<(), Error> {
    lobby.login(user, password).await?;
    println!("logged in");
    let games = lobby.live_games().await?;
    println!("{} live games", games.len());
    for g in games {
        println!("  {} {:?} tc={:?} rated={} postal={}", g.gid, g.players, g.time_control, g.rated, g.postal);
    }
    Ok(())
}

async fn find_game_id(lobby: &mut Lobby, user: &str, password: &str, tid: &str) -> Result<(), Error> {
    lobby.login(user, password).await?;
    println!("logged in");
    println!("permanent id of {tid}: {}", lobby.find_game_id(tid).await?);
    Ok(())
}

async fn watch(
    lobby: &mut Lobby,
    http: Http,
    user: &str,
    password: &str,
    gid: &str,
    (seat_asip, server_asip, polls, maxwait): (Option<Asip>, Asip, u32, Duration),
) -> Result<(), Error> {
    match seat_asip {
        None => lobby.login(user, password).await?,
        Some(_) => lobby.login_asip(user, password).await?,
    }
    println!("logged in");
    let mut server = match seat_asip {
        None => {
            let (server, how) = lobby.watch(gid, Color::Gold).await?;
            println!("viewer seat: {how:?}");
            server
        }
        Some(v) => {
            lobby.set_asip(v);
            let seat = lobby.reserve_seat(gid, Role::Viewer).await?;
            lobby.set_asip(Asip::V2);
            println!("viewer seat over {v:?}: reply as {:?}, gsurl {}", seat.reply_format, seat.gsurl);
            GameServer::sit(http, &seat, server_asip).await?
        }
    };
    println!("following on {}", server.url());
    let state = server.game_state().await?;
    println!("gamestate:");
    show(&state);
    for i in 1..=polls {
        if state.result.is_some() {
            break;
        }
        match server.update(maxwait).await {
            Ok(s) => {
                println!("update {i}:");
                show(&s);
                if s.result.is_some() {
                    break;
                }
            }
            Err(Error::Empty) => {
                println!("update {i}: empty reply; backing off");
                tokio::time::sleep(Duration::from_secs(5)).await;
            }
            Err(e) => return Err(e),
        }
    }
    Ok(())
}
