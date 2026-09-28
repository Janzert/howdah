//! Arimaa time controls: the `M/R/P/L/G/T` format and clock arithmetic.
//!
//! - `M`: time per move
//! - `R`: starting reserve
//! - `P`: percent of unused move time added to the reserve (default 100)
//! - `L`: reserve limit (0 = none)
//! - `G`: game time limit, or a turn limit with a `t` suffix (0 = none)
//! - `T`: maximum time for a single turn (0 = none)
//!
//! Only `M` and `R` are required. Times are whole seconds, written with
//! units (`30s`, `5m`, `1h`, `1d`, combinable as `1m30s`), or as bare or
//! colon-separated numbers whose first unit is minutes (hours for `G`), so
//! `1:30` is 90 s but `G` = `1:30` is 90 minutes. This follows AEI's
//! `pyrimaa/util.py`.

use std::fmt;
use std::str::FromStr;
use std::time::Duration;

use crate::error::ParseError;
use crate::types::data_type;

data_type! {
    /// All times in seconds; 0 means "no limit" for `max_reserve`,
    /// `time_limit` and `max_turn_time`.
    #[cfg_attr(feature = "serde", serde(rename_all = "camelCase"))]
    pub struct TimeControl {
        pub move_time: u32,
        pub reserve: u32,
        pub percent: u32,
        pub max_reserve: u32,
        /// Maximum number of turns per side (0 = none). Mutually exclusive with `time_limit`.
        pub turn_limit: u32,
        /// Game time limit in seconds (0 = none).
        pub time_limit: u32,
        pub max_turn_time: u32,
    }
}

const UNITS: [(char, u32); 4] = [('d', 86_400), ('h', 3_600), ('m', 60), ('s', 1)];

fn unit_seconds(c: char) -> Option<u32> {
    UNITS.iter().find(|(u, _)| *u == c).map(|&(_, s)| s)
}

/// Parses one time field. Bare numbers and `:`-separated parts start at
/// `start_unit` and step down one unit per colon.
fn parse_time_field(field: &str, start_unit: char) -> Result<u32, ParseError> {
    let err = || ParseError::new(format!("invalid time {field:?}"));
    let order = ['d', 'h', 'm', 's'];
    let mut implicit = order.iter().position(|&u| u == start_unit).ok_or_else(err)?;
    let mut total: u32 = 0;
    // ":30" means "0:30".
    let padded = if field.starts_with(':') { format!("0{field}") } else { field.to_string() };
    let mut rest = padded.as_str();
    while !rest.is_empty() {
        let digits = rest.find(|c: char| !c.is_ascii_digit()).unwrap_or(rest.len());
        if digits == 0 {
            return Err(err());
        }
        let n: u32 = rest[..digits].parse().map_err(|_| err())?;
        rest = &rest[digits..];
        let unit = match rest.chars().next() {
            None | Some(':') => {
                let u = *order.get(implicit).ok_or_else(err)?;
                implicit += 1;
                u
            }
            Some(c) => c,
        };
        let secs = unit_seconds(unit).ok_or_else(err)?;
        total = n.checked_mul(secs).and_then(|v| total.checked_add(v)).ok_or_else(err)?;
        if !rest.is_empty() {
            rest = &rest[1..];
        }
    }
    Ok(total)
}

/// Formats seconds compactly with units, e.g. 90 -> `1m30s`, 0 -> `0`.
fn format_time(mut secs: u32) -> String {
    if secs == 0 {
        return "0".to_string();
    }
    let mut out = String::new();
    for (unit, len) in UNITS {
        if secs >= len {
            out.push_str(&format!("{}{unit}", secs / len));
            secs %= len;
        }
    }
    out
}

impl FromStr for TimeControl {
    type Err = ParseError;

    fn from_str(s: &str) -> Result<TimeControl, ParseError> {
        let fields: Vec<&str> = s.trim().split('/').collect();
        if fields.len() < 2 || fields.len() > 6 {
            return Err(ParseError::new(format!("time control {s:?} needs 2 to 6 fields")));
        }
        let field = |i: usize| fields.get(i).copied().filter(|f| !f.is_empty());
        let time = |i: usize, unit| field(i).map_or(Ok(0), |f| parse_time_field(f, unit));
        let move_time = time(0, 'm')?;
        if field(1).is_none() {
            return Err(ParseError::new("time control needs a starting reserve"));
        }
        let reserve = time(1, 'm')?;
        let percent = match field(2) {
            None => 100,
            Some(p) => p.parse().map_err(|_| ParseError::new(format!("invalid percent {p:?}")))?,
        };
        let max_reserve = time(3, 'm')?;
        let (turn_limit, time_limit) = match field(4) {
            Some(g) if g.ends_with('t') => {
                let turns = g[..g.len() - 1]
                    .parse()
                    .map_err(|_| ParseError::new(format!("invalid turn limit {g:?}")))?;
                (turns, 0)
            }
            _ => (0, time(4, 'h')?),
        };
        let max_turn_time = time(5, 'm')?;
        Ok(TimeControl { move_time, reserve, percent, max_reserve, turn_limit, time_limit, max_turn_time })
    }
}

/// Canonical form, dropping trailing default fields (`1m30s/5m`).
impl fmt::Display for TimeControl {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let game =
            if self.turn_limit > 0 { format!("{}t", self.turn_limit) } else { format_time(self.time_limit) };
        let mut out = vec![
            format_time(self.move_time),
            format_time(self.reserve),
            self.percent.to_string(),
            format_time(self.max_reserve),
            game,
            format_time(self.max_turn_time),
        ];
        let defaults = ["", "", "100", "0", "0", "0"];
        while out.len() > 2 && out.last().map(String::as_str) == Some(defaults[out.len() - 1]) {
            out.pop();
        }
        // pyrimaa writes an all-zero control with an explicit reserve limit.
        if out == ["0", "0", "0"] {
            out.push("0".into());
        }
        write!(f, "{}", out.join("/"))
    }
}

impl TimeControl {
    pub fn move_time(&self) -> Duration {
        Duration::from_secs(self.move_time.into())
    }

    pub fn starting_reserve(&self) -> Duration {
        Duration::from_secs(self.reserve.into())
    }

    /// Longest the side to move may take this turn, given its reserve.
    pub fn turn_allowance(&self, reserve: Duration) -> Duration {
        let allowance = self.move_time() + reserve;
        if self.max_turn_time > 0 {
            allowance.min(Duration::from_secs(self.max_turn_time.into()))
        } else {
            allowance
        }
    }

    /// Reserve after a turn that took `used`. Unused move time is added at
    /// `percent`; overtime comes out of the reserve; the result is capped at
    /// `max_reserve`. Setup moves don't change the reserve.
    pub fn reserve_after(&self, reserve: Duration, used: Duration) -> Duration {
        let move_time = self.move_time();
        let updated = if used <= move_time {
            reserve + (move_time - used).mul_f64(f64::from(self.percent) / 100.0)
        } else {
            reserve.saturating_sub(used - move_time)
        };
        if self.max_reserve > 0 { updated.min(Duration::from_secs(self.max_reserve.into())) } else { updated }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tc(s: &str) -> TimeControl {
        s.parse().unwrap()
    }

    // Cases from AEI's pyrimaa/tests/test_util.py.

    #[test]
    fn move_time() {
        assert!("none".parse::<TimeControl>().is_err());
        assert_eq!(tc("30s/1s").move_time, 30);
        assert_eq!(tc("5m/1s").move_time, 300);
        assert_eq!(tc("1h/1s").move_time, 3600);
        assert_eq!(tc("1d/1s").move_time, 86_400);
        assert_eq!(tc("1:30/1s").move_time, 90);
        assert_eq!(tc("2/2").move_time, 120, "bare numbers are minutes");
    }

    #[test]
    fn reserve() {
        assert!("30s".parse::<TimeControl>().is_err());
        assert_eq!(tc("30s/10s").reserve, 10);
        assert_eq!(tc("30s/1:30").reserve, 90);
    }

    #[test]
    fn percent() {
        assert_eq!(tc("30s/10s").percent, 100);
        assert_eq!(tc("30s/10s/100").percent, 100);
        assert_eq!(tc("30s/10s/50").percent, 50);
        assert_eq!(tc("30s/10s/0").percent, 0);
    }

    #[test]
    fn max_reserve() {
        assert_eq!(tc("30s/10s").max_reserve, 0);
        assert_eq!(tc("30s/10s/100/0").max_reserve, 0);
        assert_eq!(tc("30s/10s/100/10s").max_reserve, 10);
        assert_eq!(tc("30s/10s/100/1:30").max_reserve, 90);
    }

    #[test]
    fn turn_and_time_limits() {
        assert_eq!(tc("30s/10s").turn_limit, 0);
        assert_eq!(tc("30s/10s/100/0/0t").turn_limit, 0);
        assert_eq!(tc("30s/10s/100/0/120t").turn_limit, 120);
        assert_eq!(tc("30s/10s").time_limit, 0);
        assert_eq!(tc("30s/10s/100/0/1h").time_limit, 3600);
        assert_eq!(tc("30s/10s/100/0/1:30").time_limit, 90 * 60, "G starts at hours");
    }

    #[test]
    fn max_turn_time() {
        assert_eq!(tc("30s/10s").max_turn_time, 0);
        assert_eq!(tc("30s/10s/100/0/0/2m").max_turn_time, 120);
        assert_eq!(tc("30s/10s/100/0/0/1:30").max_turn_time, 90);
    }

    #[test]
    fn display() {
        assert_eq!(tc("0/0/0").to_string(), "0/0/0/0");
        assert_eq!(tc("1:30/5/100/0").to_string(), "1m30s/5m");
        assert_eq!(tc("60m/15h/50").to_string(), "1h/15h/50");
        assert_eq!(tc("1/1/100/10/5/5").to_string(), "1m/1m/100/10m/5h/5m");
        assert_eq!(tc("1/1/100/0/50t/5").to_string(), "1m/1m/100/0/50t/5m");
        for s in ["1m30s/5m", "1h/15h/50", "1m/1m/100/10m/5h/5m", "1m/1m/100/0/50t/5m", "0/0/0/0"] {
            assert_eq!(tc(s).to_string(), s, "round trip");
        }
    }

    #[test]
    fn bad_fields() {
        for bad in ["30x/1", "30s/", "30s/1s/abc", "30s/1s/100/0/xt", "1/2/3/4/5/6/7", "::/1"] {
            assert!(bad.parse::<TimeControl>().is_err(), "{bad}");
        }
    }

    #[test]
    fn clock_arithmetic() {
        let t = tc("30s/1m/50/90s/0/45s");
        let s = Duration::from_secs;
        assert_eq!(t.turn_allowance(s(60)), s(45), "capped by max turn time");
        assert_eq!(tc("30s/1m").turn_allowance(s(60)), s(90));
        // 10 s unused, half of it added.
        assert_eq!(t.reserve_after(s(60), s(20)), s(65));
        // 5 s over the move time comes out of the reserve.
        assert_eq!(t.reserve_after(s(60), s(35)), s(55));
        // Capped at 90 s.
        assert_eq!(t.reserve_after(s(88), s(0)), s(90));
        assert_eq!(t.reserve_after(s(3), s(60)), s(0));
    }
}
