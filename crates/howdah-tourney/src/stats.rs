//! A test's statistics, from engine 1's side (index 0).
//!
//! Arimaa games can't be drawn, so a game is a win or a loss. Results are
//! counted by colour-swapped pair: a pair scores 0, ½ or 1 (the mean of its
//! two games). Arimaa games diverge fast and neither colour has a shown
//! advantage, so the two games of a pair are nearly independent and this
//! gives about the same answer as counting games; it stays right if they
//! aren't. Only complete pairs count.
//!
//! The SPRT is the generalized SPRT of fishtest and fastchess, with the
//! normal approximation: for `n` pairs with mean score `m` and variance
//! `v`, and `s0`, `s1` the scores expected at `elo0` and `elo1`,
//! `LLR = n (s1 − s0) (2m − s0 − s1) / (2v)`. The variance has one pair of
//! each outcome added (see [`llr`]), and the test decides at the first
//! pair that takes the LLR past a bound.

use std::collections::{BTreeMap, HashMap, HashSet};

use serde::{Deserialize, Serialize};

use crate::results::GameEntry;
use crate::spec::SprtSpec;

/// The SPRT's verdict on engine 1.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum Decision {
    /// At least `elo1` stronger, more likely than at most `elo0`.
    Pass,
    /// The other way round.
    Fail,
}

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Summary {
    /// Complete pairs.
    pub pairs: u32,
    /// Every logged game, in a complete pair or not.
    pub games: u32,
    /// Games won by each engine.
    pub wins: [u32; 2],
    /// Games each engine won as gold.
    pub gold_wins: [u32; 2],
    /// Complete pairs in which engine 1 won 0, 1 and 2 games.
    pub pair_counts: [u32; 3],
    /// Engine 1's mean pair score (0 to 1).
    pub score: Option<f64>,
    #[serde(with = "infinite")]
    pub elo: Option<f64>,
    /// 95% interval of the Elo difference; None while every pair has
    /// scored the same.
    #[serde(with = "infinite_pair")]
    pub elo_interval: Option<(f64, f64)>,
    /// Likelihood of superiority: the chance engine 1 is the stronger.
    pub los: Option<f64>,
    pub llr: Option<f64>,
    /// The LLR bounds (lower, upper) from alpha and beta.
    pub llr_bounds: Option<(f64, f64)>,
    /// Set by the first pair that took the LLR past a bound, even if later
    /// games bring it back.
    pub decision: Option<Decision>,
    /// Games by how they ended.
    pub reasons: BTreeMap<String, u32>,
    /// Games whose moves are the same as an earlier game's.
    pub repeated: u32,
}

/// Elo can be infinite (every game won), which JSON can't hold: these
/// write it as `"+inf"` or `"-inf"`.
mod infinite {
    use serde::{Deserialize, Deserializer, Serialize, Serializer};

    #[derive(Serialize, Deserialize)]
    #[serde(untagged)]
    pub(super) enum Number {
        Finite(f64),
        Text(String),
    }

    pub(super) fn to_number(x: f64) -> Number {
        match x {
            f64::INFINITY => Number::Text("+inf".into()),
            f64::NEG_INFINITY => Number::Text("-inf".into()),
            x => Number::Finite(x),
        }
    }

    pub(super) fn from_number<E: serde::de::Error>(n: Number) -> Result<f64, E> {
        match n {
            Number::Finite(x) => Ok(x),
            Number::Text(t) if t == "+inf" => Ok(f64::INFINITY),
            Number::Text(t) if t == "-inf" => Ok(f64::NEG_INFINITY),
            Number::Text(t) => Err(E::custom(format!("not a number: {t:?}"))),
        }
    }

    pub fn serialize<S: Serializer>(x: &Option<f64>, s: S) -> Result<S::Ok, S::Error> {
        x.map(to_number).serialize(s)
    }

    pub fn deserialize<'de, D: Deserializer<'de>>(d: D) -> Result<Option<f64>, D::Error> {
        Option::<Number>::deserialize(d)?.map(from_number).transpose()
    }
}

mod infinite_pair {
    use serde::{Deserialize, Deserializer, Serialize, Serializer};

    use super::infinite::{Number, from_number, to_number};

    pub fn serialize<S: Serializer>(x: &Option<(f64, f64)>, s: S) -> Result<S::Ok, S::Error> {
        x.map(|(a, b)| (to_number(a), to_number(b))).serialize(s)
    }

    pub fn deserialize<'de, D: Deserializer<'de>>(d: D) -> Result<Option<(f64, f64)>, D::Error> {
        match Option::<(Number, Number)>::deserialize(d)? {
            Some((a, b)) => Ok(Some((from_number(a)?, from_number(b)?))),
            None => Ok(None),
        }
    }
}

/// The Elo difference a score (0 to 1) means; infinite at 0 and 1.
pub fn elo_from_score(score: f64) -> f64 {
    if score <= 0.0 {
        f64::NEG_INFINITY
    } else if score >= 1.0 {
        f64::INFINITY
    } else {
        -400.0 * (1.0 / score - 1.0).log10()
    }
}

/// The score an Elo difference predicts.
pub fn score_from_elo(elo: f64) -> f64 {
    1.0 / (1.0 + 10f64.powf(-elo / 400.0))
}

pub fn summarize(sprt: Option<SprtSpec>, entries: &[GameEntry]) -> Summary {
    let mut s = Summary { games: entries.len() as u32, ..Summary::default() };
    let mut pairs: HashMap<u32, [Option<usize>; 2]> = HashMap::new();
    let mut seen = HashSet::new();
    // Pairs are counted as they complete, in log order, so the SPRT stops
    // at its first crossing: games that finish after it (already running
    // then) are counted in the results but don't undo the decision.
    for e in entries {
        s.wins[e.winner] += 1;
        if e.winner == e.gold() {
            s.gold_wins[e.winner] += 1;
        }
        *s.reasons.entry(e.reason.clone()).or_default() += 1;
        if !seen.insert(e.moves_hash.as_str()) {
            s.repeated += 1;
        }
        let games = pairs.entry(e.pair).or_default();
        let Some(slot) = games.get_mut(usize::from(e.game.saturating_sub(1))) else { continue };
        *slot = Some(e.winner);
        if let [Some(a), Some(b)] = *games {
            s.pairs += 1;
            s.pair_counts[usize::from(a == 0) + usize::from(b == 0)] += 1;
            if let Some(sp) = sprt
                && s.decision.is_none()
            {
                s.decision = decide(sp, llr(sp, s.pair_counts));
            }
        }
    }
    if let Some(sp) = sprt {
        s.llr = Some(llr(sp, s.pair_counts));
        s.llr_bounds = Some(llr_bounds(sp));
    }
    if s.pairs == 0 {
        return s;
    }

    let n = f64::from(s.pairs);
    let (m, v) = mean_var(s.pair_counts.map(f64::from));
    let se = (v / n).sqrt();
    s.score = Some(m);
    s.elo = Some(elo_from_score(m));
    // With every pair scoring the same there's no spread to measure.
    s.elo_interval = (se > 0.0).then(|| (elo_from_score(m - 1.96 * se), elo_from_score(m + 1.96 * se)));
    s.los = Some(if se > 0.0 {
        phi((m - 0.5) / se)
    } else if m > 0.5 {
        1.0
    } else if m < 0.5 {
        0.0
    } else {
        0.5
    });
    s
}

/// The log-likelihood ratio for these pair counts. The variance is taken
/// as if one more pair of each outcome had been played: early on the
/// sample variance can be near zero (after one pair it is zero), which
/// would make the LLR cross a bound at once. The extra pairs matter little
/// once there are a few dozen real ones, and the mean stays the real one.
fn llr(sp: SprtSpec, counts: [u32; 3]) -> f64 {
    let n: u32 = counts.iter().sum();
    if n == 0 {
        return 0.0;
    }
    let real = counts.map(f64::from);
    let (m, _) = mean_var(real);
    let (_, v) = mean_var(real.map(|c| c + 1.0));
    let (s0, s1) = (score_from_elo(sp.elo0), score_from_elo(sp.elo1));
    f64::from(n) * (s1 - s0) * (2.0 * m - s0 - s1) / (2.0 * v)
}

/// The chance that an undecided SPRT still reaches a bound by `max_pairs`;
/// None once it has decided, or before the first pair.
///
/// The rest of the test is taken as a Brownian motion from the current
/// LLR, with the per-pair drift and variance the LLR has at a true score
/// `q`, and the chances of crossing each bound within the pairs left (the
/// first-passage formula) are added, which slightly overstates it. That is
/// averaged over a normal posterior for `q`: the observed score, with the
/// LLR's variance over `n`. Checked against a simulation of the exact
/// test in the parent repo's SPRT cutoff chart.
pub fn decide_chance(sp: SprtSpec, s: &Summary, max_pairs: u32) -> Option<f64> {
    if s.decision.is_some() || s.pairs == 0 {
        return None;
    }
    Some(chance_at(sp, s.pair_counts, max_pairs))
}

/// [`decide_chance`] for an undecided test with these pair counts (at
/// least one pair).
fn chance_at(sp: SprtSpec, counts: [u32; 3], max_pairs: u32) -> f64 {
    let n: u32 = counts.iter().sum();
    let (lower, upper) = llr_bounds(sp);
    let l = llr(sp, counts);
    if l >= upper || l <= lower {
        return 1.0;
    }
    let left = f64::from(max_pairs.saturating_sub(n));
    if left == 0.0 {
        return 0.0;
    }
    let (s0, s1) = (score_from_elo(sp.elo0), score_from_elo(sp.elo1));
    let d = s1 - s0;
    let hit = |q: f64| {
        let v = q * (1.0 - q) / 2.0;
        let (mu, s2) = (d * (2.0 * q - s0 - s1) / (2.0 * v), d * d / v);
        (cross(upper - l, mu, s2, left) + cross(l - lower, -mu, s2, left)).min(1.0)
    };
    let (m, _) = mean_var(counts.map(f64::from));
    let (_, v) = mean_var(counts.map(|c| f64::from(c) + 1.0));
    let sd = (v / f64::from(n)).sqrt();
    // A grid over ±4 sd, a third of one apart.
    let (mut sum, mut weights) = (0.0, 0.0);
    for i in 0..=24 {
        let z = -4.0 + f64::from(i) / 3.0;
        let w = (-z * z / 2.0).exp();
        sum += w * hit((m + sd * z).clamp(1e-6, 1.0 - 1e-6));
        weights += w;
    }
    sum / weights
}

/// How an undecided test is likely to end, from simulating the rest of it
/// ([`project`]).
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Projection {
    /// The chances it passes, fails, ends undecided (the chance to decide
    /// falling below the cutoff) and plays every pair (without a pair
    /// limit: is still going after [`NO_LIMIT_HORIZON`] more pairs). They
    /// add up to 1.
    pub pass: f64,
    pub fail: f64,
    pub undecided: f64,
    pub done: f64,
    /// Pairs left to complete when it ends, at the 10th, 50th and 90th
    /// percentiles.
    pub pairs_left: [u32; 3],
    /// The same in games: two a pair, less those already played in
    /// incomplete pairs. Games running when the test ends still finish,
    /// so a few more than this are played.
    pub games_left: [u32; 3],
}

/// Within this many times the cutoff, [`project`] checks the chance to
/// decide after every pair.
const NEAR: f64 = 3.0;

/// How far a test without a pair limit is simulated.
pub const NO_LIMIT_HORIZON: u32 = 20_000;

/// Paths simulated by [`project`].
const PATHS: u32 = 500;

/// How an undecided SPRT test is likely to end, and when: None once it
/// has decided, or before the first pair.
///
/// The rest of the test is simulated pair by pair, many times, with the
/// runner's own rules: the LLR deciding at the first crossing, the pair
/// limit, and, with `undecided_below`, the [`decide_chance`] cutoff. Each
/// path draws a true Elo difference from its posterior given the pairs so
/// far ([`prior`] times the likelihood of the mean score), and splits
/// pairs as often as they have split, so early on the endings spread over
/// the Elo the test could still turn out to have. The cutoff is checked
/// every few pairs, and on falling below it the pair it first did is
/// found by bisection. Simulations are seeded from the counts, so the
/// same results give the same projection.
pub fn project(
    sp: SprtSpec,
    s: &Summary,
    max_pairs: Option<u32>,
    undecided_below: Option<f64>,
) -> Option<Projection> {
    project_paths(sp, s, max_pairs, undecided_below, PATHS)
}

fn project_paths(
    sp: SprtSpec,
    s: &Summary,
    max_pairs: Option<u32>,
    undecided_below: Option<f64>,
    paths: u32,
) -> Option<Projection> {
    if s.decision.is_some() || s.pairs == 0 {
        return None;
    }
    let start = s.pairs;
    let limit = max_pairs.unwrap_or(start + NO_LIMIT_HORIZON);
    // The runner only stops undecided with a pair limit.
    let cutoff = undecided_below.filter(|_| max_pairs.is_some());
    let chance = |counts: [u32; 3]| chance_at(sp, counts, limit);
    let partial = s.games.saturating_sub(2 * s.pairs);
    let games = |pairs: u32| (2 * pairs).saturating_sub(partial);
    let first = cutoff.map(|_| chance(s.pair_counts));
    if start >= limit || first.zip(cutoff).is_some_and(|(f, c)| f < c) {
        let p = [0; 3];
        let (undecided, done) = if start >= limit { (0.0, 1.0) } else { (1.0, 0.0) };
        return Some(Projection { pass: 0.0, fail: 0.0, undecided, done, pairs_left: p, games_left: p });
    }

    let (lower, upper) = llr_bounds(sp);
    let mut rng = Rng::seeded(&[
        u64::from(s.pair_counts[0]),
        u64::from(s.pair_counts[1]),
        u64::from(s.pair_counts[2]),
        u64::from(limit),
        cutoff.map_or(0, f64::to_bits),
        sp.elo0.to_bits() ^ sp.elo1.to_bits().rotate_left(17),
    ]);
    // The posterior for the Elo difference on a grid: the prior times the
    // normal likelihood of the mean pair score (with the LLR's variance).
    let n = f64::from(s.pairs);
    let (m, _) = mean_var(s.pair_counts.map(f64::from));
    let (_, v) = mean_var(s.pair_counts.map(|c| f64::from(c) + 1.0));
    let elos: Vec<f64> = (-1200..=1200).map(|i| f64::from(i) / 4.0).collect();
    let weights = elos.iter().map(|&e| prior(e) * (-(m - score_from_elo(e)).powi(2) * n / (2.0 * v)).exp());
    let cdf: Vec<f64> = weights.scan(0.0, |sum, w| Some(*sum + w).inspect(|&t| *sum = t)).collect();
    // Split pairs as often as so far (with the extra pair of each).
    let split = (f64::from(s.pair_counts[1]) + 1.0) / (n + 3.0);
    let left = limit - start;
    // The cutoff costs far more to check than a pair: about 40 times a
    // path while the chance is well above it, every pair once it's near
    // (the chance moves with the LLR's noise, and the runner catches the
    // first dip under it).
    let every = (left / 40).max(1);
    let mut ends = [0u32; 4];
    let mut lengths = Vec::with_capacity(paths as usize);
    let mut path: Vec<[u32; 3]> = Vec::new();
    for _ in 0..paths {
        let u = rng.unit() * cdf[cdf.len() - 1];
        let q = score_from_elo(elos[cdf.partition_point(|&c| c < u)]);
        let (p0, p01) = ((1.0 - q - split / 2.0).max(0.0), (1.0 - q + split / 2.0).min(1.0));
        let mut counts = s.pair_counts;
        let (mut checked, mut last) = (0, first.unwrap_or(1.0));
        path.clear();
        let (end, played) = loop {
            let u = rng.unit();
            counts[if u < p0 {
                0
            } else if u < p01 {
                1
            } else {
                2
            }] += 1;
            path.push(counts);
            let played = path.len() as u32;
            let l = llr(sp, counts);
            if l >= upper {
                break (0, played);
            }
            if l <= lower {
                break (1, played);
            }
            if played == left {
                break (3, played);
            }
            let Some(c) = cutoff else { continue };
            if played - checked < if last < NEAR * c { 1 } else { every } {
                continue;
            }
            last = chance(counts);
            if last < c {
                // The first pair since the last check under the cutoff.
                let (mut lo, mut hi) = (checked, played);
                while hi - lo > 1 {
                    let mid = lo + (hi - lo) / 2;
                    if chance(path[mid as usize - 1]) < c {
                        hi = mid;
                    } else {
                        lo = mid;
                    }
                }
                break (2, hi);
            }
            checked = played;
        };
        ends[end] += 1;
        lengths.push(played);
    }
    lengths.sort_unstable();
    let at = |p: f64| lengths[((p * f64::from(paths)) as usize).min(lengths.len() - 1)];
    let pairs_left = [at(0.1), at(0.5), at(0.9)];
    let share = |k: usize| f64::from(ends[k]) / f64::from(paths);
    Some(Projection {
        pass: share(0),
        fail: share(1),
        undecided: share(2),
        done: share(3),
        pairs_left,
        games_left: pairs_left.map(games),
    })
}

/// The prior for a test's Elo difference in [`project`] (unnormalized): a
/// change usually moves an engine's strength by a few Elo either way,
/// but now and then by a lot (a bug, or a fixed one). Without it early
/// projections would take every Elo the results can't yet rule out as
/// likely, and so expect a quick pass or fail far too often.
fn prior(elo: f64) -> f64 {
    let normal = |sd: f64| (-(elo / sd).powi(2) / 2.0).exp() / sd;
    0.8 * normal(PRIOR_SD[0]) + 0.2 * normal(PRIOR_SD[1])
}

/// The spreads of [`prior`]'s two parts, in Elo.
const PRIOR_SD: [f64; 2] = [15.0, 80.0];

/// SplitMix64, for [`project`]'s simulations: small, and repeatable from
/// a seed.
struct Rng(u64);

impl Rng {
    fn seeded(words: &[u64]) -> Rng {
        let mut r = Rng(0x05ee_d0fa_71aa);
        for &w in words {
            r.0 ^= w;
            r.next();
        }
        r
    }

    fn next(&mut self) -> u64 {
        self.0 = self.0.wrapping_add(0x9e37_79b9_7f4a_7c15);
        let mut z = self.0;
        z = (z ^ (z >> 30)).wrapping_mul(0xbf58_476d_1ce4_e5b9);
        z = (z ^ (z >> 27)).wrapping_mul(0x94d0_49bb_1331_11eb);
        z ^ (z >> 31)
    }

    /// Uniform in [0, 1).
    fn unit(&mut self) -> f64 {
        (self.next() >> 11) as f64 / (1u64 << 53) as f64
    }

    #[cfg(test)]
    fn normal(&mut self) -> f64 {
        let u = 1.0 - self.unit();
        (-2.0 * u.ln()).sqrt() * (std::f64::consts::TAU * self.unit()).cos()
    }
}

/// The chance that a Brownian motion with drift `mu` and variance `s2` per
/// pair rises by `h` within `t` pairs.
fn cross(h: f64, mu: f64, s2: f64, t: f64) -> f64 {
    let sd = (s2 * t).sqrt();
    ln_phi((mu * t - h) / sd).exp() + (2.0 * mu * h / s2 + ln_phi((-h - mu * t) / sd)).exp()
}

/// ln Φ(x), accurate far into the lower tail, where `phi` underflows.
fn ln_phi(x: f64) -> f64 {
    // erfc from Numerical Recipes (relative error below 1.2e-7), in logs.
    let z = x.abs() / std::f64::consts::SQRT_2;
    let t = 1.0 / (1.0 + 0.5 * z);
    let poly = -1.265_512_23
        + t * (1.000_023_68
            + t * (0.374_091_96
                + t * (0.096_784_18
                    + t * (-0.186_288_06
                        + t * (0.278_868_07
                            + t * (-1.135_203_98
                                + t * (1.488_515_87 + t * (-0.822_152_23 + t * 0.170_872_77))))))));
    let ln_erfc = t.ln() - z * z + poly;
    if x < 0.0 { 0.5f64.ln() + ln_erfc } else { (-0.5 * ln_erfc.exp()).ln_1p() }
}

fn decide(sp: SprtSpec, llr: f64) -> Option<Decision> {
    let (lower, upper) = llr_bounds(sp);
    if llr >= upper {
        Some(Decision::Pass)
    } else if llr <= lower {
        Some(Decision::Fail)
    } else {
        None
    }
}

fn llr_bounds(sp: SprtSpec) -> (f64, f64) {
    ((sp.beta / (1.0 - sp.alpha)).ln(), ((1.0 - sp.beta) / sp.alpha).ln())
}

/// Mean and variance of pair scores 0, ½ and 1 with these counts.
fn mean_var(counts: [f64; 3]) -> (f64, f64) {
    let total: f64 = counts.iter().sum();
    let xs = [0.0, 0.5, 1.0];
    let m = counts.iter().zip(xs).map(|(c, x)| c * x).sum::<f64>() / total;
    let v = counts.iter().zip(xs).map(|(c, x)| c * (x - m).powi(2)).sum::<f64>() / total;
    (m, v)
}

/// The standard normal distribution function.
fn phi(x: f64) -> f64 {
    0.5 * (1.0 + erf(x / std::f64::consts::SQRT_2))
}

/// Abramowitz and Stegun 7.1.26 (error below 1.5e-7).
fn erf(x: f64) -> f64 {
    let t = 1.0 / (1.0 + 0.327_591_1 * x.abs());
    let poly = t
        * (0.254_829_592
            + t * (-0.284_496_736 + t * (1.421_413_741 + t * (-1.453_152_027 + t * 1.061_405_429))));
    let y = 1.0 - poly * (-x * x).exp();
    if x < 0.0 { -y } else { y }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::results::entry;

    const SPRT: SprtSpec = SprtSpec { elo0: 0.0, elo1: 10.0, alpha: 0.05, beta: 0.05, undecided_below: None };

    /// Pairs scoring 0, 1 and 2 wins for engine 0, `counts` of each,
    /// spread evenly through the log (the SPRT decides in log order).
    fn pairs(counts: [u32; 3]) -> Vec<GameEntry> {
        let total: u32 = counts.iter().sum();
        let mut used = [0u32; 3];
        let mut out = Vec::new();
        for pair in 1..=total {
            // The outcome furthest behind its share so far.
            let behind =
                |k: usize| f64::from(counts[k]) * f64::from(pair) / f64::from(total) - f64::from(used[k]);
            let wins = (0..3)
                .filter(|&k| used[k] < counts[k])
                .max_by(|&a, &b| behind(a).total_cmp(&behind(b)))
                .unwrap();
            used[wins] += 1;
            out.push(entry(pair, 1, if wins >= 1 { 0 } else { 1 }));
            out.push(entry(pair, 2, if wins >= 2 { 0 } else { 1 }));
        }
        out
    }

    fn close(a: f64, b: f64) -> bool {
        (a - b).abs() < 1e-6 * b.abs().max(1.0)
    }

    #[test]
    fn elo_and_score_are_inverses() {
        assert_eq!(elo_from_score(0.5), 0.0);
        assert!(close(score_from_elo(elo_from_score(0.64)), 0.64));
        assert!(close(elo_from_score(score_from_elo(-120.0)), -120.0));
        assert_eq!(elo_from_score(1.0), f64::INFINITY);
    }

    #[test]
    fn erf_matches_known_values() {
        assert!((erf(0.0)).abs() < 1e-7);
        assert!((erf(1.0) - 0.842_700_79).abs() < 2e-7);
        assert!((erf(-2.0) + 0.995_322_27).abs() < 2e-7);
        assert!((phi(1.96) - 0.975).abs() < 1e-3);
    }

    #[test]
    fn counts_complete_pairs_only() {
        let mut games = pairs([1, 2, 3]);
        games.push(entry(99, 1, 0));
        let s = summarize(None, &games);
        assert_eq!(s.pairs, 6);
        assert_eq!(s.games, 13);
        assert_eq!(s.pair_counts, [1, 2, 3]);
        assert_eq!(s.wins, [2 + 6 + 1, 2 + 2]);
        // Engine 0 is gold in game 1 of each pair.
        assert_eq!(s.gold_wins, [2 + 3 + 1, 1 + 2]);
        assert!(close(s.score.unwrap(), (1.0 + 3.0) / 6.0));
        assert_eq!(s.repeated, 0);
    }

    #[test]
    fn elo_interval_and_los() {
        // 60 pairs: 10 lost, 20 split, 30 won: m = 0.6667, v = 0.0556.
        let s = summarize(None, &pairs([10, 20, 30]));
        let m: f64 = 40.0 / 60.0;
        let v = (10.0 * m * m + 20.0 * (0.5 - m).powi(2) + 30.0 * (1.0 - m).powi(2)) / 60.0;
        let se = (v / 60.0f64).sqrt();
        assert!(close(s.elo.unwrap(), elo_from_score(m)));
        let (lo, hi) = s.elo_interval.unwrap();
        assert!(close(lo, elo_from_score(m - 1.96 * se)));
        assert!(close(hi, elo_from_score(m + 1.96 * se)));
        assert!(s.los.unwrap() > 0.999);
        let even = summarize(None, &pairs([5, 10, 5]));
        assert!(close(even.los.unwrap(), 0.5));
        assert_eq!(even.elo.unwrap(), 0.0);
    }

    #[test]
    fn sprt_llr_follows_the_normal_approximation() {
        let s = summarize(Some(SPRT), &pairs([30, 40, 35]));
        let n: f64 = 105.0;
        let m = (40.0 * 0.5 + 35.0) / n;
        // The variance has one more pair of each outcome.
        let mv = (41.0 * 0.5 + 36.0) / (n + 3.0);
        let v = (31.0 * mv * mv + 41.0 * (0.5 - mv).powi(2) + 36.0 * (1.0 - mv).powi(2)) / (n + 3.0);
        let (s0, s1) = (0.5, score_from_elo(10.0));
        let llr = n * (s1 - s0) * (2.0 * m - s0 - s1) / (2.0 * v);
        assert!(close(s.llr.unwrap(), llr));
        let (lo, hi) = s.llr_bounds.unwrap();
        assert!(close(lo, (0.05f64 / 0.95).ln()) && close(hi, (0.95f64 / 0.05).ln()));
        assert_eq!(s.decision, None);
    }

    #[test]
    fn sprt_decides_both_ways() {
        let strong = summarize(Some(SPRT), &pairs([100, 400, 200]));
        assert_eq!(strong.decision, Some(Decision::Pass));
        let weak = summarize(Some(SPRT), &pairs([200, 400, 100]));
        assert_eq!(weak.decision, Some(Decision::Fail));
        // All won: one pair decides nothing, but a run of them does.
        let one = summarize(Some(SPRT), &pairs([0, 0, 1]));
        assert!(one.llr.unwrap() < 0.2, "{one:?}");
        assert_eq!(one.decision, None);
        let sweep = summarize(Some(SPRT), &pairs([0, 0, 40]));
        assert_eq!(sweep.decision, Some(Decision::Pass));
        assert_eq!((sweep.elo, sweep.elo_interval), (Some(f64::INFINITY), None));
        let none = summarize(Some(SPRT), &[]);
        assert_eq!((none.llr, none.decision), (Some(0.0), None));
    }

    #[test]
    fn the_first_crossing_decides() {
        // Engine 0 wins 60 pairs, crossing the upper bound, then loses
        // enough to bring the LLR back below it.
        let mut games = pairs([0, 0, 60]);
        let crossed = (1..=60)
            .find(|&n| summarize(Some(SPRT), &games[..2 * n]).decision.is_some())
            .expect("60 won pairs pass");
        for pair in 61..=100 {
            games.push(entry(pair, 1, 1));
            games.push(entry(pair, 2, 1));
        }
        let s = summarize(Some(SPRT), &games);
        assert!(s.llr.unwrap() < s.llr_bounds.unwrap().1);
        assert_eq!(s.decision, Some(Decision::Pass), "decided at pair {crossed}");
    }

    #[test]
    fn a_summary_survives_json_with_infinite_elo() {
        let sweep = summarize(Some(SPRT), &pairs([0, 0, 3]));
        let mut half = summarize(None, &pairs([2, 2, 1]));
        half.elo_interval = Some((f64::NEG_INFINITY, 12.5));
        for s in [sweep, half] {
            let json = serde_json::to_string(&s).unwrap();
            assert_eq!(serde_json::from_str::<Summary>(&json).unwrap(), s, "{json}");
        }
    }

    /// A summary of `n` pairs of independent games, each won with `q`.
    fn expected(n: u32, q: f64) -> Summary {
        let nf = f64::from(n);
        let won = (nf * q * q).round() as u32;
        let lost = (nf * (1.0 - q) * (1.0 - q)).round() as u32;
        Summary { pairs: n, pair_counts: [lost, n - won - lost, won], ..Summary::default() }
    }

    #[test]
    fn decide_chance_projects_the_rest_of_the_test() {
        // 8000 of 10000 games at +5 Elo: a simulation of the exact test
        // from there decides about 6% of the time (the cutoff chart).
        let mid = decide_chance(SPRT, &expected(4000, score_from_elo(5.0)), 5000).unwrap();
        assert!((0.04..0.10).contains(&mid), "{mid}");
        // Early on the true Elo could be well off the middle, so it likely
        // decides (though not surely in 6000 games); at the limit it can't.
        let early = decide_chance(SPRT, &expected(100, score_from_elo(5.0)), 3000).unwrap();
        assert!((0.8..0.95).contains(&early), "{early}");
        assert_eq!(decide_chance(SPRT, &expected(3000, score_from_elo(5.0)), 3000), Some(0.0));
        // Further from the middle it's likelier to get there in time.
        let off = decide_chance(SPRT, &expected(4000, score_from_elo(8.0)), 5000).unwrap();
        assert!(off > mid, "{off} vs {mid}");
        let decided = Summary { decision: Some(Decision::Pass), ..expected(100, 0.6) };
        assert_eq!(decide_chance(SPRT, &decided, 3000), None);
        assert_eq!(decide_chance(SPRT, &Summary::default(), 3000), None);
    }

    const CUT: SprtSpec = SprtSpec { undecided_below: Some(0.05), ..SPRT };

    #[test]
    fn project_covers_every_ending() {
        // Early on, at +5 Elo (the middle of [0, 10]), any ending is
        // possible; the cutoff ends many before the limit.
        let p = project(CUT, &expected(100, score_from_elo(5.0)), Some(3000), Some(0.05)).unwrap();
        let total = p.pass + p.fail + p.undecided + p.done;
        assert!((total - 1.0).abs() < 1e-9, "{p:?}");
        assert!(p.pass > 0.1 && p.fail > 0.1 && p.undecided > 0.1, "{p:?}");
        assert!(p.pairs_left[0] <= p.pairs_left[1] && p.pairs_left[1] <= p.pairs_left[2], "{p:?}");
        assert!(p.pairs_left[2] <= 2900, "{p:?}");
        assert_eq!(p.games_left, p.pairs_left.map(|n| 2 * n), "no partial pairs");
        // Without the cutoff it plays on to the limit instead.
        let on = project(SPRT, &expected(100, score_from_elo(5.0)), Some(3000), None).unwrap();
        assert_eq!(on.undecided, 0.0);
        assert!(on.done > 0.05, "{on:?}");
        // Well ahead: it likely passes, and soon (though the prior expects
        // some of that lead to be luck).
        let strong = project(CUT, &expected(200, score_from_elo(40.0)), Some(3000), Some(0.05)).unwrap();
        assert!(strong.pass > 0.8 && strong.pairs_left[1] < 700, "{strong:?}");
        // The same counts give the same projection.
        assert_eq!(
            project(CUT, &expected(100, 0.51), Some(3000), Some(0.05)),
            project(CUT, &expected(100, 0.51), Some(3000), Some(0.05))
        );
    }

    #[test]
    fn project_edges() {
        let decided = Summary { decision: Some(Decision::Pass), ..expected(100, 0.6) };
        assert_eq!(project(SPRT, &decided, Some(3000), None), None);
        assert_eq!(project(SPRT, &Summary::default(), Some(3000), None), None);
        // Already below the cutoff (a lowered limit): it ends now.
        let late = project(CUT, &expected(2900, score_from_elo(5.0)), Some(3000), Some(0.05)).unwrap();
        assert_eq!((late.undecided, late.pairs_left), (1.0, [0; 3]));
        // A game played in an unfinished pair is one fewer to play.
        let odd = Summary { games: 201, ..expected(100, score_from_elo(40.0)) };
        let p = project(SPRT, &odd, Some(3000), None).unwrap();
        assert_eq!(p.games_left, p.pairs_left.map(|n| (2 * n).saturating_sub(1)));
        // Without a limit it runs to the horizon at most.
        let open = project(SPRT, &expected(100, score_from_elo(5.0)), None, Some(0.05)).unwrap();
        assert_eq!(open.undecided, 0.0, "no cutoff without a limit");
        assert!(open.pairs_left[2] <= NO_LIMIT_HORIZON, "{open:?}");
    }

    /// Plays a whole test at a true score `q` (independent games) by the
    /// runner's rules, calling `at` with the summary after each pair, and
    /// returns how it ended (0 pass, 1 fail, 2 undecided, 3 done) and its
    /// pairs.
    fn play_test(rng: &mut Rng, q: f64, max: u32, mut at: impl FnMut(&Summary)) -> (usize, u32) {
        let mut s = Summary::default();
        loop {
            let wins = usize::from(rng.unit() < q) + usize::from(rng.unit() < q);
            s.pair_counts[wins] += 1;
            s.pairs += 1;
            s.games += 2;
            if let Some(d) = decide(CUT, llr(CUT, s.pair_counts)) {
                return (if d == Decision::Pass { 0 } else { 1 }, s.pairs);
            }
            if s.pairs == max {
                return (3, s.pairs);
            }
            if decide_chance(CUT, &s, max).is_some_and(|c| c < 0.05) {
                return (2, s.pairs);
            }
            at(&s);
        }
    }

    /// How well projections did over `trials` tests with true Elo drawn
    /// from the prior, each projected after the pairs in `at` (while still
    /// running) with `paths` paths: the projections, how often the 80%
    /// range held the pairs left, how often they were under the median,
    /// and the mean predicted chance and the share of each ending.
    fn calibration(trials: u32, max: u32, at: &[u32], paths: u32) -> (f64, f64, f64, [f64; 4], [f64; 4]) {
        let mut rng = Rng::seeded(&[7]);
        let (mut n, mut inside, mut under) = (0.0, 0.0, 0.0);
        let (mut predicted, mut actual) = ([0.0; 4], [0.0; 4]);
        for _ in 0..trials {
            let sd = if rng.unit() < 0.8 { PRIOR_SD[0] } else { PRIOR_SD[1] };
            let elo = sd * rng.normal();
            let mut seen = Vec::new();
            let (end, pairs) = play_test(&mut rng, score_from_elo(elo), max, |s| {
                if at.contains(&s.pairs) {
                    seen.push((s.pairs, project_paths(CUT, s, Some(max), Some(0.05), paths).unwrap()));
                }
            });
            for (at, p) in seen {
                let left = pairs - at;
                n += 1.0;
                inside += f64::from(u8::from(p.pairs_left[0] <= left && left <= p.pairs_left[2]));
                under += f64::from(u8::from(left < p.pairs_left[1]));
                for (k, c) in [p.pass, p.fail, p.undecided, p.done].into_iter().enumerate() {
                    predicted[k] += c;
                }
                actual[end] += 1.0;
            }
        }
        eprintln!("{n} projections: 80% range held {:.3}, under the median {:.3}", inside / n, under / n);
        for k in 0..4 {
            eprintln!("ending {k}: predicted {:.3}, actual {:.3}", predicted[k] / n, actual[k] / n);
        }
        (n, inside / n, under / n, predicted.map(|c| c / n), actual.map(|c| c / n))
    }

    /// The projection is calibrated: the 80% range of pairs left holds the
    /// real number about 80% of the time, the median is above it about
    /// half the time, and the chances of each ending match how often it
    /// happens.
    #[test]
    fn project_is_calibrated() {
        let (n, inside, under, predicted, actual) = calibration(200, 1000, &[60, 300], 200);
        assert!(n > 250.0, "{n}");
        assert!((0.70..0.90).contains(&inside), "{inside}");
        assert!((0.40..0.60).contains(&under), "{under}");
        for k in 0..4 {
            assert!((predicted[k] - actual[k]).abs() < 0.08, "ending {k}: {predicted:?} vs {actual:?}");
        }
    }

    /// The same, with more tests and longer ones (about half a minute in
    /// a release build): `cargo test --release -p howdah-tourney --
    /// --ignored calibrated`. When last run: the range held 77% of 769,
    /// 48% were under the median, and pass, fail and undecided were
    /// predicted 21%, 36% and 43% against 22%, 34% and 44%.
    #[test]
    #[ignore]
    fn project_is_calibrated_at_length() {
        let (n, inside, under, predicted, actual) = calibration(300, 1500, &[50, 250, 700], 300);
        assert!(n > 600.0, "{n}");
        assert!((0.74..0.86).contains(&inside), "{inside}");
        assert!((0.44..0.56).contains(&under), "{under}");
        for k in 0..4 {
            assert!((predicted[k] - actual[k]).abs() < 0.04, "ending {k}: {predicted:?} vs {actual:?}");
        }
    }

    #[test]
    fn ln_phi_follows_phi_into_the_tail() {
        for x in [-3.0, -1.0, 0.0, 0.5, 2.0] {
            assert!((ln_phi(x).exp() - phi(x)).abs() < 1e-6, "{x}");
        }
        // Φ(−40) is about 3.7e-350, below f64's range; its log isn't.
        assert!((ln_phi(-40.0) + 804.608).abs() < 0.01, "{}", ln_phi(-40.0));
    }

    #[test]
    fn counts_repeated_games() {
        let mut games = pairs([0, 2, 0]);
        games[2].moves_hash = games[0].moves_hash.clone();
        games[3].moves_hash = games[0].moves_hash.clone();
        assert_eq!(summarize(None, &games).repeated, 2);
    }
}
