//! Pools: a standing tournament of many engines (`[pool]` in the spec).
//!
//! Every engine plays the others in colour-swapped pairs, and engines can
//! be added or retired while it runs, so it can keep testing each new
//! version against the rest for as long as it's left going. A pool has no
//! SPRT; without `max_pairs` it plays until stopped.
//!
//! Ratings come from all the games at once: the Bradley–Terry model
//! (logistic Elo; Arimaa has no draws) at its most likely ratings under a
//! wide normal prior, so an engine that has won or lost every game still
//! gets a finite rating, with their covariance from the inverse Hessian.
//! They're shown relative to the anchor (the spec's `anchor`, else the
//! first engine with games), and each one's error is that of its
//! difference from the anchor.
//!
//! Each new pair's matchup is chosen by [`choose`]:
//! - Coverage first: while a matchup of active engines has fewer than
//!   `min_pairs` pairs (pairs in play count), one of the least played is
//!   drawn. A new engine's matchups all start at none, so it catches up
//!   before anything else.
//! - Then by information: each matchup is weighted by how much one more
//!   pair would shrink the variance of its rating difference,
//!   `v² I / (1 + v I)` with `v` that variance and `I = 2 p (1 − p)` a
//!   pair's Fisher information, counting the games in play as played. To
//!   that is added a floor ([`FLOOR`] of the mean), so far-apart matchups
//!   still get the odd pair and the crosstable fills in, and the mean
//!   again for a matchup whose result looks out of line ([`SUSPECT_Z`])
//!   but isn't yet an outlier. The matchup is drawn in proportion to its
//!   weight, so slots picking at once spread out.
//!
//! A matchup is an outlier once it has [`OUTLIER_MIN_GAMES`] games and its
//! result is `outlier_z` standard deviations or more from what the
//! ratings predict: `z = (wins − n p) / √(n p (1 − p))`.

use std::collections::{HashMap, HashSet};
use std::f64::consts::LN_10;
use std::path::PathBuf;

use serde::{Deserialize, Serialize};

use crate::error::TourneyError;
use crate::results::GameEntry;
use crate::spec::{EngineSpec, TestSpec};
use crate::spsa::Rng;
use crate::testdir::{TestDir, slug};

/// The prior's standard deviation, in Elo: wide enough that a few hundred
/// games anywhere decide a rating, narrow enough that 2–0 isn't infinite.
const PRIOR_SD_ELO: f64 = 600.0;

/// A matchup this many standard deviations out of line gets more games,
/// until it's confirmed (`outlier_z`) or falls back in line.
pub const SUSPECT_Z: f64 = 2.0;

/// The fewest games for a matchup to count as an outlier.
pub const OUTLIER_MIN_GAMES: u32 = 6;

/// Every matchup's weight has this share of the mean added.
pub const FLOOR: f64 = 0.1;

const ELO_PER_NAT: f64 = 400.0 / LN_10;

/// `[pool]` in a spec.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PoolSpec {
    /// Pairs every matchup of active engines plays before the scheduler
    /// weighs them by what they'd tell.
    #[serde(default = "default_min_pairs")]
    pub min_pairs: u32,
    /// How far out of line (in standard deviations) a matchup's result is
    /// to be an outlier.
    #[serde(default = "default_outlier_z")]
    pub outlier_z: f64,
    /// The engine rated 0 (by name); by default the first with games.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub anchor: Option<String>,
    /// For drawing matchups; set when the test is made.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub seed: Option<u32>,
}

impl Default for PoolSpec {
    fn default() -> Self {
        PoolSpec { min_pairs: default_min_pairs(), outlier_z: default_outlier_z(), anchor: None, seed: None }
    }
}

fn default_min_pairs() -> u32 {
    2
}

fn default_outlier_z() -> f64 {
    3.0
}

impl PoolSpec {
    pub(crate) fn validate(&self, engines: &[EngineSpec]) -> Result<(), String> {
        if self.min_pairs == 0 {
            return Err("the pool's min_pairs must be at least 1".into());
        }
        if !(self.outlier_z > 0.0 && self.outlier_z.is_finite()) {
            return Err("the pool's outlier_z must be above 0".into());
        }
        if let Some(a) = &self.anchor
            && !engines.iter().any(|e| e.name == *a)
        {
            return Err(format!("the pool's anchor {a:?} isn't one of its engines"));
        }
        Ok(())
    }
}

/// A pool's engines need different names, and different snapshot
/// directories.
pub(crate) fn check_names(engines: &[EngineSpec]) -> Result<(), String> {
    for (i, e) in engines.iter().enumerate() {
        for f in &engines[..i] {
            if f.name == e.name {
                return Err(format!("two engines are named {:?}", e.name));
            }
            if slug(&f.name) == slug(&e.name) {
                return Err(format!(
                    "{:?} and {:?} are too alike to tell apart in file names",
                    f.name, e.name
                ));
            }
        }
    }
    Ok(())
}

/// A pool's standings, from its games.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PoolInfo {
    /// In the spec's order: [`GameEntry::engines`] indexes them.
    pub engines: Vec<PoolEngine>,
    /// Engine indexes, the best rated first; those without games last.
    pub ranking: Vec<usize>,
    /// The engine rated 0, if any has games.
    pub anchor: Option<usize>,
    /// Every matchup with games, `a < b`.
    pub matchups: Vec<Matchup>,
    pub min_pairs: u32,
    pub outlier_z: f64,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PoolEngine {
    pub name: String,
    /// Plays no new pairs; its games still count.
    pub retired: bool,
    pub games: u32,
    pub wins: u32,
    /// Elo relative to the anchor; None before its first game.
    pub elo: Option<f64>,
    /// Half the 95% interval of `elo` (0 for the anchor).
    pub error: Option<f64>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Matchup {
    pub a: usize,
    pub b: usize,
    /// Games each won (a, b).
    pub wins: [u32; 2],
    /// Complete pairs.
    pub pairs: u32,
    /// `a`'s score the ratings predict (0 to 1).
    pub expected: f64,
    /// How far `a`'s wins are from that, in standard deviations.
    pub z: f64,
    pub outlier: bool,
}

/// The engines of a game's pair (indexes into the spec's engines). Logs
/// from before pools have none: engines 1 and 2.
pub(crate) fn engines_of(entry: &GameEntry) -> [usize; 2] {
    entry.engines.unwrap_or([0, 1])
}

/// Games won, by engine and opponent: `wins[i][j]` is i's wins over j.
fn wins(n: usize, entries: &[GameEntry]) -> Vec<Vec<u32>> {
    let mut wins = vec![vec![0; n]; n];
    for e in entries {
        let sides = engines_of(e);
        if sides.iter().all(|&i| i < n) {
            wins[sides[e.winner]][sides[1 - e.winner]] += 1;
        }
    }
    wins
}

/// A pool's standings from its engines and games.
pub fn pool_info(spec: &PoolSpec, engines: &[EngineSpec], entries: &[GameEntry]) -> PoolInfo {
    let n = engines.len();
    let wins = wins(n, entries);
    let fit = fit(&wins, None);
    let games = |i: usize| (0..n).map(|j| wins[i][j] + wins[j][i]).sum::<u32>();
    let anchor = spec
        .anchor
        .as_ref()
        .and_then(|a| engines.iter().position(|e| e.name == *a))
        .filter(|&a| games(a) > 0)
        .or_else(|| (0..n).find(|&i| games(i) > 0));
    let list: Vec<PoolEngine> = (0..n)
        .map(|i| {
            let rated = anchor.filter(|_| games(i) > 0);
            PoolEngine {
                name: engines[i].name.clone(),
                retired: engines[i].retired,
                games: games(i),
                wins: wins[i].iter().sum(),
                elo: rated.map(|a| (fit.r[i] - fit.r[a]) * ELO_PER_NAT),
                error: rated.map(|a| 1.96 * fit.diff_var(i, a).max(0.0).sqrt() * ELO_PER_NAT),
            }
        })
        .collect();
    let mut ranking: Vec<usize> = (0..n).collect();
    ranking.sort_by(|&i, &j| match (list[i].elo, list[j].elo) {
        (Some(a), Some(b)) => b.total_cmp(&a).then(i.cmp(&j)),
        (a, b) => b.is_some().cmp(&a.is_some()).then(i.cmp(&j)),
    });
    let mut pairs: HashMap<u32, Vec<&GameEntry>> = HashMap::new();
    for e in entries {
        pairs.entry(e.pair).or_default().push(e);
    }
    let mut complete = vec![vec![0u32; n]; n];
    for games in pairs.values().filter(|g| g.len() == 2) {
        let [a, b] = engines_of(games[0]);
        if a < n && b < n {
            complete[a.min(b)][a.max(b)] += 1;
        }
    }
    let mut matchups = Vec::new();
    for a in 0..n {
        for b in a + 1..n {
            let played = wins[a][b] + wins[b][a];
            if played == 0 {
                continue;
            }
            let expected = fit.p(a, b);
            let z = z_score(wins[a][b], played, expected);
            matchups.push(Matchup {
                a,
                b,
                wins: [wins[a][b], wins[b][a]],
                pairs: complete[a][b],
                expected,
                z,
                outlier: played >= OUTLIER_MIN_GAMES && z.abs() >= spec.outlier_z,
            });
        }
    }
    PoolInfo {
        engines: list,
        ranking,
        anchor,
        matchups,
        min_pairs: spec.min_pairs,
        outlier_z: spec.outlier_z,
    }
}

/// How far `wins` of `games` is from what a chance `p` per game predicts,
/// in standard deviations.
fn z_score(wins: u32, games: u32, p: f64) -> f64 {
    let n = f64::from(games);
    let sd = (n * p * (1.0 - p)).sqrt();
    if games == 0 || sd < 1e-9 { 0.0 } else { (f64::from(wins) - n * p) / sd }
}

/// Ratings in natural units (a difference `d` means `1 / (1 + e^-d)`)
/// with their covariance.
struct Fit {
    r: Vec<f64>,
    cov: Vec<Vec<f64>>,
}

impl Fit {
    /// The chance `i` beats `j`.
    fn p(&self, i: usize, j: usize) -> f64 {
        sigmoid(self.r[i] - self.r[j])
    }

    /// The variance of `r[i] − r[j]`.
    fn diff_var(&self, i: usize, j: usize) -> f64 {
        self.cov[i][i] + self.cov[j][j] - 2.0 * self.cov[i][j]
    }
}

fn sigmoid(x: f64) -> f64 {
    1.0 / (1.0 + (-x).exp())
}

/// `ln σ(x)`, without overflow.
fn ln_sigmoid(x: f64) -> f64 {
    -(x.max(0.0) - x + (-x.abs()).exp().ln_1p())
}

/// The most likely ratings for `wins` under the prior, and their
/// covariance, which also counts `pending[i][j]` games between i and j as
/// played (at the chance the ratings give): what's known once the games
/// in play are in.
fn fit(wins: &[Vec<u32>], pending: Option<&[Vec<u32>]>) -> Fit {
    let n = wins.len();
    let tau = (ELO_PER_NAT / PRIOR_SD_ELO).powi(2);
    let log_post = |r: &[f64]| -> f64 {
        let mut l = -0.5 * tau * r.iter().map(|x| x * x).sum::<f64>();
        for i in 0..n {
            for j in 0..n {
                if wins[i][j] > 0 {
                    l += f64::from(wins[i][j]) * ln_sigmoid(r[i] - r[j]);
                }
            }
        }
        l
    };
    // The negative Hessian, adding `extra` games between each pair.
    let hessian = |r: &[f64], extra: Option<&[Vec<u32>]>| -> Vec<Vec<f64>> {
        let mut h = vec![vec![0.0; n]; n];
        for (i, row) in h.iter_mut().enumerate() {
            row[i] = tau;
        }
        for i in 0..n {
            for j in i + 1..n {
                let mut games = f64::from(wins[i][j] + wins[j][i]);
                if let Some(x) = extra {
                    games += f64::from(x[i][j] + x[j][i]);
                }
                if games > 0.0 {
                    let p = sigmoid(r[i] - r[j]);
                    let w = games * p * (1.0 - p);
                    h[i][i] += w;
                    h[j][j] += w;
                    h[i][j] -= w;
                    h[j][i] -= w;
                }
            }
        }
        h
    };
    let mut r = vec![0.0; n];
    let mut current = log_post(&r);
    for _ in 0..100 {
        let mut g: Vec<f64> = r.iter().map(|x| -tau * x).collect();
        for i in 0..n {
            for j in 0..n {
                if wins[i][j] > 0 {
                    // Each of i's wins over j pulls them apart by 1 − p.
                    let d = f64::from(wins[i][j]) * (1.0 - sigmoid(r[i] - r[j]));
                    g[i] += d;
                    g[j] -= d;
                }
            }
        }
        let step = solve(&hessian(&r, None), &g);
        // Newton's step, halved until it doesn't go downhill.
        let mut scale = 1.0;
        let mut next: Vec<f64>;
        loop {
            next = r.iter().zip(&step).map(|(x, s)| x + scale * s).collect();
            let value = log_post(&next);
            if value >= current - 1e-12 || scale < 1e-6 {
                current = value;
                break;
            }
            scale /= 2.0;
        }
        let moved = step.iter().map(|s| (scale * s).abs()).fold(0.0, f64::max);
        r = next;
        if moved < 1e-10 {
            break;
        }
    }
    let cov = inverse(&hessian(&r, pending));
    Fit { r, cov }
}

/// `L` with `L Lᵀ = a`, for a symmetric positive definite `a`.
fn cholesky(a: &[Vec<f64>]) -> Vec<Vec<f64>> {
    let n = a.len();
    let mut l = vec![vec![0.0; n]; n];
    for i in 0..n {
        for j in 0..=i {
            let s: f64 = (0..j).map(|k| l[i][k] * l[j][k]).sum();
            if i == j {
                l[i][i] = (a[i][i] - s).max(1e-300).sqrt();
            } else {
                l[i][j] = (a[i][j] - s) / l[j][j];
            }
        }
    }
    l
}

/// Solves `a x = b` for a symmetric positive definite `a`.
fn solve(a: &[Vec<f64>], b: &[f64]) -> Vec<f64> {
    solve_with(&cholesky(a), b)
}

fn solve_with(l: &[Vec<f64>], b: &[f64]) -> Vec<f64> {
    let n = b.len();
    let mut y = vec![0.0; n];
    for i in 0..n {
        y[i] = (b[i] - (0..i).map(|k| l[i][k] * y[k]).sum::<f64>()) / l[i][i];
    }
    let mut x = vec![0.0; n];
    for i in (0..n).rev() {
        x[i] = (y[i] - (i + 1..n).map(|k| l[k][i] * x[k]).sum::<f64>()) / l[i][i];
    }
    x
}

fn inverse(a: &[Vec<f64>]) -> Vec<Vec<f64>> {
    let n = a.len();
    let l = cholesky(a);
    let cols: Vec<Vec<f64>> = (0..n)
        .map(|j| {
            let e: Vec<f64> = (0..n).map(|i| f64::from(u8::from(i == j))).collect();
            solve_with(&l, &e)
        })
        .collect();
    (0..n).map(|i| (0..n).map(|j| cols[j][i]).collect()).collect()
}

/// What the scheduler knows about a pool, by engine and opponent.
pub(crate) struct Tally {
    /// Games won.
    pub wins: Vec<Vec<u32>>,
    /// Pairs started (played or in play), counted both ways.
    pub pairs: Vec<Vec<u32>>,
    /// Games in play or yet to play in pairs already started.
    pub pending: Vec<Vec<u32>>,
}

/// Chooses a new pair's matchup, engine 1 then engine 2, among the engines
/// not retired (see the module's notes). None with fewer than two.
pub(crate) fn choose(spec: &PoolSpec, retired: &[bool], tally: &Tally, rng: &mut Rng) -> Option<[usize; 2]> {
    let active: Vec<usize> = (0..retired.len()).filter(|&i| !retired[i]).collect();
    let mut matchups = Vec::new();
    for (k, &a) in active.iter().enumerate() {
        for &b in &active[k + 1..] {
            matchups.push((a, b));
        }
    }
    let least = matchups.iter().map(|&(a, b)| tally.pairs[a][b]).min()?;
    let (a, b) = if least < spec.min_pairs {
        let behind: Vec<_> = matchups.iter().filter(|&&(a, b)| tally.pairs[a][b] == least).collect();
        *behind[(rng.next() % behind.len() as u64) as usize]
    } else {
        let weights = weights(spec, &matchups, tally);
        let total: f64 = weights.iter().sum();
        let mut x = rng.unit() * total;
        let mut chosen = matchups[matchups.len() - 1];
        for (m, w) in matchups.iter().zip(&weights) {
            if x < *w {
                chosen = *m;
                break;
            }
            x -= w;
        }
        chosen
    };
    Some(if rng.next() & 1 == 0 { [a, b] } else { [b, a] })
}

/// Each matchup's weight for the next pair.
fn weights(spec: &PoolSpec, matchups: &[(usize, usize)], tally: &Tally) -> Vec<f64> {
    let fit = fit(&tally.wins, Some(&tally.pending));
    let gains: Vec<f64> = matchups
        .iter()
        .map(|&(a, b)| {
            let p = fit.p(a, b);
            let v = fit.diff_var(a, b).max(0.0);
            let info = 2.0 * p * (1.0 - p);
            v * v * info / (1.0 + v * info)
        })
        .collect();
    let mean = gains.iter().sum::<f64>() / gains.len() as f64;
    matchups
        .iter()
        .zip(&gains)
        .map(|(&(a, b), gain)| {
            let z = z_score(tally.wins[a][b], tally.wins[a][b] + tally.wins[b][a], fit.p(a, b)).abs();
            let suspect = (SUSPECT_Z..spec.outlier_z).contains(&z);
            gain + FLOOR * mean + if suspect { mean } else { 0.0 }
        })
        .collect()
}

/// What to do with a game about to be handed out.
#[derive(Debug, PartialEq, Eq)]
pub(crate) enum Prepared {
    Play,
    /// Its pair has a retired engine and its other game is played or in
    /// play: drop it.
    Skip,
    /// Fewer than two engines are active.
    Wait,
}

/// A pool's engines and matchups while it runs. Its engines are the
/// spec's, plus any added since, written back to `test.toml` as they
/// change.
pub(crate) struct PoolRun {
    spec: TestSpec,
    path: PathBuf,
    /// Each pair's engines (1, 2), once chosen.
    matchups: HashMap<u32, [usize; 2]>,
}

impl PoolRun {
    pub(crate) fn open(dir: &TestDir, entries: &[GameEntry]) -> PoolRun {
        let matchups = entries.iter().map(|e| (e.pair, engines_of(e))).collect();
        PoolRun { spec: dir.spec.clone(), path: dir.path.clone(), matchups }
    }

    fn pool(&self) -> &PoolSpec {
        self.spec.pool.as_ref().expect("a pool's spec has [pool]")
    }

    pub(crate) fn engines(&self) -> &[EngineSpec] {
        &self.spec.engines
    }

    /// Pair `pair`'s engines, once chosen.
    pub(crate) fn sides(&self, pair: u32) -> Option<[usize; 2]> {
        self.matchups.get(&pair).copied()
    }

    /// Gets game `game` of pair `pair` ready to go out: chooses its
    /// pair's matchup if it has none yet (or again, if an engine has been
    /// retired since and neither game has been played).
    pub(crate) fn prepare(
        &mut self,
        (pair, game): (u32, u8),
        entries: &[GameEntry],
        done: &HashSet<(u32, u8)>,
        running: &[(u32, u8)],
    ) -> Prepared {
        let other = (pair, 3 - game);
        if let Some(m) = self.matchups.get(&pair) {
            if m.iter().all(|&i| !self.spec.engines[i].retired) {
                return Prepared::Play;
            }
            if done.contains(&other) || running.contains(&other) {
                return Prepared::Skip;
            }
            self.matchups.remove(&pair);
        }
        let tally = self.tally(entries, done);
        let retired: Vec<bool> = self.spec.engines.iter().map(|e| e.retired).collect();
        let mut rng = Rng::new(self.pool().seed.unwrap_or(0), pair);
        match choose(self.pool(), &retired, &tally, &mut rng) {
            Some(m) => {
                self.matchups.insert(pair, m);
                Prepared::Play
            }
            None => Prepared::Wait,
        }
    }

    fn tally(&self, entries: &[GameEntry], done: &HashSet<(u32, u8)>) -> Tally {
        let n = self.spec.engines.len();
        let mut pairs = vec![vec![0; n]; n];
        let mut pending = vec![vec![0; n]; n];
        for (&pair, &[a, b]) in &self.matchups {
            pairs[a][b] += 1;
            pairs[b][a] += 1;
            let left = (1..=2).filter(|&g| !done.contains(&(pair, g))).count() as u32;
            pending[a][b] += left;
            pending[b][a] += left;
        }
        Tally { wins: wins(n, entries), pairs, pending }
    }

    pub(crate) fn info(&self, entries: &[GameEntry]) -> PoolInfo {
        pool_info(self.pool(), &self.spec.engines, entries)
    }

    /// Adds an engine, already snapshotted into the test's directory.
    pub(crate) fn add_engine(&mut self, engine: EngineSpec) -> Result<(), TourneyError> {
        let mut engines = self.spec.engines.clone();
        engines.push(engine);
        check_names(&engines).map_err(TourneyError::Refused)?;
        self.save_with(engines)
    }

    /// The index of the engine named `name`.
    pub(crate) fn index_of(&self, name: &str) -> Option<usize> {
        self.spec.engines.iter().position(|e| e.name == name)
    }

    /// Retires engine `index` (no new pairs), or brings it back.
    pub(crate) fn set_retired(&mut self, index: usize, retired: bool) -> Result<(), TourneyError> {
        let mut engines = self.spec.engines.clone();
        engines[index].retired = retired;
        self.save_with(engines)
    }

    /// How many engines can play.
    pub(crate) fn active(&self) -> usize {
        self.spec.engines.iter().filter(|e| !e.retired).count()
    }

    fn save_with(&mut self, engines: Vec<EngineSpec>) -> Result<(), TourneyError> {
        let spec = TestSpec { engines, ..self.spec.clone() };
        TestDir::write_spec(&self.path, &spec)?;
        self.spec = spec;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::results::entry;

    fn game(pair: u32, game: u8, engines: [usize; 2], winner: usize) -> GameEntry {
        GameEntry { engines: Some(engines), ..entry(pair, game, winner) }
    }

    fn engines(names: &[&str]) -> Vec<EngineSpec> {
        names
            .iter()
            .map(|n| {
                TestSpec::parse(&format!(
                    "name = \"t\"\n[pool]\n[[engines]]\nname = \"{n}\"\nprogram = \"p\"\n\
                     [[engines]]\nname = \"other\"\nprogram = \"p\""
                ))
                .unwrap()
                .engines[0]
                    .clone()
            })
            .collect()
    }

    /// `wins[i][j]` games i won over j, logged in pairs.
    fn games(wins: &[Vec<u32>]) -> Vec<GameEntry> {
        let mut out = Vec::new();
        let mut pair = 0;
        for (i, row) in wins.iter().enumerate() {
            for (j, &won) in row.iter().enumerate() {
                for _ in 0..won {
                    pair += 1;
                    out.push(game(pair, 1, [i, j], 0));
                }
            }
        }
        out
    }

    #[test]
    fn two_engines_rate_as_their_score_says() {
        // 75% is about 191 Elo; the prior pulls it in a little.
        let info =
            pool_info(&PoolSpec::default(), &engines(&["a", "b"]), &games(&[vec![0, 300], vec![100, 0]]));
        let [a, b] = [&info.engines[0], &info.engines[1]];
        assert_eq!(info.anchor, Some(0));
        assert_eq!((a.elo, a.error), (Some(0.0), Some(0.0)));
        let elo = b.elo.unwrap();
        assert!((-191.0..-185.0).contains(&elo), "{elo}");
        // 1.96 standard errors: √(0.75 × 0.25 / 400) of score is 20 Elo.
        let error = b.error.unwrap();
        assert!((36.0..42.0).contains(&error), "{error}");
        assert_eq!(info.ranking, [0, 1]);
        assert_eq!((b.games, b.wins), (400, 100));
    }

    #[test]
    fn a_clean_sweep_still_has_a_rating() {
        let info = pool_info(&PoolSpec::default(), &engines(&["a", "b"]), &games(&[vec![0, 4], vec![0, 0]]));
        let elo = info.engines[1].elo.unwrap();
        assert!(elo.is_finite() && elo < -100.0, "{elo}");
    }

    #[test]
    fn ratings_are_transitive_and_outliers_stand_out() {
        // a > b > c by 100 Elo steps (64%), but c beats a every time.
        let wins = vec![vec![0, 64, 0], vec![36, 0, 64], vec![40, 36, 0]];
        let info = pool_info(&PoolSpec::default(), &engines(&["a", "b", "c"]), &games(&wins));
        assert_eq!(info.matchups.len(), 3);
        let ca = info.matchups.iter().find(|m| (m.a, m.b) == (0, 2)).unwrap();
        assert_eq!(ca.wins, [0, 40]);
        assert!(ca.outlier && ca.z < -3.0, "{ca:?}");
        assert!(info.matchups.iter().filter(|m| m.outlier).count() >= 1);
        let ab = info.matchups.iter().find(|m| (m.a, m.b) == (0, 1)).unwrap();
        assert!(ab.expected > 0.0 && ab.expected < 1.0);
    }

    #[test]
    fn the_anchor_is_the_specs_once_it_has_games() {
        let spec = PoolSpec { anchor: Some("b".into()), ..PoolSpec::default() };
        let names = engines(&["a", "b", "c"]);
        let info = pool_info(&spec, &names, &games(&[vec![0, 3, 0], vec![1, 0, 0], vec![0, 0, 0]]));
        assert_eq!(info.anchor, Some(1));
        assert!(info.engines[0].elo.unwrap() > 0.0);
        assert_eq!(info.engines[2].elo, None);
        assert_eq!(info.ranking, [0, 1, 2]);
        let none = pool_info(&spec, &names, &[]);
        assert_eq!(none.anchor, None);
    }

    #[test]
    fn complete_pairs_are_counted() {
        let log = vec![game(1, 1, [1, 0], 0), game(1, 2, [1, 0], 1), game(2, 1, [0, 1], 0)];
        let info = pool_info(&PoolSpec::default(), &engines(&["a", "b"]), &log);
        assert_eq!(info.matchups[0].pairs, 1);
        assert_eq!(info.matchups[0].wins, [2, 1]);
    }

    fn tally(wins: Vec<Vec<u32>>, pairs: Vec<Vec<u32>>) -> Tally {
        let n = wins.len();
        Tally { wins, pairs, pending: vec![vec![0; n]; n] }
    }

    #[test]
    fn coverage_comes_first() {
        let spec = PoolSpec::default();
        // c is new: every pick plays c until it has two pairs with each.
        let t = tally(
            vec![vec![0, 20, 0], vec![20, 0, 0], vec![0, 0, 0]],
            vec![vec![0, 20, 0], vec![20, 0, 1], vec![0, 1, 0]],
        );
        for seed in 0..50 {
            let m = choose(&spec, &[false; 3], &t, &mut Rng::new(seed, 1)).unwrap();
            assert!(m.contains(&2) && m.contains(&0), "{m:?}");
        }
        // Retired engines don't play.
        let m = choose(&spec, &[false, false, true], &t, &mut Rng::new(1, 1)).unwrap();
        assert!(!m.contains(&2));
        assert_eq!(choose(&spec, &[false, true, true], &t, &mut Rng::new(1, 1)), None);
    }

    #[test]
    fn the_least_known_matchup_is_drawn_most() {
        let spec = PoolSpec::default();
        // a–b are well measured; b–c only just covered.
        let t = tally(
            vec![vec![0, 200, 2], vec![200, 0, 2], vec![2, 2, 0]],
            vec![vec![0, 200, 2], vec![200, 0, 2], vec![2, 2, 0]],
        );
        let mut counts = HashMap::new();
        for seed in 0..400 {
            let [a, b] = choose(&spec, &[false; 3], &t, &mut Rng::new(seed, 9)).unwrap();
            *counts.entry((a.min(b), a.max(b))).or_insert(0) += 1;
        }
        let ab = counts.get(&(0, 1)).copied().unwrap_or(0);
        assert!(ab > 0, "the floor still gives a–b some: {counts:?}");
        assert!(counts[&(0, 2)] > 3 * ab && counts[&(1, 2)] > 3 * ab, "{counts:?}");
    }

    #[test]
    fn games_in_play_count_as_known() {
        let spec = PoolSpec::default();
        let wins = vec![vec![0, 2, 2], vec![2, 0, 2], vec![2, 2, 0]];
        let pairs = vec![vec![0, 2, 2], vec![2, 0, 2], vec![2, 2, 0]];
        let mut pending = vec![vec![0; 3]; 3];
        // Twenty games of a–b in play.
        pending[0][1] = 20;
        pending[1][0] = 20;
        let t = Tally { wins, pairs, pending };
        let w = weights(&spec, &[(0, 1), (0, 2), (1, 2)], &t);
        assert!(w[0] < w[1] / 2.0 && w[0] < w[2] / 2.0, "{w:?}");
    }

    #[test]
    fn suspect_matchups_get_more() {
        let spec = PoolSpec::default();
        // Evenly matched overall, but a beat c 9–1: out of line, not yet
        // an outlier.
        let wins = vec![vec![0, 20, 9], vec![20, 0, 15], vec![1, 15, 0]];
        let pairs: Vec<Vec<u32>> = wins
            .iter()
            .enumerate()
            .map(|(i, r)| r.iter().enumerate().map(|(j, w)| (w + wins[j][i]) / 2).collect())
            .collect();
        let n = 3;
        let t = Tally { wins, pairs, pending: vec![vec![0; n]; n] };
        let w = weights(&spec, &[(0, 1), (0, 2), (1, 2)], &t);
        assert!(w[1] > w[0] && w[1] > w[2], "{w:?}");
    }

    #[test]
    fn names_must_differ_in_files_too() {
        assert!(check_names(&engines(&["a", "b"])).is_ok());
        assert!(check_names(&engines(&["a", "a"])).unwrap_err().contains("two engines"));
        assert!(check_names(&engines(&["v 1", "v_1"])).unwrap_err().contains("too alike"));
    }
}
