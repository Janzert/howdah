//! Adaptive runs of analysis over the whole game (`docs/ANALYSIS.md`,
//! "Adaptive runs"): a quick pass over the main line, then more search
//! where it's most likely to change the graph or the marks, within a total
//! time or until stopped. Pure scheduling: the session says what the main line holds
//! ([`Entry`]) and the run says what to search next ([`Next`]).

use std::collections::{HashMap, HashSet, VecDeque};
use std::time::Duration;

use howdah_arimaa::NodeId;

/// The quick pass takes this share of the budget, spread over the positions.
const QUICK_SHARE: u32 = 10;
/// The quick pass's time a position, at least and at most.
const MIN_QUICK: Duration = Duration::from_millis(250);
const MAX_QUICK: Duration = Duration::from_secs(5);
/// A new move in a game in progress adds this many quick passes' time.
const ALLOWANCE_QUICKS: u32 = 4;
/// Positions refined between replans, taken in game order.
const BATCH: usize = 4;
/// Below this priority (a share of the eval bar) a move is settled.
pub const MIN_PRIORITY: f64 = 0.02;
/// A position without an eval is searched again until it has had this many
/// quick passes' time; an engine may never give one for some positions.
const UNEVALUATED_QUICKS: u32 = 8;
/// How near a mark's threshold a drop must be to count, in share.
const NEAR_THRESHOLD: f64 = 0.05;
/// The marks' thresholds (`MARK_DROPS` in the session).
const THRESHOLDS: [f64; 3] = [0.10, 0.15, 0.25];
/// The quick pass's time for priorities when no run has set one.
pub const DEFAULT_QUICK: Duration = Duration::from_secs(1);
/// The quick pass's time a position in a run without a budget.
pub const UNTIL_STOPPED_QUICK: Duration = Duration::from_millis(500);

/// A position of the main line, as the run sees it.
#[derive(Clone, Debug, PartialEq)]
pub struct Entry {
    pub node: NodeId,
    /// Whether the run searches it (not a result on the board).
    pub searchable: bool,
    /// Whether a setup is due here: the move from it is a setup, which
    /// gets no mark, so it gives no reason to search more.
    pub setup: bool,
    /// Gold's share of the eval bar here, if known.
    pub share: Option<f64>,
    /// Whether gold is to move here.
    pub gold_to_move: bool,
    /// Whether the move from here to the next entry was the engine's own
    /// first turn here (then no mark is possible).
    pub next_is_choice: bool,
    /// How long it has been searched (the stored line's time).
    pub searched: Duration,
}

/// What to search next.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Next {
    /// A position for a time.
    Search(NodeId, Duration),
    /// A game in progress with nothing queued: the live position, until
    /// the next move.
    Follow(NodeId),
    /// Nothing left.
    Done,
}

/// Where a run is, for its progress line.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Phase {
    Quick,
    Refining,
    Following,
}

/// A run over the main line.
#[derive(Clone, Debug)]
pub struct Run {
    /// The time in all; `None` until stopped.
    budget: Option<Duration>,
    spent: Duration,
    /// Every move is settled: an unlimited run goes on deepening them all.
    settled_all: bool,
    quick: Duration,
    /// The positions it covers: the main line at the start, and in a game
    /// in progress each new one.
    covered: HashSet<NodeId>,
    /// The longest time the run gave each position.
    given: HashMap<NodeId, Duration>,
    /// Searches that ended by themselves (a proven result): never again.
    settled: HashSet<NodeId>,
    batch: VecDeque<(NodeId, Duration)>,
    /// A position the user asked about, refined next.
    focus: Option<NodeId>,
    /// The search under way, kept until it's done: its own stored line
    /// grows as it goes, which mustn't make the run move on early.
    current: Option<(NodeId, Duration)>,
}

impl Run {
    /// A run over `entries` with `budget` in all, or until stopped
    /// (`None`). The quick pass's time is a tenth of the budget spread over
    /// the positions, within bounds, or [`UNTIL_STOPPED_QUICK`].
    pub fn new(entries: &[Entry], budget: Option<Duration>) -> Run {
        let covered: HashSet<NodeId> = entries.iter().filter(|e| e.searchable).map(|e| e.node).collect();
        let n = u32::try_from(covered.len().max(1)).unwrap_or(u32::MAX);
        let quick = budget.map_or(UNTIL_STOPPED_QUICK, |b| (b / QUICK_SHARE / n).clamp(MIN_QUICK, MAX_QUICK));
        Run {
            budget,
            spent: Duration::ZERO,
            settled_all: false,
            quick,
            covered,
            given: HashMap::new(),
            settled: HashSet::new(),
            batch: VecDeque::new(),
            focus: None,
            current: None,
        }
    }

    #[cfg(test)]
    pub fn quick(&self) -> Duration {
        self.quick
    }

    /// The budget left; `None` in a run until stopped.
    pub fn remaining(&self) -> Option<Duration> {
        self.budget.map(|b| b.saturating_sub(self.spent))
    }

    /// The time searched so far.
    pub fn spent(&self) -> Duration {
        self.spent
    }

    /// Whether every move is settled (a run until stopped then deepens
    /// them all).
    pub fn settled_all(&self) -> bool {
        self.settled_all
    }

    /// How long `e` has been searched, by the run or before it.
    fn searched(&self, e: &Entry) -> Duration {
        self.given.get(&e.node).copied().unwrap_or_default().max(e.searched)
    }

    /// Whether `e` still needs its quick look.
    fn needs_quick(&self, e: &Entry) -> bool {
        e.searchable
            && self.covered.contains(&e.node)
            && !self.settled.contains(&e.node)
            && !self.given.contains_key(&e.node)
            && e.searched * 2 < self.quick
    }

    /// Brings the run up to date with the main line: in a game in progress
    /// (`live`) new positions join it with their allowance, and when the
    /// batch is used up the next one is planned. False when the run is over.
    pub fn refresh(&mut self, entries: &[Entry], live: bool) -> bool {
        if live {
            for e in entries.iter().filter(|e| e.searchable) {
                if self.covered.insert(e.node)
                    && let Some(b) = &mut self.budget
                {
                    *b += self.quick * ALLOWANCE_QUICKS;
                }
            }
        }
        let on_line: HashSet<NodeId> = entries.iter().map(|e| e.node).collect();
        self.batch.retain(|(n, _)| on_line.contains(n));
        if self.current.is_some_and(|(n, _)| !on_line.contains(&n)) {
            self.current = None;
        }
        if self.batch.is_empty() && !entries.iter().any(|e| self.needs_quick(e)) {
            self.batch = self.plan(entries);
        }
        if self.current.is_none()
            && let Next::Search(n, t) = self.pick(entries, false)
        {
            self.current = Some((n, t));
        }
        live || self.next(entries, false) != Next::Done
    }

    /// What to search now: the search under way, else the next one. In a
    /// game in progress (`live`) with nothing queued, the live position:
    /// the main line's last searchable one.
    pub fn next(&self, entries: &[Entry], live: bool) -> Next {
        match self.current {
            Some((n, t)) => Next::Search(n, t),
            None => self.pick(entries, live),
        }
    }

    fn pick(&self, entries: &[Entry], live: bool) -> Next {
        if let Some(e) = entries.iter().find(|e| self.needs_quick(e)) {
            return Next::Search(e.node, self.quick);
        }
        if let Some(&(n, t)) = self.batch.front() {
            return Next::Search(n, t);
        }
        match entries.last().filter(|e| live && e.searchable) {
            Some(e) => Next::Follow(e.node),
            None => Next::Done,
        }
    }

    /// The run's search of `node` is over after `elapsed`; `ended` when
    /// the engine ended it by itself.
    pub fn done(&mut self, node: NodeId, elapsed: Duration, ended: bool) {
        if self.current.is_some_and(|(n, _)| n == node) {
            self.current = None;
        }
        let given = self.given.entry(node).or_default();
        *given = (*given).max(elapsed);
        self.spent += elapsed;
        if ended {
            self.settled.insert(node);
        }
        if self.batch.front().is_some_and(|&(n, _)| n == node) {
            self.batch.pop_front();
        }
    }

    /// Refines `node` and the position before it next.
    pub fn focus(&mut self, node: NodeId) {
        self.focus = Some(node);
        self.batch.clear();
    }

    pub fn phase(&self, entries: &[Entry], live: bool) -> Phase {
        if entries.iter().any(|e| self.needs_quick(e)) {
            return Phase::Quick;
        }
        match self.next(entries, live) {
            Next::Follow(_) => Phase::Following,
            _ => Phase::Refining,
        }
    }

    /// The quick pass's progress: positions looked at, and in all.
    pub fn quick_progress(&self, entries: &[Entry]) -> (usize, usize) {
        let mine: Vec<&Entry> =
            entries.iter().filter(|e| e.searchable && self.covered.contains(&e.node)).collect();
        (mine.iter().filter(|e| !self.needs_quick(e)).count(), mine.len())
    }

    /// Whether the move from `entries[i]` is still unsettled for this run.
    pub fn unsettled(&self, entries: &[Entry], i: usize) -> bool {
        unsettled(entries, i, |e| self.searched(e), self.quick, |e| self.settled.contains(&e.node))
    }

    pub fn queued(&self) -> usize {
        self.batch.len()
    }

    /// The next batch: the positions most worth more search that the
    /// budget still pays for, each at double its time so far, in game order
    /// (so the engine mostly gets `makemove`s and keeps its tables), with a
    /// focused position first.
    fn plan(&mut self, entries: &[Entry]) -> VecDeque<(NodeId, Duration)> {
        let focus = self.focus.take();
        let mut wanted: HashMap<usize, f64> = HashMap::new();
        for (i, p) in priorities(entries, |e| self.searched(e), self.quick) {
            let slot = wanted.entry(i).or_default();
            *slot = slot.max(p);
        }
        if let Some(f) = focus.and_then(|f| entries.iter().position(|e| e.node == f)) {
            for i in [f.wrapping_sub(1), f] {
                if i < entries.len() {
                    wanted.insert(i, f64::INFINITY);
                }
            }
        }
        let open = |i: usize| entries[i].searchable && !self.settled.contains(&entries[i].node);
        let mut picks: Vec<(usize, f64)> =
            wanted.iter().map(|(&i, &p)| (i, p)).filter(|&(i, p)| open(i) && p >= MIN_PRIORITY).collect();
        self.settled_all = picks.is_empty();
        if self.settled_all && self.budget.is_none() {
            // Until stopped: every move is settled, so deepen them all a
            // level at a time, the least searched first and among those the
            // ones nearest to mattering.
            picks = (0..entries.len())
                .filter(|&i| open(i))
                .map(|i| (i, *wanted.get(&i).unwrap_or(&0.0)))
                .collect();
            picks.sort_by(|a, b| {
                (self.searched(&entries[a.0]).cmp(&self.searched(&entries[b.0])))
                    .then(b.1.total_cmp(&a.1))
                    .then(a.0.cmp(&b.0))
            });
        } else {
            picks.sort_by(|a, b| b.1.total_cmp(&a.1).then(a.0.cmp(&b.0)));
        }
        let mut left = self.remaining().unwrap_or(Duration::MAX);
        let mut batch: Vec<(usize, Duration, bool)> = Vec::new();
        for (i, p) in picks {
            if batch.len() == BATCH {
                break;
            }
            let searched = self.searched(&entries[i]);
            let time = (searched * 2).max(self.quick);
            if time <= left {
                left -= time;
                batch.push((i, time, p.is_infinite()));
            } else if batch.is_empty() && left >= searched + self.quick {
                // The last of the budget, when a doubling no longer fits
                // but it still buys a real step beyond what was searched.
                batch.push((i, left, p.is_infinite()));
                break;
            }
        }
        batch.sort_by_key(|&(i, _, focused)| (!focused, i));
        batch.into_iter().map(|(i, t, _)| (entries[i].node, t)).collect()
    }
}

/// How much each position is worth more search, as `(index, priority)`
/// (several per index possible). `searched` says how long a position has
/// been searched; more search already done makes it count for less. A
/// searchable position without an eval (its search gave no line) counts
/// as much as an even game's biggest gap, as nothing around it can be
/// judged until it has one, up to [`UNEVALUATED_QUICKS`].
pub fn priorities(
    entries: &[Entry],
    searched: impl Fn(&Entry) -> Duration,
    quick: Duration,
) -> Vec<(usize, f64)> {
    let cost = |e: &Entry| {
        let ratio = searched(e).as_secs_f64() / quick.as_secs_f64().max(1e-3);
        1.0 + ratio.max(1.0).log2()
    };
    let mut out: Vec<(usize, f64)> = entries
        .iter()
        .enumerate()
        .filter(|(_, e)| e.searchable && e.share.is_none() && searched(e) < quick * UNEVALUATED_QUICKS)
        .map(|(i, e)| (i, 1.0 / cost(e)))
        .collect();
    for i in 0..entries.len().saturating_sub(1) {
        let Some(p) = move_signal(entries, i) else { continue };
        // The pair is refined together: the less searched side first.
        let (a, b) = (&entries[i], &entries[i + 1]);
        let (ta, tb) = (searched(a), searched(b));
        let ends: &[usize] = match (a.searchable, b.searchable) {
            (true, true) if ta == tb => &[i, i + 1],
            (true, true) if ta < tb => &[i],
            (true, true) => &[i + 1],
            (true, false) => &[i],
            (false, true) => &[i + 1],
            (false, false) => &[],
        };
        for &j in ends {
            out.push((j, p / cost(&entries[j])));
        }
    }
    for i in 1..entries.len().saturating_sub(1) {
        let (Some(before), Some(here), Some(after)) =
            (entries[i - 1].share, entries[i].share, entries[i + 1].share)
        else {
            continue;
        };
        let (x, y) = (here - before, here - after);
        if x * y > 0.0 && entries[i].searchable && !entries[i - 1].setup {
            let near = closeness(here).max(closeness(before)).max(closeness(after));
            let p = x.abs().min(y.abs()) * near;
            out.push((i, p / cost(&entries[i])));
        }
    }
    out
}

/// Whether the move from `entries[i]` to the next is still unsettled: more
/// search of a position on either side (one `done` doesn't rule out) could
/// well change its eval or its mark.
pub fn unsettled(
    entries: &[Entry],
    i: usize,
    searched: impl Fn(&Entry) -> Duration,
    quick: Duration,
    done: impl Fn(&Entry) -> bool,
) -> bool {
    move_signal(entries, i).is_some()
        && priorities(entries, searched, quick).into_iter().any(|(j, p)| {
            (j == i || j == i + 1) && p >= MIN_PRIORITY && entries[j].searchable && !done(&entries[j])
        })
}

/// How much more search could change the move from `entries[i]`: the
/// self-consistency gap when it was the engine's choice, otherwise how near
/// its drop is to a mark's threshold, or how far it beat the engine's
/// choice; weighted by how close the game is.
fn move_signal(entries: &[Entry], i: usize) -> Option<f64> {
    let (a, b) = (&entries[i], entries.get(i + 1)?);
    if a.setup {
        return None;
    }
    let (sa, sb) = (a.share?, b.share?);
    let drop = if a.gold_to_move { sa - sb } else { sb - sa };
    let signal = if a.next_is_choice {
        drop.abs()
    } else {
        let near =
            THRESHOLDS.iter().map(|t| (NEAR_THRESHOLD - (drop - t).abs()).max(0.0)).fold(0.0, f64::max);
        near.max(-drop)
    };
    Some(signal * closeness(sa).max(closeness(sb)))
}

/// 1 for an even game, falling to 0 for a decided one.
fn closeness(share: f64) -> f64 {
    4.0 * share * (1.0 - share)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ids(n: usize) -> Vec<NodeId> {
        (0..n as u32).map(NodeId).collect()
    }

    fn entries(shares: &[f64]) -> Vec<Entry> {
        ids(shares.len())
            .into_iter()
            .zip(shares)
            .enumerate()
            .map(|(i, (node, &s))| Entry {
                node,
                searchable: true,
                setup: false,
                share: Some(s),
                gold_to_move: i % 2 == 0,
                next_is_choice: true,
                searched: Duration::ZERO,
            })
            .collect()
    }

    #[test]
    fn a_setup_move_gives_no_reason_to_refine() {
        // Swings from each setup and a spike right after them.
        let mut es = entries(&[0.5, 0.9, 0.2, 0.2, 0.2]);
        let quick = Duration::from_secs(1);
        let at = |es: &[Entry]| {
            let mut v: Vec<usize> = priorities(es, |e| e.searched, quick)
                .into_iter()
                .filter(|&(_, p)| p >= MIN_PRIORITY)
                .map(|(i, _)| i)
                .collect();
            v.sort();
            v.dedup();
            v
        };
        assert!(at(&es).contains(&0), "as moves, they would be refined");
        es[0].setup = true;
        es[1].setup = true;
        assert_eq!(at(&es), Vec::<usize>::new(), "not as setups");
    }

    fn quick_pass(run: &mut Run, es: &[Entry]) {
        while let Next::Search(n, t) = run.next(es, false) {
            if t != run.quick() {
                break;
            }
            run.done(n, t, false);
        }
    }

    #[test]
    fn the_quick_pass_goes_forward_then_the_run_refines_what_disagrees() {
        // Position 3 is out of line with both neighbours: a spike.
        let es = entries(&[0.5, 0.5, 0.52, 0.2, 0.5, 0.5]);
        let mut run = Run::new(&es, Some(Duration::from_secs(60)));
        assert_eq!(run.quick(), Duration::from_secs(1), "a tenth of a minute over six positions");
        let mut order = Vec::new();
        while let Next::Search(n, t) = run.next(&es, false) {
            if t != run.quick() {
                break;
            }
            order.push(n);
            run.done(n, t, false);
        }
        assert_eq!(order, es.iter().map(|e| e.node).collect::<Vec<_>>(), "in game order");
        assert!(run.refresh(&es, false));
        let batch: Vec<NodeId> = run.batch.iter().map(|&(n, _)| n).collect();
        assert!(batch.contains(&es[3].node), "the spike: {batch:?}");
        assert!(!batch.contains(&es[0].node), "the settled start");
        let mut sorted = batch.clone();
        sorted.sort_by_key(|n| es.iter().position(|e| e.node == *n));
        assert_eq!(batch, sorted, "game order");
        assert!(run.batch.iter().all(|&(_, t)| t == Duration::from_secs(2)), "double the quick pass");
    }

    #[test]
    fn a_position_left_without_an_eval_is_searched_again() {
        // The quick look at position 2 gave no line: no share.
        let mut es = entries(&[0.5, 0.51, 0.5, 0.51, 0.5]);
        es[2].share = None;
        let mut run = Run::new(&es, Some(Duration::from_secs(60)));
        quick_pass(&mut run, &es);
        assert!(run.refresh(&es, false), "not settled while a position has no eval");
        assert_eq!(run.next(&es, false), Next::Search(es[2].node, run.quick() * 2));
        // Still none after a few tries: the engine gives none here, so
        // the run stops asking.
        run.done(es[2].node, run.quick() * 2, false);
        es[2].searched = run.quick() * UNEVALUATED_QUICKS;
        assert!(!run.refresh(&es, false), "settled");
    }

    #[test]
    fn a_search_under_way_is_kept_while_its_line_grows() {
        let mut es = entries(&[0.5, 0.5, 0.5]);
        let mut run = Run::new(&es, Some(Duration::from_secs(30)));
        run.refresh(&es, false);
        assert_eq!(run.next(&es, false), Next::Search(es[0].node, run.quick()));
        // Its own stored line passes half the quick time mid-search.
        es[0].searched = run.quick();
        run.refresh(&es, false);
        assert_eq!(run.next(&es, false), Next::Search(es[0].node, run.quick()));
        run.done(es[0].node, run.quick(), false);
        run.refresh(&es, false);
        assert_eq!(run.next(&es, false), Next::Search(es[1].node, run.quick()));
        assert_eq!(run.remaining(), Some(Duration::from_secs(30) - run.quick()), "charged");
    }

    #[test]
    fn a_run_until_stopped_deepens_a_settled_game_and_never_ends() {
        let es = entries(&[0.5, 0.51, 0.5, 0.51, 0.5]);
        let mut run = Run::new(&es, None);
        assert_eq!(run.quick(), UNTIL_STOPPED_QUICK);
        assert_eq!(run.remaining(), None);
        let mut times = HashMap::new();
        for _ in 0..40 {
            assert!(run.refresh(&es, false), "it goes on");
            let Next::Search(n, t) = run.next(&es, false) else { panic!("something to search") };
            run.done(n, t, false);
            times.insert(n, t);
        }
        assert!(run.settled_all(), "a steady game is settled");
        // It deepens every position, none far ahead of the others.
        let (lo, hi) = (times.values().min().unwrap(), times.values().max().unwrap());
        assert_eq!(times.len(), es.len());
        assert!(*hi <= *lo * 4, "{lo:?} to {hi:?}");
        // Proven everywhere: nothing left to search, so even this run ends.
        for e in &es {
            run.done(e.node, Duration::ZERO, true);
        }
        run.batch.clear();
        assert!(!run.refresh(&es, false));
    }

    #[test]
    fn a_steady_game_is_settled_after_the_quick_pass() {
        let es = entries(&[0.5, 0.51, 0.5, 0.51, 0.5]);
        let mut run = Run::new(&es, Some(Duration::from_secs(60)));
        quick_pass(&mut run, &es);
        assert!(!run.refresh(&es, false), "nothing to refine: the run ends");
        assert_eq!(run.next(&es, false), Next::Done);
    }

    #[test]
    fn a_drop_near_a_threshold_is_refined_on_both_sides() {
        // Silver's move loses 0.11, near the 0.10 threshold.
        let mut es = entries(&[0.5, 0.5, 0.61, 0.61]);
        es[1].next_is_choice = false;
        let mut run = Run::new(&es, Some(Duration::from_secs(60)));
        quick_pass(&mut run, &es);
        run.refresh(&es, false);
        let batch: Vec<NodeId> = run.batch.iter().map(|&(n, _)| n).collect();
        assert_eq!(batch, vec![es[1].node, es[2].node]);
    }

    #[test]
    fn decided_positions_and_proven_searches_get_no_more() {
        let mut es = entries(&[0.5, 0.5, 0.0, 0.0]);
        es[1].next_is_choice = false;
        let mut run = Run::new(&es, Some(Duration::from_secs(60)));
        quick_pass(&mut run, &es);
        run.refresh(&es, false);
        // Silver's move beat the engine's choice by a half: both sides are
        // refined, but not the lost position after it.
        let batch: Vec<NodeId> = run.batch.iter().map(|&(n, _)| n).collect();
        assert_eq!(batch, vec![es[1].node, es[2].node]);
        // A position whose search proved its result is never searched again.
        let mut run = Run::new(&es, Some(Duration::from_secs(60)));
        for e in &es {
            run.done(e.node, run.quick(), true);
        }
        assert!(!run.refresh(&es, false));
    }

    #[test]
    fn the_budget_bounds_the_refinement() {
        let es = entries(&[0.5, 0.5, 0.2, 0.5, 0.2, 0.5]);
        let mut run = Run::new(&es, Some(Duration::from_secs(6)));
        assert_eq!(run.quick(), MIN_QUICK);
        let mut total = Duration::ZERO;
        let mut visits = 0;
        loop {
            run.refresh(&es, false);
            let Next::Search(n, t) = run.next(&es, false) else { break };
            run.done(n, t, false);
            total += t;
            visits += 1;
            assert!(visits < 100, "it ends");
        }
        assert!(total <= Duration::from_secs(6), "{total:?}");
        assert!(visits > es.len(), "it refined some");
        assert!(run.remaining().unwrap() < run.quick() * 2, "it used the budget: {:?} left", run.remaining());
    }

    #[test]
    fn a_focused_position_comes_first_with_the_one_before_it() {
        let es = entries(&[0.5, 0.5, 0.5, 0.5, 0.5]);
        let mut run = Run::new(&es, Some(Duration::from_secs(60)));
        quick_pass(&mut run, &es);
        run.focus(es[3].node);
        assert!(run.refresh(&es, false));
        let batch: Vec<NodeId> = run.batch.iter().map(|&(n, _)| n).collect();
        assert_eq!(batch, vec![es[2].node, es[3].node]);
    }

    #[test]
    fn a_game_in_progress_follows_the_live_position_and_takes_new_moves() {
        let all = entries(&[0.5, 0.5, 0.5, 0.5, 0.5]);
        let es = &all[..3];
        let mut run = Run::new(es, Some(Duration::from_secs(30)));
        quick_pass(&mut run, es);
        assert!(run.refresh(es, true), "a live run waits");
        assert_eq!(run.next(es, true), Next::Follow(es[2].node));
        assert_eq!(run.phase(es, true), Phase::Following);
        let before = run.remaining().unwrap();
        // A move arrives: its position gets the quick look, paid by its allowance.
        let es = &all[..4];
        run.refresh(es, true);
        assert_eq!(run.remaining(), Some(before + run.quick() * ALLOWANCE_QUICKS));
        assert_eq!(run.next(es, true), Next::Search(es[3].node, run.quick()));
        run.done(es[3].node, run.quick(), false);
        run.refresh(es, true);
        assert_eq!(run.next(es, true), Next::Follow(es[3].node));
        // The game ends: with nothing to refine, the run is over.
        assert!(!run.refresh(es, false));
    }

    #[test]
    fn a_long_search_settles_a_move() {
        let es = entries(&[0.5, 0.5, 0.45, 0.5]);
        let searched = |_: &Entry| Duration::from_secs(1);
        let never = |_: &Entry| false;
        assert!(unsettled(&es, 1, searched, Duration::from_secs(1), never));
        let long = |_: &Entry| Duration::from_secs(64);
        assert!(!unsettled(&es, 1, long, Duration::from_secs(1), never));
        assert!(!unsettled(&es, 1, searched, Duration::from_secs(1), |_| true), "proven on both sides");
    }
}
