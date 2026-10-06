//! Estimating the game server's clock from its replies.
//!
//! The server stamps its replies with `timeonserver` and times turns with
//! `{w,b}startmove`, both in whole seconds of its own clock. To show a
//! turn's time as the server counts it, the client needs the offset
//! between that clock and its own. This is Cristian's algorithm with
//! interval intersection (as Marzullo's algorithm and NTP do), against the
//! local monotonic clock, so changes to the computer's wall clock don't
//! matter:
//!
//! - A request sent at local time `t0` whose reply arrives at `t1`, stamped
//!   with server second `T`, was stamped somewhere in `[t0, t1]`, when the
//!   server's clock read somewhere in `[T, T + 1)`. So the offset
//!   (server time minus local time) lies in `[T - t1, T + 1 - t0]`.
//! - Every sample's interval holds the true offset, so their intersection
//!   does too. Samples are intersected newest first; one that doesn't fit
//!   the newer ones (the server's clock was stepped, say) is dropped with
//!   everything older.
//! - Long polls are held for minutes, so their intervals are wide, but
//!   their lower bound is tight: the server stamps the reply as it sends
//!   it. Quick requests (`gamestate`) bound it from above.
//! - Within the intersection `[lo, hi]`, the offset sits above `lo` by the
//!   fraction of a second at which the sample setting `lo` was stamped,
//!   plus that reply's travel time. With `k` samples stamped at unrelated
//!   points of their seconds, the smallest fraction is `1 / (k + 1)` on
//!   average; the travel time is taken as half the quickest round trip.
//!   The estimate is `lo` plus those, kept inside `[lo, hi]`.
//!
//! The clocks' rates are taken as equal: drift is parts per million,
//! milliseconds over a long game, and old samples age out of the window.

use std::collections::VecDeque;
use std::time::{Duration, Instant};

/// How many recent samples are kept.
const WINDOW: usize = 64;

/// The longest a reply's travel time is taken to be, when the quickest
/// round trip seen is long (only long polls so far).
const MAX_ONE_WAY: f64 = 0.5;

#[derive(Clone, Copy, Debug)]
struct Sample {
    /// Local seconds (since [`ClockSync`]'s base) when the request went out
    /// and when its reply came back.
    sent: f64,
    received: f64,
    /// The server's whole second in the reply.
    server: i64,
}

impl Sample {
    fn bounds(&self) -> (f64, f64) {
        let t = self.server as f64;
        (t - self.received, t + 1.0 - self.sent)
    }
}

/// The server clock as estimated from replies (see the module docs).
#[derive(Clone, Debug)]
pub struct ClockSync {
    base: Instant,
    samples: VecDeque<Sample>,
}

impl Default for ClockSync {
    fn default() -> Self {
        ClockSync::new()
    }
}

impl ClockSync {
    pub fn new() -> ClockSync {
        ClockSync { base: Instant::now(), samples: VecDeque::new() }
    }

    fn local(&self, at: Instant) -> f64 {
        match at.checked_duration_since(self.base) {
            Some(d) => d.as_secs_f64(),
            None => -self.base.duration_since(at).as_secs_f64(),
        }
    }

    /// Adds a reply stamped `server` (whole seconds) to a request sent at
    /// `sent` whose reply came back at `received`.
    pub fn add(&mut self, sent: Instant, received: Instant, server: i64) {
        let sample = Sample { sent: self.local(sent), received: self.local(received), server };
        if sample.received < sample.sent {
            return;
        }
        self.samples.push_back(sample);
        if self.samples.len() > WINDOW {
            self.samples.pop_front();
        }
    }

    /// The samples that agree with the newest ones: their bounds'
    /// intersection, their count, and the quickest round trip among them.
    fn consistent(&self) -> Option<(f64, f64, usize, f64)> {
        let mut iter = self.samples.iter().rev();
        let first = iter.next()?;
        let (mut lo, mut hi) = first.bounds();
        let mut count = 1;
        let mut rtt = first.received - first.sent;
        for s in iter {
            let (l, h) = s.bounds();
            let (nlo, nhi) = (lo.max(l), hi.min(h));
            if nlo > nhi {
                break;
            }
            (lo, hi) = (nlo, nhi);
            count += 1;
            rtt = rtt.min(s.received - s.sent);
        }
        Some((lo, hi, count, rtt))
    }

    /// The estimated offset: server seconds minus local seconds since the
    /// base. `None` before the first sample.
    fn offset(&self) -> Option<f64> {
        let (lo, hi, count, rtt) = self.consistent()?;
        let one_way = (rtt / 2.0).min(MAX_ONE_WAY);
        Some((lo + 1.0 / (count as f64 + 1.0) + one_way).clamp(lo, hi))
    }

    /// The local instant at which the server's clock reads `server`
    /// seconds, or `None` before the first sample.
    pub fn local_instant(&self, server: f64) -> Option<Instant> {
        let local = server - self.offset()?;
        Some(if local >= 0.0 {
            self.base + Duration::from_secs_f64(local)
        } else {
            self.base.checked_sub(Duration::from_secs_f64(-local))?
        })
    }

    /// The server's clock at local instant `at`, in seconds.
    pub fn server_time(&self, at: Instant) -> Option<f64> {
        Some(self.local(at) + self.offset()?)
    }

    /// How uncertain the offset is: the width of the samples'
    /// intersection.
    pub fn uncertainty(&self) -> Option<Duration> {
        let (lo, hi, ..) = self.consistent()?;
        Some(Duration::from_secs_f64((hi - lo).max(0.0)))
    }

    /// The estimated time for a request to reach the server: half the
    /// quickest round trip seen, at most half a second.
    pub fn one_way_delay(&self) -> Duration {
        let rtt = self.consistent().map_or(0.0, |(.., rtt)| rtt);
        Duration::from_secs_f64((rtt / 2.0).clamp(0.0, MAX_ONE_WAY))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A server whose clock reads `offset` seconds ahead of `sync`'s base,
    /// answering a request sent at local second `sent` after `up` seconds
    /// and reaching the client `down` seconds later.
    fn exchange(sync: &mut ClockSync, offset: f64, sent: f64, up: f64, down: f64) {
        let at = |s: f64| sync.base + Duration::from_secs_f64(s);
        let server = (sent + up + offset).floor() as i64;
        let (t0, t1) = (at(sent), at(sent + up + down));
        sync.add(t0, t1, server);
    }

    fn offset(sync: &ClockSync) -> f64 {
        sync.server_time(sync.base).unwrap()
    }

    #[test]
    fn nothing_is_known_before_a_sample() {
        let sync = ClockSync::new();
        assert!(sync.server_time(Instant::now()).is_none());
        assert!(sync.local_instant(1e9).is_none());
    }

    #[test]
    fn one_quick_reply_is_within_its_round_trip_and_second() {
        let mut sync = ClockSync::new();
        let truth = 1_791_300_000.37;
        exchange(&mut sync, truth, 10.0, 0.1, 0.1);
        let (lo, hi) = sync.samples[0].bounds();
        assert!(lo <= truth && truth <= hi);
        assert!((offset(&sync) - truth).abs() < 1.2);
    }

    #[test]
    fn long_polls_at_varied_moments_converge() {
        let mut sync = ClockSync::new();
        let truth = 1_791_300_000.37;
        // One quick request at the start, then long polls answered at
        // unrelated moments, each reply taking 80 ms to arrive.
        exchange(&mut sync, truth, 0.0, 0.08, 0.08);
        let mut t = 1.0;
        for i in 0..30 {
            let held = 3.0 + (i as f64 * 7.31) % 11.0;
            exchange(&mut sync, truth, t, held, 0.08);
            t += held + 0.08;
        }
        let err = offset(&sync) - truth;
        assert!(err.abs() < 0.12, "error {err}");
        // Only the quick request bounds it from above, so the interval stays
        // about a second wide; the estimate is still close.
        assert!(sync.uncertainty().unwrap() < Duration::from_millis(1200));
        assert_eq!(sync.one_way_delay(), Duration::from_millis(80));
    }

    #[test]
    fn a_stepped_server_clock_is_followed() {
        let mut sync = ClockSync::new();
        let before = 1_791_300_000.25;
        let mut t = 0.0;
        for i in 0..20 {
            exchange(&mut sync, before, t, 0.05 + (i as f64 * 0.37) % 1.0, 0.05);
            t += 2.0;
        }
        // The server's clock jumps 5 s ahead.
        let after = before + 5.0;
        for i in 0..5 {
            exchange(&mut sync, after, t, 0.05 + (i as f64 * 0.61) % 1.0, 0.05);
            t += 2.0;
        }
        let err = offset(&sync) - after;
        assert!(err.abs() < 0.5, "error {err}");
    }

    #[test]
    fn local_and_server_times_round_trip() {
        let mut sync = ClockSync::new();
        exchange(&mut sync, 1_791_300_000.5, 1.0, 0.1, 0.1);
        let at = sync.local_instant(1_791_300_002.0).unwrap();
        let back = sync.server_time(at).unwrap();
        assert!((back - 1_791_300_002.0).abs() < 1e-6);
    }
}
