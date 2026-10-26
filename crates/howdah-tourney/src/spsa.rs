//! SPSA tuning: a test whose two sides are one engine with its parameters
//! nudged apart, which steps the parameters along each iteration's result.
//!
//! A spec with `[spsa]` has one engine and a list of parameters (AEI
//! options), each with a start value, bounds and a final step size
//! `c_end`. Iteration `k` (from 1 to `iterations`) plays
//! `pairs_per_iteration` colour-swapped pairs between engine 1 at
//! θ + c_k·Δ and engine 2 at θ − c_k·Δ, Δ a random ±1 per parameter, and
//! then moves θ by `r_k·c_k·(W − L)·Δ`, W and L engine 1's wins and losses
//! over the iteration (each pair counts 2, 0 or −2: Arimaa has no draws).
//! θ is clamped to each parameter's bounds and stays real; an integer
//! parameter is rounded only for the engine, at random in proportion
//! (`floor(x + U)`), so small steps still move it on average.
//!
//! The schedules are OpenBench's (`OpenBench/spsa_utils.py`), with `N`
//! iterations and `A = a_ratio·N`:
//!
//! - `c_k = c_end·(N/k)^γ`, at least 0.5 for an integer;
//! - `a_k = r_end·c_end²·((A + N)/(A + k))^α`, and `r_k = a_k / c_k²`;
//!
//! so the last iteration steps by `c_end` and `r_end`. The defaults are
//! OpenBench's: α 0.602, γ 0.101, `a_ratio` 0.1, `r_end` 0.002, 8 pairs
//! per iteration.
//!
//! An iteration's point (θ, Δ and the values each side gets) is drawn when
//! its first game is handed out and appended to `spsa-points.jsonl`; when
//! its last game is logged, the step is appended to `spsa-history.jsonl`
//! (θ after it, by name), so a run resumes from the files. The next
//! iteration opens once every game of the open ones is handed out, from θ
//! as it stands then (OpenBench works the same way), so slots don't wait
//! for an iteration's last game.

use std::collections::{BTreeMap, BTreeSet};
use std::path::Path;

use serde::de::DeserializeOwned;
use serde::{Deserialize, Serialize};

use crate::error::TourneyError;
use crate::results::{GameEntry, ResultsLog};
use crate::spec::OptionValue;

/// Each iteration's point, as it was drawn.
pub const POINTS_FILE: &str = "spsa-points.jsonl";
/// θ after each iteration, for plotting.
pub const HISTORY_FILE: &str = "spsa-history.jsonl";

/// `[spsa]` in a spec.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SpsaSpec {
    pub iterations: u32,
    #[serde(default = "default_pairs")]
    pub pairs_per_iteration: u32,
    #[serde(default = "default_alpha")]
    pub alpha: f64,
    #[serde(default = "default_gamma")]
    pub gamma: f64,
    /// `A` as a share of the iterations.
    #[serde(default = "default_a_ratio")]
    pub a_ratio: f64,
    /// Seeds the random directions and roundings. Picked when the test is
    /// made, if not given.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub seed: Option<u32>,
    pub params: Vec<SpsaParam>,
}

/// A parameter (`[[spsa.params]]`).
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SpsaParam {
    /// The AEI option.
    pub name: String,
    pub start: f64,
    pub min: f64,
    pub max: f64,
    /// Sent rounded to an integer.
    #[serde(default, skip_serializing_if = "is_false")]
    pub int: bool,
    /// The perturbation in the last iteration.
    pub c_end: f64,
    /// The learning rate in the last iteration.
    #[serde(default = "default_r_end")]
    pub r_end: f64,
}

fn default_pairs() -> u32 {
    8
}

fn default_alpha() -> f64 {
    0.602
}

fn default_gamma() -> f64 {
    0.101
}

fn default_a_ratio() -> f64 {
    0.1
}

fn default_r_end() -> f64 {
    0.002
}

fn is_false(b: &bool) -> bool {
    !b
}

impl SpsaSpec {
    /// Every pair of the run.
    pub fn pairs(&self) -> u32 {
        self.iterations.saturating_mul(self.pairs_per_iteration)
    }

    /// The iteration (from 1) pair `pair` (from 1) belongs to.
    pub fn iteration_of(&self, pair: u32) -> u32 {
        (pair - 1) / self.pairs_per_iteration + 1
    }

    /// The perturbation of parameter `i` in iteration `k`.
    pub fn c_k(&self, i: usize, k: u32) -> f64 {
        let p = &self.params[i];
        let c = p.c_end * (f64::from(self.iterations) / f64::from(k)).powf(self.gamma);
        if p.int { c.max(0.5) } else { c }
    }

    /// The gain of parameter `i` in iteration `k`.
    pub fn a_k(&self, i: usize, k: u32) -> f64 {
        let p = &self.params[i];
        let n = f64::from(self.iterations);
        let big_a = self.a_ratio * n;
        p.r_end * p.c_end * p.c_end * ((big_a + n) / (big_a + f64::from(k))).powf(self.alpha)
    }

    /// The learning rate of parameter `i` in iteration `k`.
    pub fn r_k(&self, i: usize, k: u32) -> f64 {
        self.a_k(i, k) / self.c_k(i, k).powi(2)
    }

    pub(crate) fn validate(&self) -> Result<(), String> {
        if self.iterations == 0 || self.pairs_per_iteration == 0 {
            return Err("SPSA iterations and pairs_per_iteration must be at least 1".into());
        }
        if self.iterations.checked_mul(self.pairs_per_iteration).is_none() {
            return Err("SPSA iterations × pairs_per_iteration is too large".into());
        }
        let positive = |x: f64| x.is_finite() && x > 0.0;
        if !positive(self.alpha)
            || !positive(self.gamma)
            || !(self.a_ratio.is_finite() && self.a_ratio >= 0.0)
        {
            return Err("SPSA alpha and gamma must be above 0, and a_ratio 0 or more".into());
        }
        if self.params.is_empty() {
            return Err("SPSA needs at least one parameter".into());
        }
        let mut names = BTreeSet::new();
        for p in &self.params {
            if p.name.trim().is_empty() || p.name.contains(char::is_whitespace) {
                return Err(format!("SPSA parameter {:?} needs a name without spaces", p.name));
            }
            if !names.insert(p.name.as_str()) {
                return Err(format!("SPSA parameter {} is listed twice", p.name));
            }
            let finite = [p.start, p.min, p.max].iter().all(|x| x.is_finite());
            if !finite || p.min >= p.max || p.start < p.min || p.start > p.max {
                return Err(format!("SPSA parameter {}: need min < max and start between them", p.name));
            }
            if !positive(p.c_end) || !positive(p.r_end) {
                return Err(format!("SPSA parameter {}: c_end and r_end must be above 0", p.name));
            }
        }
        Ok(())
    }
}

/// An iteration's point: θ as it opened, the direction, and what each
/// side was sent. Per parameter, in the spec's order.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SpsaPoint {
    pub iteration: u32,
    pub theta: Vec<f64>,
    /// Δ: 1 or −1.
    pub flip: Vec<i8>,
    pub c: Vec<f64>,
    pub r: Vec<f64>,
    /// Engine 1's values (θ + cΔ, clamped, integers rounded).
    pub plus: Vec<f64>,
    /// Engine 2's values (θ − cΔ).
    pub minus: Vec<f64>,
}

/// An iteration's step, a line of the history.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SpsaStep {
    pub iteration: u32,
    /// Engine 1's (θ + cΔ) wins and losses.
    pub wins: u32,
    pub losses: u32,
    /// θ after the step, by parameter name.
    pub theta: BTreeMap<String, f64>,
    /// When it ended, in seconds since the Unix epoch.
    pub finished: u64,
}

/// A tuning run's progress, as clients see it.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SpsaInfo {
    pub iterations: u32,
    pub pairs_per_iteration: u32,
    /// Iterations played and stepped.
    pub done: u32,
    /// θ now, in the spec's order.
    pub params: Vec<SpsaValue>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SpsaValue {
    pub name: String,
    pub value: f64,
    pub start: f64,
    pub min: f64,
    pub max: f64,
    pub int: bool,
    /// The perturbation of the next iteration (the last one's once the
    /// run is done): a move in θ smaller than this is within the noise.
    #[serde(default)]
    pub c: f64,
}

/// θ after a step: clamped to the bounds. `result` is W − L for engine 1.
pub fn step(spec: &SpsaSpec, theta: &mut [f64], point: &SpsaPoint, result: i64) {
    for (i, (t, p)) in theta.iter_mut().zip(&spec.params).enumerate() {
        let delta = point.r[i] * point.c[i] * result as f64 * f64::from(point.flip[i]);
        *t = (*t + delta).clamp(p.min, p.max);
    }
}

/// SplitMix64: small, and the same everywhere.
pub(crate) struct Rng(u64);

impl Rng {
    pub(crate) fn new(seed: u32, iteration: u32) -> Rng {
        Rng((u64::from(seed) << 32 | u64::from(iteration)) ^ 0x6a09_e667_f3bc_c909)
    }

    pub(crate) fn next(&mut self) -> u64 {
        self.0 = self.0.wrapping_add(0x9e37_79b9_7f4a_7c15);
        let mut z = self.0;
        z = (z ^ (z >> 30)).wrapping_mul(0xbf58_476d_1ce4_e5b9);
        z = (z ^ (z >> 27)).wrapping_mul(0x94d0_49bb_1331_11eb);
        z ^ (z >> 31)
    }

    /// In [0, 1).
    pub(crate) fn unit(&mut self) -> f64 {
        (self.next() >> 11) as f64 / (1u64 << 53) as f64
    }
}

/// Draws iteration `k`'s point from θ.
pub fn draw_point(spec: &SpsaSpec, theta: &[f64], k: u32) -> SpsaPoint {
    let mut rng = Rng::new(spec.seed.unwrap_or(0), k);
    let n = spec.params.len();
    let flip: Vec<i8> = (0..n).map(|_| if rng.next() & 1 == 1 { 1 } else { -1 }).collect();
    let c: Vec<f64> = (0..n).map(|i| spec.c_k(i, k)).collect();
    let r: Vec<f64> = (0..n).map(|i| spec.r_k(i, k)).collect();
    let mut side = |sign: f64| -> Vec<f64> {
        (0..n)
            .map(|i| {
                let p = &spec.params[i];
                let x = theta[i] + sign * c[i] * f64::from(flip[i]);
                let x = if p.int { (x + rng.unit()).floor() } else { x };
                x.clamp(p.min, p.max)
            })
            .collect()
    };
    let plus = side(1.0);
    let minus = side(-1.0);
    SpsaPoint { iteration: k, theta: theta.to_vec(), flip, c, r, plus, minus }
}

/// A test's tuning state while it runs.
pub(crate) struct SpsaRun {
    spec: SpsaSpec,
    theta: Vec<f64>,
    /// Points drawn, by iteration.
    points: BTreeMap<u32, SpsaPoint>,
    /// Iterations stepped.
    done: BTreeSet<u32>,
    points_log: ResultsLog,
    history_log: ResultsLog,
}

impl SpsaRun {
    /// Picks up where the test's files left off: θ after the last step, and
    /// steps for iterations whose games were all logged before a stop.
    pub(crate) fn open(dir: &Path, spec: &SpsaSpec, entries: &[GameEntry]) -> Result<SpsaRun, TourneyError> {
        let points_path = dir.join(POINTS_FILE);
        let history_path = dir.join(HISTORY_FILE);
        let history: Vec<SpsaStep> = read_lines(&history_path)?;
        let mut theta: Vec<f64> = spec.params.iter().map(|p| p.start).collect();
        if let Some(last) = history.last() {
            for (t, p) in theta.iter_mut().zip(&spec.params) {
                if let Some(v) = last.theta.get(&p.name) {
                    *t = *v;
                }
            }
        }
        let mut points = BTreeMap::new();
        for p in read_lines::<SpsaPoint>(&points_path)? {
            if p.flip.len() != spec.params.len() {
                return Err(TourneyError::Parse {
                    path: points_path,
                    message: format!("iteration {} has the wrong number of parameters", p.iteration),
                });
            }
            points.entry(p.iteration).or_insert(p);
        }
        let mut run = SpsaRun {
            spec: spec.clone(),
            theta,
            points,
            done: history.iter().map(|s| s.iteration).collect(),
            points_log: ResultsLog::open(&points_path)?,
            history_log: ResultsLog::open(&history_path)?,
        };
        let open: Vec<u32> = run.points.keys().copied().filter(|k| !run.done.contains(k)).collect();
        for k in open {
            run.step_if_done(entries, k, 0)?;
        }
        Ok(run)
    }

    /// The point of the iteration pair `pair` is in, drawn now if it's the
    /// first game handed out.
    pub(crate) fn open_point(&mut self, pair: u32) -> Result<(), TourneyError> {
        let k = self.spec.iteration_of(pair);
        if !self.points.contains_key(&k) {
            let point = draw_point(&self.spec, &self.theta, k);
            self.points_log.append_line(&point)?;
            self.points.insert(k, point);
        }
        Ok(())
    }

    /// The options engine `engine` plays pair `pair` with.
    pub(crate) fn options(&self, pair: u32, engine: usize) -> Vec<(String, OptionValue)> {
        let Some(point) = self.points.get(&self.spec.iteration_of(pair)) else { return Vec::new() };
        let values = if engine == 0 { &point.plus } else { &point.minus };
        self.spec
            .params
            .iter()
            .zip(values)
            .map(|(p, v)| {
                let value = if p.int { OptionValue::Int(v.round() as i64) } else { OptionValue::Float(*v) };
                (p.name.clone(), value)
            })
            .collect()
    }

    /// After a game of pair `pair` is logged: steps θ if it was its
    /// iteration's last.
    pub(crate) fn game_logged(
        &mut self,
        entries: &[GameEntry],
        pair: u32,
        now: u64,
    ) -> Result<(), TourneyError> {
        self.step_if_done(entries, self.spec.iteration_of(pair), now)
    }

    fn step_if_done(&mut self, entries: &[GameEntry], k: u32, now: u64) -> Result<(), TourneyError> {
        if self.done.contains(&k) {
            return Ok(());
        }
        let Some(point) = self.points.get(&k) else { return Ok(()) };
        let games: Vec<&GameEntry> = entries.iter().filter(|e| self.spec.iteration_of(e.pair) == k).collect();
        if games.len() < 2 * self.spec.pairs_per_iteration as usize {
            return Ok(());
        }
        let wins = games.iter().filter(|e| e.winner == 0).count() as u32;
        let losses = games.len() as u32 - wins;
        step(&self.spec, &mut self.theta, point, i64::from(wins) - i64::from(losses));
        let finished = if now == 0 { games.iter().map(|e| e.finished).max().unwrap_or(0) } else { now };
        let theta = self.spec.params.iter().map(|p| p.name.clone()).zip(self.theta.iter().copied()).collect();
        self.history_log.append_line(&SpsaStep { iteration: k, wins, losses, theta, finished })?;
        self.done.insert(k);
        Ok(())
    }

    pub(crate) fn info(&self) -> SpsaInfo {
        info(&self.spec, self.done.len() as u32, &self.theta)
    }
}

fn info(spec: &SpsaSpec, done: u32, theta: &[f64]) -> SpsaInfo {
    let next = (done + 1).min(spec.iterations);
    SpsaInfo {
        iterations: spec.iterations,
        pairs_per_iteration: spec.pairs_per_iteration,
        done,
        params: spec
            .params
            .iter()
            .zip(theta)
            .enumerate()
            .map(|(i, (p, v))| SpsaValue {
                name: p.name.clone(),
                value: *v,
                start: p.start,
                min: p.min,
                max: p.max,
                int: p.int,
                c: spec.c_k(i, next),
            })
            .collect(),
    }
}

/// A run's progress from its history (`spsa-history.jsonl`, already read).
pub(crate) fn info_from_history(
    spec: &SpsaSpec,
    history: &str,
    path: &Path,
) -> Result<SpsaInfo, TourneyError> {
    let steps: Vec<SpsaStep> = parse_lines(history, path)?;
    let theta: Vec<f64> = spec
        .params
        .iter()
        .map(|p| steps.last().and_then(|s| s.theta.get(&p.name)).copied().unwrap_or(p.start))
        .collect();
    let done = steps.iter().map(|s| s.iteration).collect::<BTreeSet<_>>().len() as u32;
    Ok(info(spec, done, &theta))
}

/// A test directory's tuning progress, from its history.
pub fn read_progress(dir: &Path, spec: &SpsaSpec) -> Result<SpsaInfo, TourneyError> {
    let path = dir.join(HISTORY_FILE);
    let text = match std::fs::read_to_string(&path) {
        Ok(t) => t,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => String::new(),
        Err(e) => return Err(TourneyError::io(&path)(e)),
    };
    info_from_history(spec, &text, &path)
}

/// A test directory's SPSA history (`spsa-history.jsonl`), in the order
/// the steps were taken; empty if it has none.
pub fn read_history(dir: &Path) -> Result<Vec<SpsaStep>, TourneyError> {
    read_lines(&dir.join(HISTORY_FILE))
}

fn read_lines<T: DeserializeOwned>(path: &Path) -> Result<Vec<T>, TourneyError> {
    match std::fs::read_to_string(path) {
        Ok(t) => parse_lines(&t, path),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(Vec::new()),
        Err(e) => Err(TourneyError::io(path)(e)),
    }
}

/// JSON lines, leaving out a last line cut short (as the results log).
fn parse_lines<T: DeserializeOwned>(text: &str, path: &Path) -> Result<Vec<T>, TourneyError> {
    let complete = text.ends_with('\n');
    let lines: Vec<&str> = text.lines().collect();
    let mut out = Vec::new();
    for (i, line) in lines.iter().enumerate() {
        if line.trim().is_empty() {
            continue;
        }
        match serde_json::from_str(line) {
            Ok(v) => out.push(v),
            Err(_) if i + 1 == lines.len() && !complete => {}
            Err(e) => {
                return Err(TourneyError::Parse {
                    path: path.into(),
                    message: format!("line {}: {e}", i + 1),
                });
            }
        }
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::results::entry;

    fn spec(int: bool) -> SpsaSpec {
        SpsaSpec {
            iterations: 100,
            pairs_per_iteration: 2,
            alpha: 0.602,
            gamma: 0.101,
            a_ratio: 0.1,
            seed: Some(7),
            params: vec![SpsaParam {
                name: "x".into(),
                start: 50.0,
                min: 0.0,
                max: 100.0,
                int,
                c_end: 4.0,
                r_end: 0.002,
            }],
        }
    }

    fn close(a: f64, b: f64) -> bool {
        (a - b).abs() < 1e-9 * b.abs().max(1.0)
    }

    #[test]
    fn schedules_end_at_c_end_and_r_end() {
        let s = spec(false);
        assert!(close(s.c_k(0, 100), 4.0));
        assert!(close(s.r_k(0, 100), 0.002));
        assert!(close(s.a_k(0, 100), 0.002 * 16.0));
        // The first iteration: c_end·N^γ, and a_end·((A + N)/(A + 1))^α.
        assert!(close(s.c_k(0, 1), 4.0 * 100f64.powf(0.101)));
        assert!(close(s.a_k(0, 1), 0.032 * (110.0f64 / 11.0).powf(0.602)));
        // Both shrink as the run goes on.
        assert!(s.c_k(0, 10) > s.c_k(0, 50) && s.a_k(0, 10) > s.a_k(0, 50));
    }

    #[test]
    fn integers_perturb_by_at_least_half() {
        let mut s = spec(true);
        s.params[0].c_end = 0.1;
        assert_eq!(s.c_k(0, 100), 0.5);
        assert!(close(s.r_k(0, 100), 0.002 * 0.01 / 0.25));
    }

    #[test]
    fn steps_along_the_result_and_clamps() {
        let s = spec(false);
        let point = draw_point(&s, &[50.0], 100);
        let d = f64::from(point.flip[0]);
        let mut theta = vec![50.0];
        step(&s, &mut theta, &point, 4);
        // r·c·(W − L)·Δ = 0.002·4·4·Δ.
        assert!(close(theta[0], 50.0 + 0.032 * d));
        step(&s, &mut theta, &point, -4);
        assert!(close(theta[0], 50.0));
        let mut far = vec![99.99];
        step(&s, &mut far, &SpsaPoint { flip: vec![1], ..point.clone() }, 10_000);
        assert_eq!(far, [100.0]);
    }

    #[test]
    fn points_are_reproducible_and_symmetric() {
        let s = spec(false);
        let a = draw_point(&s, &[50.0], 3);
        assert_eq!(a, draw_point(&s, &[50.0], 3));
        let d = f64::from(a.flip[0]);
        assert!(close(a.plus[0], 50.0 + a.c[0] * d) && close(a.minus[0], 50.0 - a.c[0] * d));
        // Directions vary by iteration.
        let flips: BTreeSet<i8> = (1..20).map(|k| draw_point(&s, &[50.0], k).flip[0]).collect();
        assert_eq!(flips.len(), 2);
        // Clamped at the bounds.
        let edge = draw_point(&s, &[100.0], 3);
        assert!(edge.plus[0] <= 100.0 && edge.minus[0] <= 100.0);
    }

    #[test]
    fn integers_round_at_random_without_bias() {
        let mut s = spec(true);
        s.params[0].c_end = 0.3;
        let points: Vec<SpsaPoint> = (1..=2000).map(|k| draw_point(&s, &[50.25], k)).collect();
        assert!(points.iter().all(|p| p.plus[0].fract() == 0.0 && p.minus[0].fract() == 0.0));
        let mean = points.iter().map(|p| p.plus[0] + p.minus[0]).sum::<f64>() / (2.0 * points.len() as f64);
        assert!((mean - 50.25).abs() < 0.05, "mean {mean}");
    }

    #[test]
    fn steps_once_an_iteration_is_played_and_resumes_from_the_files() {
        let dir = tempfile::tempdir().unwrap();
        let s = spec(false);
        let mut run = SpsaRun::open(dir.path(), &s, &[]).unwrap();
        run.open_point(1).unwrap();
        let point = run.points[&1].clone();
        assert_eq!(run.options(2, 0), [("x".to_string(), OptionValue::Float(point.plus[0]))]);
        assert_eq!(run.options(2, 1), [("x".to_string(), OptionValue::Float(point.minus[0]))]);
        // Pairs 1 and 2 are iteration 1: engine 1 wins three of its four games.
        let mut entries = vec![entry(1, 1, 0), entry(1, 2, 0), entry(2, 1, 1)];
        for e in 1..=entries.len() {
            run.game_logged(&entries[..e], entries[e - 1].pair, 5).unwrap();
        }
        assert_eq!(run.info().done, 0);
        // Stopped here; the last game is logged before the step is written.
        entries.push(entry(2, 2, 0));
        drop(run);
        let mut run = SpsaRun::open(dir.path(), &s, &entries).unwrap();
        let expected = 50.0 + point.r[0] * point.c[0] * 2.0 * f64::from(point.flip[0]);
        let info = run.info();
        assert_eq!(info.done, 1);
        assert!(close(info.params[0].value, expected));
        run.game_logged(&entries, 2, 6).unwrap();
        // The next iteration opens from the new θ.
        run.open_point(3).unwrap();
        assert!(close(run.points[&2].theta[0], expected));

        let again = read_progress(dir.path(), &s).unwrap();
        assert_eq!(again, run.info());
        let history: Vec<SpsaStep> = read_lines(&dir.path().join(HISTORY_FILE)).unwrap();
        assert_eq!(history.len(), 1);
        assert_eq!((history[0].wins, history[0].losses), (3, 1));
    }

    #[test]
    fn values_read_back_exactly() {
        // A resumed run carries on from what the files say, so θ, c and r
        // must read back as written (serde_json's `float_roundtrip`).
        let mut rng = Rng::new(1, 1);
        for _ in 0..10_000 {
            let x = rng.unit() * 10f64.powi((rng.next() % 9) as i32 - 4);
            let back: f64 = serde_json::from_str(&serde_json::to_string(&x).unwrap()).unwrap();
            assert_eq!(back, x);
        }
    }
}
