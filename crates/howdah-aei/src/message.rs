//! Parsing engine-to-controller messages (AEI_PROTOCOL.md, "Engine to
//! Controller Messages").

/// One line from the engine.
#[derive(Clone, Debug, PartialEq)]
pub enum EngineMessage {
    ProtocolVersion(String),
    /// `id <key> <value>`, e.g. `id name OpFor`.
    Id {
        key: String,
        value: String,
    },
    AeiOk,
    ReadyOk,
    /// The move text, or `resign`.
    BestMove(String),
    Info(Info),
    Log(String),
    /// Anything else. The protocol says the controller should report it.
    Unknown(String),
}

/// `info <type> <value>`.
#[derive(Clone, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum Info {
    /// Centi-rabbits from the mover's point of view.
    Score(i32),
    /// Completed depth in steps; `in_progress` is true for `12+`.
    Depth {
        depth: u32,
        in_progress: bool,
    },
    Nodes(u64),
    /// Seconds.
    Time(f64),
    CurrMoveNumber(u32),
    /// The principal variation, split into turns (see [`split_pv`]).
    Pv(Vec<String>),
    /// `winrate <p>`: the mover's chance of winning, 0 to 1.
    WinRate(f64),
    /// `proven win <n>` or `proven loss <n>`: the game's result is proven,
    /// for or against the mover, ending in the mover's `n`th turn from
    /// this one (1 is this turn) for a win, the opponent's for a loss.
    /// The count is optional.
    Proven {
        win: bool,
        turns: Option<u32>,
    },
    /// Any other info type, or a known one with an unparsable value.
    Other {
        key: String,
        value: String,
    },
}

impl EngineMessage {
    pub fn parse(line: &str) -> EngineMessage {
        let line = line.trim_end_matches(['\r', '\n']);
        let (kind, rest) = line.split_once(' ').unwrap_or((line, ""));
        let rest = rest.trim();
        match kind {
            "protocol-version" => EngineMessage::ProtocolVersion(rest.to_string()),
            "id" => {
                let (key, value) = rest.split_once(' ').unwrap_or((rest, ""));
                EngineMessage::Id { key: key.to_string(), value: value.trim().to_string() }
            }
            "aeiok" => EngineMessage::AeiOk,
            "readyok" => EngineMessage::ReadyOk,
            "bestmove" => EngineMessage::BestMove(rest.to_string()),
            "info" => EngineMessage::Info(Info::parse(rest)),
            "log" => EngineMessage::Log(rest.to_string()),
            _ => EngineMessage::Unknown(line.to_string()),
        }
    }
}

impl Info {
    /// Parses the part after `info `.
    pub fn parse(text: &str) -> Info {
        let (key, value) = text.split_once(' ').unwrap_or((text, ""));
        let value = value.trim();
        let other = || Info::Other { key: key.to_string(), value: value.to_string() };
        match key {
            "score" => value.parse().map(Info::Score).unwrap_or_else(|_| other()),
            "depth" => {
                let (digits, in_progress) = value.strip_suffix('+').map_or((value, false), |d| (d, true));
                digits.parse().map(|depth| Info::Depth { depth, in_progress }).unwrap_or_else(|_| other())
            }
            "nodes" => value.parse().map(Info::Nodes).unwrap_or_else(|_| other()),
            "time" => value.parse().map(Info::Time).unwrap_or_else(|_| other()),
            "currmovenumber" => value.parse().map(Info::CurrMoveNumber).unwrap_or_else(|_| other()),
            "pv" => Info::Pv(split_pv(value)),
            "winrate" => match value.parse::<f64>() {
                Ok(p) if (0.0..=1.0).contains(&p) => Info::WinRate(p),
                _ => other(),
            },
            "proven" => {
                let mut words = value.split_whitespace();
                let win = match words.next() {
                    Some("win") => true,
                    Some("loss") => false,
                    _ => return other(),
                };
                match (words.next().map(str::parse::<u32>), words.next()) {
                    (None, None) => Info::Proven { win, turns: None },
                    (Some(Ok(n)), None) if n > 0 => Info::Proven { win, turns: Some(n) },
                    _ => other(),
                }
            }
            _ => other(),
        }
    }
}

/// Search progress sent as `log` lines rather than `info` lines, as
/// bot_Sharp does. It always ends a search with a summary:
///
/// `log Depth 12.0233+ Eval 71 Time 2.80995 Seed 47d0b298fc1972e4`
///
/// and with its `verbose` option (which our fork of Sharp allows outside
/// dev builds, github.com/Janzert/arimaasharp) it also reports each finished
/// iteration (`ID`) and each new best move within one (`FS`):
///
/// `log ID Depth:  12  Eval:  90  Time: 2.37/4.89  PV: Ee2n Ee3n Ee4n Ee5n  ed7s hh7s qpss`
#[derive(Clone, Debug, PartialEq)]
pub struct SearchLog {
    pub kind: SearchLogKind,
    /// Depth in steps as written: fractional, and for the summary with `+`
    /// when a deeper iteration was started but not finished. For `FS`
    /// lines, the progress through the iteration being searched.
    pub depth: String,
    pub eval: SearchEval,
    /// Seconds used so far.
    pub time: Option<f64>,
    /// The principal variation, split into turns. Empty for the summary.
    pub pv: Vec<String>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SearchLogKind {
    /// The summary at the end of a search.
    Summary,
    /// A finished iteration (`ID`).
    Iteration,
    /// A new best move during an iteration (`FS`).
    NewBest,
}

/// An evaluation from the mover's point of view.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum SearchEval {
    /// On the engine's own scale. Sharp's is about 1000 per rabbit
    /// (measured: a rabbit up in the opening is about +1000).
    Score(i32),
    /// A proven win or loss, as the engine writes it (`Win2`, `Loss5`).
    /// The number is the winner's turns to it, its next one as 1, as in
    /// `info proven` (Sharp's `writeEval`; checked on known positions).
    Decided(String),
}

/// Sharp's evals at or beyond this size are proven wins and losses.
const SHARP_DECIDED: i32 = 990_000;

impl SearchEval {
    /// A proven result's turns to it, the winner's next one as 1.
    pub fn turns(&self) -> Option<u32> {
        match self {
            SearchEval::Decided(text) => text.trim_start_matches(char::is_alphabetic).parse().ok(),
            SearchEval::Score(_) => None,
        }
    }

    fn parse(text: &str) -> Option<SearchEval> {
        if text.starts_with("Win") || text.starts_with("Loss") {
            return Some(SearchEval::Decided(text.to_string()));
        }
        let v: i32 = text.parse().ok()?;
        Some(if v.abs() >= SHARP_DECIDED {
            // The summary writes the raw value, the steps to the end;
            // spell it in turns as the other lines do.
            let turns = (1_000_000 - v.abs() + if v > 0 { 11 } else { 7 }) / 8;
            SearchEval::Decided(format!("{}{turns}", if v > 0 { "Win" } else { "Loss" }))
        } else {
            SearchEval::Score(v)
        })
    }
}

impl SearchLog {
    /// Reads the text after `log `, if it's one of Sharp's search lines.
    /// Fields other than depth, eval, time and the PV are ignored.
    pub fn parse(text: &str) -> Option<SearchLog> {
        let (kind, rest) = match text.split_once(' ') {
            Some(("ID", rest)) => (SearchLogKind::Iteration, rest),
            Some(("FS", rest)) => (SearchLogKind::NewBest, rest),
            _ => (SearchLogKind::Summary, text),
        };
        let (fields, pv) = match rest.split_once("PV:") {
            Some((fields, pv)) if kind != SearchLogKind::Summary => (fields, pv),
            _ => (rest, ""),
        };
        let tokens: Vec<&str> = fields.split_whitespace().collect();
        if tokens.first().map(|t| t.trim_end_matches(':')) != Some("Depth") {
            return None;
        }
        let field = |name: &str| {
            tokens.chunks(2).find(|kv| kv[0].trim_end_matches(':') == name).and_then(|kv| kv.get(1).copied())
        };
        let depth = field("Depth")?;
        depth.trim_matches('+').parse::<f64>().ok()?;
        let time = field("Time").and_then(|t| t.split('/').next()?.parse().ok());
        Some(SearchLog {
            kind,
            depth: depth.to_string(),
            eval: SearchEval::parse(field("Eval")?)?,
            time,
            pv: split_sharp_pv(pv),
        })
    }
}

/// Splits a PV as Sharp writes it: turns separated by two spaces, with
/// `qpss` where a turn ends before its fourth step.
fn split_sharp_pv(text: &str) -> Vec<String> {
    text.split("  ")
        .map(|turn| turn.split_whitespace().filter(|&s| s != "qpss").collect::<Vec<_>>().join(" "))
        .filter(|turn| !turn.is_empty())
        .collect()
}

/// Splits a principal variation into turns. After the first turn, each turn
/// starts with the side to move (`w`/`b`, or `g`/`s`):
/// `Ed2n Ed3n b ee7s ee6s w Ed4n` -> `["Ed2n Ed3n", "ee7s ee6s", "Ed4n"]`.
pub fn split_pv(text: &str) -> Vec<String> {
    let mut turns: Vec<Vec<&str>> = vec![Vec::new()];
    for tok in text.split_whitespace() {
        if matches!(tok, "w" | "b" | "g" | "s") {
            if !turns.last().unwrap().is_empty() {
                turns.push(Vec::new());
            }
        } else {
            turns.last_mut().unwrap().push(tok);
        }
    }
    turns.into_iter().filter(|t| !t.is_empty()).map(|t| t.join(" ")).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_messages() {
        assert_eq!(EngineMessage::parse("protocol-version 1"), EngineMessage::ProtocolVersion("1".into()));
        assert_eq!(
            EngineMessage::parse("id name bot OpFor\r\n"),
            EngineMessage::Id { key: "name".into(), value: "bot OpFor".into() }
        );
        assert_eq!(EngineMessage::parse("aeiok"), EngineMessage::AeiOk);
        assert_eq!(EngineMessage::parse("readyok\r"), EngineMessage::ReadyOk);
        assert_eq!(EngineMessage::parse("bestmove Ee2n Ee3n"), EngineMessage::BestMove("Ee2n Ee3n".into()));
        assert_eq!(EngineMessage::parse("log hello there"), EngineMessage::Log("hello there".into()));
        assert_eq!(EngineMessage::parse("frobnicate"), EngineMessage::Unknown("frobnicate".into()));
    }

    #[test]
    fn parses_info() {
        assert_eq!(Info::parse("score -35"), Info::Score(-35));
        assert_eq!(Info::parse("depth 10"), Info::Depth { depth: 10, in_progress: false });
        assert_eq!(Info::parse("depth 10+"), Info::Depth { depth: 10, in_progress: true });
        assert_eq!(Info::parse("nodes 12345678"), Info::Nodes(12_345_678));
        assert_eq!(Info::parse("time 5.3"), Info::Time(5.3));
        assert_eq!(Info::parse("currmovenumber 7"), Info::CurrMoveNumber(7));
        assert_eq!(Info::parse("score big"), Info::Other { key: "score".into(), value: "big".into() });
        assert_eq!(Info::parse("winrate 0.637"), Info::WinRate(0.637));
        assert_eq!(Info::parse("winrate 1"), Info::WinRate(1.0));
        assert_eq!(Info::parse("winrate 63.7"), Info::Other { key: "winrate".into(), value: "63.7".into() });
        assert_eq!(Info::parse("winrate NaN"), Info::Other { key: "winrate".into(), value: "NaN".into() });
        assert_eq!(Info::parse("proven win 2"), Info::Proven { win: true, turns: Some(2) });
        assert_eq!(Info::parse("proven loss"), Info::Proven { win: false, turns: None });
        assert_eq!(Info::parse("proven win 0"), Info::Other { key: "proven".into(), value: "win 0".into() });
        assert_eq!(Info::parse("proven draw"), Info::Other { key: "proven".into(), value: "draw".into() });
        assert_eq!(Info::parse("hashfull 50"), Info::Other { key: "hashfull".into(), value: "50".into() });
    }

    #[test]
    fn parses_sharps_search_log() {
        let summary = SearchLog::parse("Depth 12.0233+ Eval -71 Time 2.80995 Seed 47d0b298fc1972e4").unwrap();
        assert_eq!(
            summary,
            SearchLog {
                kind: SearchLogKind::Summary,
                depth: "12.0233+".into(),
                eval: SearchEval::Score(-71),
                time: Some(2.80995),
                pv: vec![],
            }
        );
        // Sharp's summaries and verbose lines for the same positions: a
        // win in 12 steps is the winner's 2nd turn, a loss in 16 steps
        // the opponent's 2nd, a loss in 8 its 1st.
        let won = SearchLog::parse("Depth 4 Eval 999988 Time 0.013948 Seed 4303733890ec300f").unwrap();
        assert_eq!(won.eval, SearchEval::Decided("Win2".into()));
        assert_eq!(won.eval.turns(), Some(2));
        let lost = SearchLog::parse("Depth 4 Eval -999984 Time 0.011724 Seed 31af722318e2503c").unwrap();
        assert_eq!(lost.eval, SearchEval::Decided("Loss2".into()));
        let lost = SearchLog::parse("Depth 4 Eval -999992 Time 0.0086 Seed 2ac1616d6b88e6b7").unwrap();
        assert_eq!(lost.eval.turns(), Some(1));
        assert_eq!(SearchEval::Score(90).turns(), None);

        let id = SearchLog::parse(
            "ID Depth:  12     Eval:     90 Time: 2.37/4.89  PV: Ee2n Ee3n Ee4n Ee5n  ed7s hh7s hh6w me7w  Dg2n qpss",
        )
        .unwrap();
        assert_eq!((id.kind, id.depth.as_str(), id.time), (SearchLogKind::Iteration, "12", Some(2.37)));
        assert_eq!(id.pv, ["Ee2n Ee3n Ee4n Ee5n", "ed7s hh7s hh6w me7w", "Dg2n"]);
        let fs =
            SearchLog::parse("FS Depth: +3.001  Eval:   Loss5 Time: 0.02/7.97  PV: dg7s qpss  qpss").unwrap();
        assert_eq!((fs.kind, fs.depth.as_str()), (SearchLogKind::NewBest, "+3.001"));
        assert_eq!((fs.eval, fs.pv), (SearchEval::Decided("Loss5".into()), vec!["dg7s".to_string()]));

        assert_eq!(SearchLog::parse("Started new game"), None);
        assert_eq!(SearchLog::parse("Depth 12 Time 3"), None, "no eval");
        assert_eq!(SearchLog::parse("Depth deep Eval 3"), None);
        assert_eq!(SearchLog::parse("ID something else"), None);
        assert_eq!(SearchLog::parse("PerMove 3 (max 2.592e+06) Reserve current 10"), None);
    }

    #[test]
    fn splits_pv_into_turns() {
        assert_eq!(
            split_pv("Ee2n Ee3n Ee4n Ee5n b ed7s dd8s rh7s rh6w w Ee6s"),
            vec!["Ee2n Ee3n Ee4n Ee5n", "ed7s dd8s rh7s rh6w", "Ee6s"]
        );
        assert_eq!(split_pv("Ed2n s ee7s g Ed3n"), vec!["Ed2n", "ee7s", "Ed3n"]);
        assert_eq!(split_pv(""), Vec::<String>::new());
    }
}
