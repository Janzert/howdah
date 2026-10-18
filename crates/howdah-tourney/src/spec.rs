//! A test's spec, as written in `test.toml`.
//!
//! ```toml
//! name = "opfor-null-move"
//! time_control = "2s/30s/100/0/100t"
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

use std::collections::BTreeMap;
use std::fmt;
use std::path::PathBuf;

use howdah_arimaa::TimeControl;
use serde::{Deserialize, Serialize};

use crate::error::TourneyError;

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TestSpec {
    pub name: String,
    /// In AEI's `M/R/P/L/G/T` format. None plays without clocks.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub time_control: Option<String>,
    /// Games played at once.
    #[serde(default = "one")]
    pub concurrency: u32,
    /// Stop after this many pairs, if the SPRT hasn't decided first.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub max_pairs: Option<u32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub sprt: Option<SprtSpec>,
    /// Keep each game's engine transcripts in `logs/`.
    #[serde(default, skip_serializing_if = "is_false")]
    pub engine_logs: bool,
    /// Exactly two.
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

    pub fn time_control(&self) -> Option<TimeControl> {
        self.time_control.as_deref().map(|t| t.parse().expect("validated"))
    }

    pub fn validate(&self) -> Result<(), TourneyError> {
        let bad = |m: String| Err(TourneyError::Spec(m));
        if self.engines.len() != 2 {
            return bad(format!("a test needs two engines, not {}", self.engines.len()));
        }
        if self.engines.iter().any(|e| e.name.trim().is_empty()) {
            return bad("every engine needs a name".into());
        }
        if self.engines[0].name == self.engines[1].name {
            return bad(format!("both engines are named {:?}", self.engines[0].name));
        }
        if let Some(tc) = &self.time_control
            && let Err(e) = tc.parse::<TimeControl>()
        {
            return bad(format!("time control {tc:?}: {e}"));
        }
        if self.concurrency == 0 {
            return bad("concurrency must be at least 1".into());
        }
        if self.max_pairs == Some(0) {
            return bad("max_pairs must be at least 1".into());
        }
        match self.sprt {
            None if self.max_pairs.is_none() => {
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
    fn refuses_specs_that_cannot_run() {
        let no_end = SPEC.replace("[sprt]\n        elo0 = 0\n        elo1 = 10", "");
        assert!(TestSpec::parse(&no_end).unwrap_err().contains("so the test ends"));
        let same = SPEC.replace("\"base\"", "\"dev\"");
        assert!(TestSpec::parse(&same).unwrap_err().contains("both engines"));
        let tc = SPEC.replace("2s/30s/100/0/0/120", "soon");
        assert!(TestSpec::parse(&tc).unwrap_err().contains("time control"));
        let bounds = SPEC.replace("elo1 = 10", "elo1 = -5");
        assert!(TestSpec::parse(&bounds).unwrap_err().contains("elo1"));
        let unknown = SPEC.replace("concurrency = 2", "concurency = 2");
        assert!(TestSpec::parse(&unknown).is_err());
        let growth = format!("{SPEC}\n        [memory]\n        growth = 0.5");
        assert!(TestSpec::parse(&growth).unwrap_err().contains("growth"));
        let limits = format!("{SPEC}\n        [memory]\n        restart_mb = 900\n        limit_mb = 800");
        assert!(TestSpec::parse(&limits).unwrap_err().contains("restart_mb"));
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
