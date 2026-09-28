//! A minimal AEI engine for tests: plays random legal moves, and can be told
//! to misbehave. Not a useful opponent.
//!
//! Usage: `aei-test-engine [--mode MODE] [--after N] [--delay-ms MS] [--seed S]`
//!
//! Modes (applied once, on the engine's (N+1)-th move; N defaults to 0):
//! - `random` (default): random legal moves.
//! - `illegal`: sends a move that can't be played.
//! - `crash`: exits instead of moving.
//! - `hang`: never answers `go` (ignores `stop` too).
//! - `resign`: resigns.
//! - `garbage`: sends an unknown message before a normal move.
//! - `slow`: waits `--delay-ms` before moving, or until `stop`.

use std::io::{BufRead, Write};
use std::sync::mpsc;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use arimaa_core::notation::{MoveBody, format_placements, format_steps, parse_move_body};
use arimaa_core::{Color, Position, TurnBuilder, apply_setup, default_setup};

struct Rng(u64);

impl Rng {
    fn next(&mut self) -> u64 {
        // xorshift64
        self.0 ^= self.0 << 13;
        self.0 ^= self.0 >> 7;
        self.0 ^= self.0 << 17;
        self.0
    }

    fn below(&mut self, n: usize) -> usize {
        (self.next() % n as u64) as usize
    }
}

struct Options {
    mode: String,
    after: u32,
    delay: Duration,
    seed: u64,
}

fn parse_options() -> Options {
    let mut o = Options {
        mode: "random".into(),
        after: 0,
        delay: Duration::from_millis(0),
        seed: SystemTime::now().duration_since(UNIX_EPOCH).map_or(1, |d| d.as_nanos() as u64) | 1,
    };
    let args: Vec<String> = std::env::args().skip(1).collect();
    let mut i = 0;
    while i + 1 < args.len() {
        let v = &args[i + 1];
        match args[i].as_str() {
            "--mode" => o.mode = v.clone(),
            "--after" => o.after = v.parse().expect("--after N"),
            "--delay-ms" => o.delay = Duration::from_millis(v.parse().expect("--delay-ms MS")),
            "--seed" => o.seed = v.parse::<u64>().expect("--seed S") | 1,
            other => panic!("unknown option {other}"),
        }
        i += 2;
    }
    o
}

struct State {
    pos: Position,
    history: Vec<Position>,
}

impl State {
    fn new() -> State {
        State { pos: Position::empty(Color::Gold), history: Vec::new() }
    }

    fn in_setup(&self) -> bool {
        self.pos.occupied_by(self.pos.side_to_move()) == 0
    }

    fn apply(&mut self, text: &str) -> Result<(), String> {
        let (body, _) = parse_move_body(text).map_err(|e| e.to_string())?;
        self.pos = match body {
            MoveBody::Setup(p) => apply_setup(&self.pos, &p).map_err(|e| e.to_string())?,
            MoveBody::Steps(steps) => {
                let mut tb = TurnBuilder::new(&self.pos);
                for s in steps {
                    tb.try_step(s.step).map_err(|e| e.to_string())?;
                }
                tb.finish().map_err(|e| e.to_string())?.end
            }
            MoveBody::Empty => return Err("empty move".into()),
        };
        self.history.push(self.pos.clone());
        Ok(())
    }

    fn random_move(&self, rng: &mut Rng) -> Option<String> {
        if self.in_setup() {
            return Some(format_placements(&default_setup(self.pos.side_to_move())));
        }
        for _ in 0..200 {
            let mut tb = TurnBuilder::new(&self.pos);
            let wanted = 1 + rng.below(4);
            while tb.steps_left() > 0 {
                let steps = tb.legal_steps();
                if steps.is_empty() {
                    break;
                }
                let (step, _) = steps[rng.below(steps.len())];
                tb.try_step(step).expect("legal step");
                if tb.steps().len() >= wanted && tb.can_finish().is_ok() {
                    break;
                }
            }
            if tb.can_finish().is_err() {
                continue;
            }
            let effects: Vec<_> = tb.steps().iter().map(|s| s.effect).collect();
            let end = tb.finish().ok()?.end;
            if self.history.iter().filter(|p| **p == end).count() >= 2 {
                continue; // would be a third repetition
            }
            return Some(format_steps(&effects));
        }
        None
    }
}

fn main() {
    let opts = parse_options();
    let mut rng = Rng(opts.seed);
    let (tx, rx) = mpsc::channel::<String>();
    std::thread::spawn(move || {
        for line in std::io::stdin().lock().lines() {
            let Ok(line) = line else { break };
            if tx.send(line).is_err() {
                break;
            }
        }
    });

    let mut out = std::io::stdout();
    let mut say = |line: &str| {
        writeln!(out, "{line}").ok();
        out.flush().ok();
    };
    let mut state = State::new();
    let mut moves_made = 0u32;

    while let Ok(line) = rx.recv() {
        let (cmd, rest) = line.trim().split_once(' ').unwrap_or((line.trim(), ""));
        match cmd {
            "aei" => {
                say("protocol-version 1");
                say("id name aei-test-engine");
                say("id author arimaa-desktop");
                say(&format!("id version {}", opts.mode));
                say("aeiok");
            }
            "isready" => say("readyok"),
            "newgame" => state = State::new(),
            "setposition" => {
                let (side, board) = rest.split_once(' ').unwrap_or(("g", ""));
                let side = Color::from_letter(side.chars().next().unwrap_or('g')).unwrap_or(Color::Gold);
                match Position::from_short_string(side, board.trim()) {
                    Ok(p) => {
                        state.pos = p;
                        state.history.clear();
                    }
                    Err(e) => say(&format!("log Error: bad setposition: {e}")),
                }
            }
            "setoption" => {}
            "makemove" => {
                if let Err(e) = state.apply(rest) {
                    say(&format!("log Error: bad makemove {rest:?}: {e}"));
                }
            }
            "go" if rest == "ponder" => {}
            "go" => {
                let misbehave = moves_made == opts.after;
                moves_made += 1;
                say("info depth 1");
                say(&format!("log thinking about move {moves_made}"));
                match opts.mode.as_str() {
                    "crash" if misbehave => std::process::exit(3),
                    "hang" if misbehave => continue,
                    "resign" if misbehave => {
                        say("bestmove resign");
                        continue;
                    }
                    "illegal" if misbehave => {
                        say("bestmove Eh8n");
                        continue;
                    }
                    "garbage" if misbehave => say("this is not an AEI message"),
                    "slow" if misbehave => {
                        let until = Instant::now() + opts.delay;
                        while let Some(left) = until.checked_duration_since(Instant::now()) {
                            match rx.recv_timeout(left) {
                                Ok(l) if l.trim() == "stop" => break,
                                Ok(l) if l.trim() == "quit" => return,
                                _ => {}
                            }
                        }
                    }
                    _ => {}
                }
                match state.random_move(&mut rng) {
                    Some(m) => say(&format!("bestmove {m}")),
                    None => say("bestmove resign"),
                }
            }
            "stop" => {}
            "quit" => return,
            _ => say(&format!("log Error: unknown command {line:?}")),
        }
    }
}
