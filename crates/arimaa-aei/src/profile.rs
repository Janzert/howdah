//! What the controller knows about particular engines: the options that
//! make them analyse (search until `stop`, report progress), and how to
//! read their scores. Engine quirks live here and nowhere else.

use crate::engine::EngineId;
use crate::message::SearchEval;

/// An evaluation from the mover's point of view, in centi-rabbits (a rabbit
/// up in the opening is about +100), or a proven result.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Score {
    CentiRabbits(i32),
    Win,
    Loss,
}

impl Score {
    /// The same score from the other side's point of view.
    pub fn flipped(self) -> Score {
        match self {
            Score::CentiRabbits(v) => Score::CentiRabbits(-v),
            Score::Win => Score::Loss,
            Score::Loss => Score::Win,
        }
    }

    /// Centi-rabbits, or `None` for a proven result.
    pub fn centi_rabbits(self) -> Option<i32> {
        match self {
            Score::CentiRabbits(v) => Some(v),
            Score::Win | Score::Loss => None,
        }
    }
}

/// An engine the controller knows, picked from its `id name`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Profile {
    /// bot_Sharp: progress as `log` lines (`SearchLog`), about 10 per
    /// centi-rabbit, no `go infinite`.
    Sharp,
    /// bot_OpFor: standard `info` lines in centi-rabbits; searches until
    /// `stop` when no time control is set.
    OpFor,
    /// Any other engine: `info score` read as centi-rabbits, no win
    /// detection.
    Generic,
}

/// Sharp's scale: about 1000 per rabbit.
const SHARP_PER_CENTI_RABBIT: i32 = 10;

/// OpFor's proven wins: `MIN_WIN_SCORE` (63980) divided by 1.96, as it
/// writes scores.
const OPFOR_WIN: i32 = 32_640;

impl Profile {
    pub fn detect(id: &EngineId) -> Profile {
        let name = id.name.as_deref().unwrap_or("").trim().to_ascii_lowercase();
        match name.strip_prefix("bot_").unwrap_or(&name) {
            "sharp" => Profile::Sharp,
            "opfor" => Profile::OpFor,
            _ => Profile::Generic,
        }
    }

    /// Options for analysis, sent after the handshake: searching until
    /// `stop`, and reporting progress while searching.
    pub fn analysis_options(self) -> &'static [(&'static str, &'static str)] {
        match self {
            // Sharp has no `go infinite`; `ignoretc` makes `go` search until
            // `stop`. `verbose` needs our build of Sharp (see
            // `tools/build-sharp.sh` in the parent repo).
            Profile::Sharp => &[("ignoretc", "true"), ("verbose", "true")],
            Profile::OpFor | Profile::Generic => &[],
        }
    }

    /// Whether the engine's `log` lines carry its search (`SearchLog`).
    pub fn logs_search(self) -> bool {
        self == Profile::Sharp
    }

    /// Reads an `info score` value.
    pub fn info_score(self, value: i32) -> Score {
        match self {
            Profile::OpFor if value >= OPFOR_WIN => Score::Win,
            Profile::OpFor if value <= -OPFOR_WIN => Score::Loss,
            _ => Score::CentiRabbits(value),
        }
    }

    /// Reads the eval of a search log line.
    pub fn log_score(self, eval: &SearchEval) -> Score {
        match eval {
            SearchEval::Score(v) => Score::CentiRabbits(v / SHARP_PER_CENTI_RABBIT),
            SearchEval::Decided(text) if text.starts_with("Win") => Score::Win,
            SearchEval::Decided(_) => Score::Loss,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn id(name: &str) -> EngineId {
        EngineId { name: Some(name.into()), ..EngineId::default() }
    }

    #[test]
    fn detects_profiles_by_name() {
        assert_eq!(Profile::detect(&id("sharp")), Profile::Sharp);
        assert_eq!(Profile::detect(&id("bot_Sharp")), Profile::Sharp);
        assert_eq!(Profile::detect(&id("OpFor")), Profile::OpFor);
        assert_eq!(Profile::detect(&id("aei-test-engine")), Profile::Generic);
        assert_eq!(Profile::detect(&EngineId::default()), Profile::Generic);
    }

    #[test]
    fn reads_scores_on_a_common_scale() {
        assert_eq!(Profile::OpFor.info_score(121), Score::CentiRabbits(121));
        assert_eq!(Profile::OpFor.info_score(32_653), Score::Win);
        assert_eq!(Profile::OpFor.info_score(-32_653), Score::Loss);
        assert_eq!(Profile::Generic.info_score(40_000), Score::CentiRabbits(40_000));
        assert_eq!(Profile::Sharp.log_score(&SearchEval::Score(-1078)), Score::CentiRabbits(-107));
        assert_eq!(Profile::Sharp.log_score(&SearchEval::Decided("Win13".into())), Score::Win);
        assert_eq!(Profile::Sharp.log_score(&SearchEval::Decided("Loss5".into())), Score::Loss);
        assert_eq!(Score::Win.flipped(), Score::Loss);
        assert_eq!(Score::CentiRabbits(30).flipped(), Score::CentiRabbits(-30));
    }
}
