//! A test's spec, as written in `test.toml`.
//!
//! ```toml
//! name = "opfor-null-move"
//! time_control = "2s/30s/100/0/100t"
//! # setup_minimum = "1m"   # the least time a setup gets (default 0: the move time)
//! concurrency = 4
//! max_pairs = 2000
//!
//! [sprt]
//! elo0 = 0
//! elo1 = 10
//!
//! [[engines]]
//! name = "dev"
//! program = "../OpFor/bot_opfor"
//! options = { hash = 64 }
//!
//! [[engines]]
//! name = "base"
//! program = "/home/me/bots/opfor-base"
//!
//! [memory]        # optional; these are the defaults
//! growth = 3.0    # restart an engine between games past 3x its size after its first game
//! # restart_mb = 2000   # ... or past this many MB
//! # limit_mb = 8000     # forfeit mid-game past this (default: a quarter of the machine's memory)
//! ```
//!
//! Engine 1 is the one being tested: scores, Elo and the SPRT are from its
//! side.
//!
//! An SPSA tuning run (see `spsa`) has `[spsa]` and one engine, which
//! plays both sides: engine 1 at θ + cΔ, engine 2 at θ − cΔ. It has no
//! SPRT or `max_pairs` (it plays `iterations × pairs_per_iteration`
//! pairs):
//!
//! ```toml
//! name = "opfor-eval"
//! time_control = "2s/30s/100/0/100t"
//! concurrency = 4
//!
//! [spsa]
//! iterations = 2000
//! pairs_per_iteration = 8   # these are the defaults:
//! # alpha = 0.602
//! # gamma = 0.101
//! # a_ratio = 0.1           # A = 10% of the iterations
//!
//! [[spsa.params]]
//! name = "eval_frozen"      # the AEI option
//! start = 1.0
//! min = 0.0
//! max = 3.0
//! c_end = 0.1
//! # r_end = 0.002
//! # int = false
//!
//! [[engines]]
//! name = "opfor"
//! program = "../OpFor/bot_opfor"
//! options = { hash = 64 }
//! ```

use std::collections::BTreeMap;
use std::fmt;
use std::path::PathBuf;
use std::time::Duration;

use howdah_arimaa::TimeControl;
use serde::{Deserialize, Serialize};

use crate::error::TourneyError;
use crate::spsa::SpsaSpec;

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TestSpec {
    pub name: String,
    /// In AEI's `M/R/P/L/G/T` format. None plays without clocks.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub time_control: Option<String>,
    /// The least move time a setup gets, in the time control's syntax.
    /// None is no minimum, the plain move time; arimaa.com gives humans a
    /// minute (`1m`).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub setup_minimum: Option<String>,
    /// Games played at once.
    #[serde(default = "one")]
    pub concurrency: u32,
    /// Stop after this many pairs, if the SPRT hasn't decided first.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub max_pairs: Option<u32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub sprt: Option<SprtSpec>,
    /// An SPSA tuning run instead of a test.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub spsa: Option<SpsaSpec>,
    /// Keep each game's engine transcripts in `logs/`.
    #[serde(default, skip_serializing_if = "is_false")]
    pub engine_logs: bool,
    /// Exactly two, or one for SPSA.
    pub engines: Vec<EngineSpec>,
    /// When an engine's memory makes the runner restart it, or forfeit
    /// its game.
    #[serde(default, skip_serializing_if = "MemorySpec::is_default")]
    pub memory: MemorySpec,
}

/// Limits on each engine's resident memory (`[memory]`). Between games
/// the runner restarts an engine that has grown past `growth` times its
/// size after its first game (and past [`MemorySpec::GROWTH_FLOOR_MB`]),
/// or past `restart_mb`. In a game, one past `limit_mb` forfeits it.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct MemorySpec {
    /// 0 turns the growth check off.
    #[serde(default = "default_growth")]
    pub growth: f64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub restart_mb: Option<u64>,
    /// None: a quarter of the machine's memory.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub limit_mb: Option<u64>,
}

impl Default for MemorySpec {
    fn default() -> Self {
        MemorySpec { growth: default_growth(), restart_mb: None, limit_mb: None }
    }
}

impl MemorySpec {
    /// Growth alone never restarts an engine smaller than this, so small
    /// engines aren't restarted for a few MB.
    pub const GROWTH_FLOOR_MB: u64 = 256;

    fn is_default(&self) -> bool {
        *self == MemorySpec::default()
    }

    /// The hard limit in bytes, given the machine's memory.
    pub fn limit_bytes(&self, physical: Option<u64>) -> u64 {
        match self.limit_mb {
            Some(mb) => mb << 20,
            None => physical.map_or(4 << 30, |p| p / 4),
        }
    }
}

fn default_growth() -> f64 {
    3.0
}

#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SprtSpec {
    /// Elo difference of the null hypothesis (no better than this).
    pub elo0: f64,
    /// Elo difference of the alternative (at least this much better).
    pub elo1: f64,
    /// Chance of passing a change that's only `elo0` better.
    #[serde(default = "five_percent")]
    pub alpha: f64,
    /// Chance of failing a change that's `elo1` better.
    #[serde(default = "five_percent")]
    pub beta: f64,
    /// With a pair limit, end the test undecided once the chance of the
    /// SPRT reaching a bound within it falls below this (0.05: at most 5%
    /// of tests ended this way would have gone on to decide); see
    /// [`crate::stats::decide_chance`]. None: play on to the limit.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub undecided_below: Option<f64>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct EngineSpec {
    pub name: String,
    /// The program. In a test's directory, its snapshot, relative to the
    /// directory.
    pub program: PathBuf,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub args: Vec<String>,
    /// Where the engine runs. By default its snapshot's directory.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub working_dir: Option<PathBuf>,
    /// AEI options, sent once when the engine starts.
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub options: BTreeMap<String, OptionValue>,
    /// Start a new process for every game instead of reusing one.
    #[serde(default, skip_serializing_if = "is_false")]
    pub restart_each_game: bool,
    /// Snapshot the program's whole directory, for an engine that needs
    /// files beside it.
    #[serde(default, skip_serializing_if = "is_false")]
    pub copy_dir: bool,
    /// Written when the snapshot is taken: the program it came from and
    /// its SHA-256.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub source: Option<PathBuf>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub sha256: Option<String>,
}

/// An option's value, as TOML writes it (`hash = 64`, `name = "x"`).
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum OptionValue {
    Bool(bool),
    Int(i64),
    Float(f64),
    Text(String),
}

impl fmt::Display for OptionValue {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            // AEI engines take 1 and 0, as pyrimaa sends them.
            OptionValue::Bool(b) => write!(f, "{}", u8::from(*b)),
            OptionValue::Int(i) => write!(f, "{i}"),
            OptionValue::Float(x) => write!(f, "{x}"),
            OptionValue::Text(s) => f.write_str(s),
        }
    }
}

impl TestSpec {
    pub fn parse(text: &str) -> Result<TestSpec, String> {
        let spec: TestSpec = toml::from_str(text).map_err(|e| e.to_string())?;
        spec.validate().map_err(|e| e.to_string())?;
        Ok(spec)
    }

    pub fn to_toml(&self) -> String {
        toml::to_string_pretty(self).expect("a spec always serializes")
    }

    /// The time control, with the setup minimum.
    pub fn time_control(&self) -> Option<TimeControl> {
        let tc: TimeControl = self.time_control.as_deref()?.parse().expect("validated");
        Some(tc.with_setup_minimum(self.setup_minimum()))
    }

    /// The least move time a setup gets: `setup_minimum`, or none.
    pub fn setup_minimum(&self) -> Duration {
        self.setup_minimum
            .as_deref()
            .map_or(Duration::ZERO, |t| TimeControl::parse_time(t).expect("validated"))
    }

    /// The pair limit: `max_pairs`, or every pair of an SPSA run.
    pub fn pair_limit(&self) -> Option<u32> {
        self.max_pairs.or(self.spsa.as_ref().map(SpsaSpec::pairs))
    }

    /// The engine playing as engine `index` (0 or 1): an SPSA run's one
    /// engine plays both.
    pub fn engine(&self, index: usize) -> &EngineSpec {
        &self.engines[index.min(self.engines.len() - 1)]
    }

    /// The two sides' names: the engines', or for SPSA the engine's with
    /// `+` (θ + cΔ) and `-` (θ − cΔ).
    pub fn side_names(&self) -> [String; 2] {
        match &self.spsa {
            Some(_) => ["+", "-"].map(|s| format!("{}{s}", self.engines[0].name)),
            None => [0, 1].map(|i| self.engines[i].name.clone()),
        }
    }

    pub fn validate(&self) -> Result<(), TourneyError> {
        let bad = |m: String| Err(TourneyError::Spec(m));
        if self.engines.iter().any(|e| e.name.trim().is_empty()) {
            return bad("every engine needs a name".into());
        }
        if let Some(spsa) = &self.spsa {
            if self.engines.len() != 1 {
                return bad(format!("an SPSA run needs one engine, not {}", self.engines.len()));
            }
            if self.sprt.is_some() || self.max_pairs.is_some() {
                return bad("an SPSA run has no SPRT or max_pairs; it plays its iterations".into());
            }
            spsa.validate().map_err(TourneyError::Spec)?;
            if let Some(p) = spsa.params.iter().find(|p| self.engines[0].options.contains_key(&p.name)) {
                return bad(format!("{} is both an engine option and an SPSA parameter", p.name));
            }
        } else if self.engines.len() != 2 {
            return bad(format!("a test needs two engines, not {}", self.engines.len()));
        } else if self.engines[0].name == self.engines[1].name {
            return bad(format!("both engines are named {:?}", self.engines[0].name));
        }
        if let Some(tc) = &self.time_control
            && let Err(e) = tc.parse::<TimeControl>()
        {
            return bad(format!("time control {tc:?}: {e}"));
        }
        if let Some(m) = &self.setup_minimum
            && let Err(e) = TimeControl::parse_time(m)
        {
            return bad(format!("setup_minimum {m:?}: {e}"));
        }
        if self.concurrency == 0 {
            return bad("concurrency must be at least 1".into());
        }
        if self.max_pairs == Some(0) {
            return bad("max_pairs must be at least 1".into());
        }
        match self.sprt {
            None if self.pair_limit().is_none() => {
                return bad("give an SPRT, max_pairs, or both, so the test ends".into());
            }
            Some(s) if s.elo1 <= s.elo0 => return bad("the SPRT's elo1 must be above elo0".into()),
            Some(s) if !(0.0 < s.alpha && s.alpha < 0.5 && 0.0 < s.beta && s.beta < 0.5) => {
                return bad("the SPRT's alpha and beta must be between 0 and 0.5".into());
            }
            Some(SprtSpec { undecided_below: Some(u), .. }) if !(0.0 < u && u < 1.0) => {
                return bad("the SPRT's undecided_below must be between 0 and 1".into());
            }
            _ => {}
        }
        let m = &self.memory;
        if !(m.growth == 0.0 || m.growth > 1.0) || !m.growth.is_finite() {
            return bad("memory growth must be 0 (off) or more than 1".into());
        }
        if m.restart_mb == Some(0) || m.limit_mb == Some(0) {
            return bad("memory limits must be at least 1 MB".into());
        }
        if let (Some(r), Some(l)) = (m.restart_mb, m.limit_mb)
            && r >= l
        {
            return bad("memory restart_mb must be below limit_mb".into());
        }
        Ok(())
    }
}

fn one() -> u32 {
    1
}

fn five_percent() -> f64 {
    0.05
}

fn is_false(b: &bool) -> bool {
    !b
}

#[cfg(test)]
mod tests {
    use super::*;

    const SPEC: &str = r#"
        name = "t"
        time_control = "2s/30s/100/0/0/120"
        concurrency = 2
        [sprt]
        elo0 = 0
        elo1 = 10
        [[engines]]
        name = "dev"
        program = "bin/dev"
        options = { hash = 64, verbose = true, book = "x.txt" }
        [[engines]]
        name = "base"
        program = "bin/base"
        args = ["aei"]
    "#;

    #[test]
    fn reads_and_writes_a_spec() {
        let spec = TestSpec::parse(SPEC).unwrap();
        assert_eq!(spec.concurrency, 2);
        assert_eq!(spec.sprt.unwrap().alpha, 0.05);
        let opts = &spec.engines[0].options;
        assert_eq!(opts["hash"].to_string(), "64");
        assert_eq!(opts["verbose"].to_string(), "1");
        assert_eq!(opts["book"].to_string(), "x.txt");
        assert_eq!(TestSpec::parse(&spec.to_toml()).unwrap(), spec);
    }

    #[test]
    fn setups_get_the_move_time_unless_the_spec_says() {
        let spec = TestSpec::parse(SPEC).unwrap();
        assert_eq!(spec.time_control().unwrap().setup_move_time(), Duration::from_secs(2));
        let minute = TestSpec::parse(&SPEC.replace("concurrency = 2", "setup_minimum = \"1m\"")).unwrap();
        assert_eq!(minute.time_control().unwrap().setup_move_time(), Duration::from_secs(60));
        assert_eq!(TestSpec::parse(&minute.to_toml()).unwrap(), minute);
    }

    #[test]
    fn refuses_specs_that_cannot_run() {
        let no_end = SPEC.replace("[sprt]\n        elo0 = 0\n        elo1 = 10", "");
        assert!(TestSpec::parse(&no_end).unwrap_err().contains("so the test ends"));
        let same = SPEC.replace("\"base\"", "\"dev\"");
        assert!(TestSpec::parse(&same).unwrap_err().contains("both engines"));
        let tc = SPEC.replace("2s/30s/100/0/0/120", "soon");
        assert!(TestSpec::parse(&tc).unwrap_err().contains("time control"));
        let minimum = SPEC.replace("concurrency = 2", "setup_minimum = \"soon\"");
        assert!(TestSpec::parse(&minimum).unwrap_err().contains("setup_minimum"));
        let bounds = SPEC.replace("elo1 = 10", "elo1 = -5");
        assert!(TestSpec::parse(&bounds).unwrap_err().contains("elo1"));
        let unknown = SPEC.replace("concurrency = 2", "concurency = 2");
        assert!(TestSpec::parse(&unknown).is_err());
        let growth = format!("{SPEC}\n        [memory]\n        growth = 0.5");
        assert!(TestSpec::parse(&growth).unwrap_err().contains("growth"));
        let limits = format!("{SPEC}\n        [memory]\n        restart_mb = 900\n        limit_mb = 800");
        assert!(TestSpec::parse(&limits).unwrap_err().contains("restart_mb"));
    }

    const SPSA: &str = r#"
        name = "tune"
        time_control = "2s/30s/100/0/0/120"
        [spsa]
        iterations = 100
        [[spsa.params]]
        name = "eval_frozen"
        start = 1
        min = 0
        max = 3
        c_end = 0.1
        [[spsa.params]]
        name = "tune_rhome_first_row"
        start = 3
        min = 0
        max = 10
        int = true
        c_end = 1
        r_end = 0.003
        [[engines]]
        name = "opfor"
        program = "bin/opfor"
        options = { hash = 64 }
    "#;

    #[test]
    fn reads_and_writes_an_spsa_spec() {
        let spec = TestSpec::parse(SPSA).unwrap();
        let spsa = spec.spsa.as_ref().unwrap();
        assert_eq!((spsa.pairs_per_iteration, spsa.alpha, spsa.gamma, spsa.a_ratio), (8, 0.602, 0.101, 0.1));
        assert_eq!(spsa.params[0].start, 1.0);
        assert_eq!(spsa.params[0].r_end, 0.002);
        assert!(spsa.params[1].int && !spsa.params[0].int);
        assert_eq!(spec.pair_limit(), Some(800));
        assert_eq!(spec.side_names(), ["opfor+", "opfor-"]);
        assert_eq!(spec.engine(1).name, "opfor");
        assert_eq!(TestSpec::parse(&spec.to_toml()).unwrap(), spec);
    }

    #[test]
    fn refuses_spsa_specs_that_cannot_run() {
        let two = format!("{SPSA}\n        [[engines]]\n        name = \"b\"\n        program = \"b\"");
        assert!(TestSpec::parse(&two).unwrap_err().contains("one engine"));
        let limit = SPSA.replace("[spsa]", "max_pairs = 5\n        [spsa]");
        assert!(TestSpec::parse(&limit).unwrap_err().contains("max_pairs"));
        let bounds = SPSA.replace("start = 3", "start = 30");
        assert!(TestSpec::parse(&bounds).unwrap_err().contains("start between"));
        let step = SPSA.replace("c_end = 1\n", "c_end = 0\n");
        assert!(TestSpec::parse(&step).unwrap_err().contains("c_end"));
        let twice = SPSA.replace("tune_rhome_first_row", "eval_frozen");
        assert!(TestSpec::parse(&twice).unwrap_err().contains("twice"));
        let clash = SPSA.replace("hash = 64", "eval_frozen = 2");
        assert!(TestSpec::parse(&clash).unwrap_err().contains("both an engine option"));
        let none = SPSA.replace("iterations = 100", "iterations = 0");
        assert!(TestSpec::parse(&none).unwrap_err().contains("at least 1"));
    }

    #[test]
    fn memory_limits_default_and_round_trip() {
        let spec = TestSpec::parse(SPEC).unwrap();
        assert_eq!(spec.memory, MemorySpec::default());
        assert!(!spec.to_toml().contains("[memory]"));
        assert_eq!(spec.memory.limit_bytes(Some(32 << 30)), 8 << 30);
        assert_eq!(spec.memory.limit_bytes(None), 4 << 30);
        let set = TestSpec::parse(&format!(
            "{SPEC}\n        [memory]\n        growth = 0\n        limit_mb = 3000"
        ))
        .unwrap();
        assert_eq!(set.memory, MemorySpec { growth: 0.0, restart_mb: None, limit_mb: Some(3000) });
        assert_eq!(set.memory.limit_bytes(Some(32 << 30)), 3000 << 20);
        assert_eq!(TestSpec::parse(&set.to_toml()).unwrap(), set);
    }
}
