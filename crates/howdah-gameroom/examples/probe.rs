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
//! probe play
//!     log in the browser's way, then read commands from stdin, one per
//!     line (for playing self-play games; see below). Each seat is long-polled
//!     in the background and its updates printed as they come.
//! ```
//!
//! `play` commands:
//! ```text
//! new w|b TC [rated]         create a game (newgame over ASIP 2.0), unrated
//!                            unless `rated` is given
//! cancel GID                 cancel an open game (cancelopengame)
//! mygames                    the user's games and the open games
//! seat NAME GID w|b HOW [GRID]
//!                            take a player's seat, HOW being browser (the
//!                            opengamewin.cgi way), asip1 or asip2 (reserveseat
//!                            and sit over that version; the browser login gives
//!                            no grid, so pass the room's, 3)
//! NAME ACTION [ARGS]         act at seat NAME: start (startgame), move TEXT,
//!                            resign, takeback, reply yes|no, chat TEXT, leave,
//!                            state (a gamestate), raw ACTION [k=v ...]
//! sleep SECS
//! quit
//! ```
//! `--log FILE` also appends the exchanges to FILE.

use std::collections::HashMap;
use std::io::{BufRead, Write};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use howdah_arimaa::Color;
use howdah_gameroom::{
    Actions, Asip, DEFAULT_GAMEROOM, Error, Exchange, GameServer, GameState, Http, Lobby, Record, Role,
    user_agent,
};

fn usage() -> ! {
    eprintln!(
        "usage: probe live | probe watch GID [--seat browser|1|2] [--server 1|2] [--polls N] [--maxwait SECS] | probe findgameid TID | probe play [--log FILE]"
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
        Some("play") => play(&mut lobby, http, &user, &password).await,
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

/// A record's fields with secrets redacted, for printing.
fn public(r: &Record) -> String {
    howdah_gameroom::client::redact(&serde_json::Value::Object(r.fields.clone()).to_string())
}

/// What a poll update says, in one line: the time since the probe started,
/// the seat, and the state's main fields.
fn brief(t0: Instant, name: &str, state: &GameState) -> String {
    let r = &state.raw;
    let start = if state.turn == Some(Color::Gold) { "wstartmove" } else { "bstartmove" };
    let held = match (r.int("timeonserver"), r.int(start)) {
        (Some(now), Some(start)) => format!("{}s", now - start),
        _ => "-".into(),
    };
    format!(
        "[{:7.1}] {name}: plies={} turn={:?} last={:?} started={} canstart={:?} takeback={:?} result={:?} \
         present={:?}/{:?} lastchange={:?} turn-started-before-reply={held}",
        t0.elapsed().as_secs_f64(),
        state.moves.len(),
        state.turn,
        state.moves.last(),
        state.started,
        r.str("canstart"),
        r.str("takeback"),
        state.result_code,
        r.str("wpresent"),
        r.str("bpresent"),
        r.str("lastchange"),
    )
}

async fn play(lobby: &mut Lobby, http: Http, user: &str, password: &str) -> Result<(), Error> {
    lobby.login(user, password).await?;
    let t0 = Instant::now();
    println!("logged in");
    let (tx, mut rx) = tokio::sync::mpsc::unbounded_channel::<String>();
    std::thread::spawn(move || {
        for line in std::io::stdin().lock().lines() {
            let Ok(line) = line else { break };
            if tx.send(line).is_err() {
                break;
            }
        }
    });
    let mut seats: HashMap<String, (Actions, tokio::task::JoinHandle<()>)> = HashMap::new();
    while let Some(line) = rx.recv().await {
        let words: Vec<&str> = line.split_whitespace().collect();
        if words.is_empty() || words[0].starts_with('#') {
            continue;
        }
        println!("[{:7.1}] > {line}", t0.elapsed().as_secs_f64());
        let side = |w: &str| if w == "w" { Color::Gold } else { Color::Silver };
        let result: Result<(), Error> = async {
            match words.as_slice() {
                ["quit"] => return Err(Error::Server("quit".into())),
                ["sleep", secs] => {
                    tokio::time::sleep(Duration::from_secs_f64(secs.parse().unwrap_or(1.0))).await
                }
                // Rated only when asked for by name: a rated game leaves a
                // record and changes both players' ratings.
                ["new", s, tc, rest @ ..] if rest.is_empty() || rest == ["rated"] => {
                    let (gid, seat) = lobby.new_game(side(s), tc, rest == ["rated"]).await?;
                    println!("  created: gid={gid:?} gsurl={} (reply {:?})", seat.gsurl, seat.reply_format);
                }
                ["cancel", gid] => {
                    lobby.cancel_open_game(gid).await?;
                    println!("  cancelled");
                }
                ["mygames"] => {
                    let (mine, open) = lobby.my_games().await?;
                    for g in mine {
                        println!("  mine: {}", public(&g));
                    }
                    for g in open {
                        println!("  open: {}", public(&g));
                    }
                }
                ["seat", name, gid, s, how, grid @ ..] => {
                    let mut server = match *how {
                        "browser" => lobby.play(gid, side(s)).await?,
                        "asip1" | "asip2" => {
                            let v = if *how == "asip1" { Asip::V1 } else { Asip::V2 };
                            lobby.set_asip(v);
                            let seat = lobby.reserve_seat(gid, Role::Player(side(s))).await;
                            lobby.set_asip(Asip::V2);
                            let mut seat = seat?;
                            if let Some(g) = grid.first() {
                                seat.grid = g.to_string();
                            }
                            println!("  seat: gsurl {} (reply {:?})", seat.gsurl, seat.reply_format);
                            GameServer::sit(http.clone(), &seat, v).await?
                        }
                        _ => return Err(Error::Server(format!("unknown seat kind {how}"))),
                    };
                    println!("  following on {}", server.url());
                    let state = server.game_state().await?;
                    show(&state);
                    let actions = server
                        .actions()
                        .ok_or_else(|| Error::BadReply("no auth in the game state".into()))?;
                    let name2 = name.to_string();
                    let poller = tokio::spawn(async move {
                        let mut done = state.result.is_some();
                        while !done {
                            match server.update(Duration::from_secs(300)).await {
                                Ok(s) => {
                                    println!("{}", brief(t0, &name2, &s));
                                    done = s.result.is_some();
                                    if done {
                                        println!("  {name2} final: {}", public(&s.raw));
                                    }
                                }
                                Err(Error::Empty) => {
                                    println!("[{:7.1}] {name2}: empty reply", t0.elapsed().as_secs_f64());
                                    tokio::time::sleep(Duration::from_secs(2)).await;
                                }
                                Err(e) => {
                                    println!(
                                        "[{:7.1}] {name2}: poll failed: {e}",
                                        t0.elapsed().as_secs_f64()
                                    );
                                    tokio::time::sleep(Duration::from_secs(5)).await;
                                }
                            }
                        }
                    });
                    if let Some((_, old)) = seats.insert(name.to_string(), (actions, poller)) {
                        old.abort();
                    }
                }
                [name, action, args @ ..] if seats.contains_key(*name) => {
                    let actions = &seats[*name].0;
                    let text = args.join(" ");
                    let (act, extra): (&str, Vec<(&str, String)>) = match *action {
                        "start" => ("startgame", vec![]),
                        "move" => ("move", vec![("move", text)]),
                        "resign" => ("resign", vec![]),
                        "takeback" => ("takeback", vec![("takeback", "req".into())]),
                        "reply" => ("takebackreply", vec![("takebackreply", text)]),
                        "chat" => ("chat", vec![("chat", text)]),
                        "leave" => ("leave", vec![]),
                        "state" => ("gamestate", vec![("wait", "0".into())]),
                        "raw" => {
                            let act = args.first().copied().unwrap_or("");
                            let extra = args[1.min(args.len())..]
                                .iter()
                                .filter_map(|kv| kv.split_once('='))
                                .map(|(k, v)| (k, v.to_string()))
                                .collect();
                            (act, extra)
                        }
                        _ => return Err(Error::Server(format!("unknown action {action}"))),
                    };
                    let r = actions.act(act, &extra).await?;
                    if act == "gamestate" {
                        show(&GameState::from_record(r));
                    } else {
                        println!("  reply: {}", public(&r));
                    }
                }
                _ => println!("  ?"),
            }
            Ok(())
        }
        .await;
        match result {
            Err(Error::Server(q)) if q == "quit" => break,
            Err(e) => println!("[{:7.1}]   failed: {e}", t0.elapsed().as_secs_f64()),
            Ok(()) => {}
        }
    }
    for (_, (_, poller)) in seats {
        poller.abort();
    }
    Ok(())
}
