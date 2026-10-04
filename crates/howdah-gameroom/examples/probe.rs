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
//! probe watch GID [--seat 1|2] [--server 1|2|3] [--polls N] [--maxwait SECS]
//!     log in, reserve a viewer seat over ASIP --seat (default 2), sit and
//!     follow the game over ASIP --server (default 1; 3 sits over 1.0
//!     and follows on the browser client's `client3gs.cgi`) for N long polls
//!     (default 3, each waiting up to --maxwait, default 30), log out
//! ```
//! `--log FILE` also appends the exchanges to FILE.

use std::io::Write;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use howdah_gameroom::{
    Asip, DEFAULT_GAMEROOM, Error, Exchange, GameServer, GameState, Http, Lobby, Role, user_agent,
};

fn usage() -> ! {
    eprintln!(
        "usage: probe live | probe watch GID [--seat 1|2] [--server 1|2] [--polls N] [--maxwait SECS] [--log FILE]"
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
    let mut keys: Vec<&String> = r.fields.keys().collect();
    keys.sort();
    println!("  fields: {}", keys.iter().map(|k| k.as_str()).collect::<Vec<_>>().join(" "));
}

#[tokio::main]
async fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let mut positional = Vec::new();
    let (mut seat_asip, mut server_asip, mut polls, mut maxwait, mut log_file) =
        (Asip::V2, Asip::V1, 3, 30, None);
    let mut gs3 = false;
    let mut it = args.iter();
    while let Some(a) = it.next() {
        let mut value = || it.next().cloned().unwrap_or_else(|| usage());
        match a.as_str() {
            "--seat" => seat_asip = asip(&value()),
            "--server" => match value().as_str() {
                "3" => (server_asip, gs3) = (Asip::V1, true),
                v => server_asip = asip(v),
            },
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
        Some("watch") => {
            let gid = positional.get(1).cloned().unwrap_or_else(|| usage());
            let opts = (seat_asip, server_asip, gs3, polls, Duration::from_secs(maxwait));
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
    println!("logged in (ASIP 2.0)");
    let games = lobby.live_games().await?;
    println!("{} live games", games.len());
    for g in games {
        println!("  {} {:?} tc={:?} rated={} postal={}", g.gid, g.players, g.time_control, g.rated, g.postal);
    }
    Ok(())
}

async fn watch(
    lobby: &mut Lobby,
    http: Http,
    user: &str,
    password: &str,
    gid: &str,
    (seat_asip, server_asip, gs3, polls, maxwait): (Asip, Asip, bool, u32, Duration),
) -> Result<(), Error> {
    lobby.login(user, password).await?;
    println!("logged in (ASIP 2.0)");
    lobby.set_asip(seat_asip);
    let seat = lobby.reserve_seat(gid, Role::Viewer).await?;
    println!("viewer seat over {seat_asip:?}: reply as {:?}, gsurl {}", seat.reply_format, seat.gsurl);
    lobby.set_asip(Asip::V2);
    let mut server = GameServer::sit(http, &seat, server_asip).await?;
    println!("sat at {}", server.url());
    if gs3 {
        server.switch_to("http://arimaa.com/arimaa/gameserver/client3gs.cgi", howdah_gameroom::Format::Json);
        println!("following on {}", server.url());
    }
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
