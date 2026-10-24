//! Arimaa time controls: the `M/R/P/L/G/T` format and clock arithmetic.
//!
//! - `M`: time per move
//! - `R`: starting reserve
//! - `P`: percent of unused move time added to the reserve (default 100)
//! - `L`: reserve limit (0 = none)
//! - `G`: game time limit, or a turn limit with a `t` suffix (0 = none)
//! - `T`: maximum time for a single turn (0 = none)
//!
//! Only `M` and `R` are required. Times are written with units (`30s`,
//! `5m`, `1h`, `1d`, combinable as `1m30s`), or as bare or colon-separated
//! numbers whose first unit is minutes (hours for `G`), so `1:30` is 90 s
//! but `G` = `1:30` is 90 minutes. This follows AEI's `pyrimaa/util.py`,
//! which takes whole numbers only; here a number may have a decimal
//! fraction (`0.5s`, `1.5m`, `0:02.25`) as long as the time comes to whole
//! milliseconds.
//!
//! The setup follows arimaa.com's match rules (`matchRules.html`): with
//! under a minute per move it gets a minute, past which the reserve is
//! used, and unused setup time is never added to the reserve. The minute
//! is there for humans; `with_setup_minimum` changes or drops it (engine
//! games), and records say so with a `SetupMinimum` tag
//! ([`SETUP_MINIMUM_TAG`]). It isn't part of the `M/R/P/L/G/T` text, which
//! always means the minute. (pyrimaa gives the setup the plain move time,
//! doesn't add unused time, and never charges overtime to the reserve.)

use std::fmt;
use std::str::FromStr;
use std::time::Duration;

use crate::error::ParseError;
use crate::types::data_type;

data_type! {
    /// All times in milliseconds; 0 means "no limit" for `max_reserve_ms`,
    /// `time_limit_ms` and `max_turn_ms`.
    #[cfg_attr(feature = "serde", serde(rename_all = "camelCase"))]
    pub struct TimeControl {
        pub move_ms: u64,
        pub reserve_ms: u64,
        pub percent: u32,
        pub max_reserve_ms: u64,
        /// Maximum number of turns per side (0 = none). Mutually exclusive with `time_limit_ms`.
        pub turn_limit: u32,
        /// Game time limit (0 = none).
        pub time_limit_ms: u64,
        pub max_turn_ms: u64,
        /// The least move time a setup gets (arimaa.com's minute unless
        /// changed; 0 for none).
        pub setup_minimum_ms: u64,
    }
}

/// The least move time a setup gets by arimaa.com's rules.
pub const DEFAULT_SETUP_MINIMUM: Duration = Duration::from_secs(60);

/// The record tag naming a game's setup minimum when it isn't
/// [`DEFAULT_SETUP_MINIMUM`], in the time control's syntax (`0`, `10s`).
pub const SETUP_MINIMUM_TAG: &str = "SetupMinimum";

const UNITS: [(char, u64); 4] = [('d', 86_400_000), ('h', 3_600_000), ('m', 60_000), ('s', 1_000)];

fn unit_ms(c: char) -> Option<u64> {
    UNITS.iter().find(|(u, _)| *u == c).map(|&(_, ms)| ms)
}

/// Parses one time field into milliseconds. Bare numbers and
/// `:`-separated parts start at `start_unit` and step down one unit per
/// colon. Each number may have a decimal fraction.
fn parse_time_field(field: &str, start_unit: char) -> Result<u64, ParseError> {
    let err = || ParseError::new(format!("invalid time {field:?}"));
    let order = ['d', 'h', 'm', 's'];
    let mut implicit = order.iter().position(|&u| u == start_unit).ok_or_else(err)?;
    let mut total: u64 = 0;
    // ":30" means "0:30".
    let padded = if field.starts_with(':') { format!("0{field}") } else { field.to_string() };
    let mut rest = padded.as_str();
    while !rest.is_empty() {
        let digits = |s: &str| s.find(|c: char| !c.is_ascii_digit()).unwrap_or(s.len());
        let whole = digits(rest);
        if whole == 0 {
            return Err(err());
        }
        let mut len = whole;
        if rest[len..].starts_with('.') {
            let fraction = digits(&rest[len + 1..]);
            if fraction == 0 {
                return Err(err());
            }
            len += 1 + fraction;
        }
        let number = &rest[..len];
        rest = &rest[len..];
        let unit = match rest.chars().next() {
            None | Some(':') => {
                let u = *order.get(implicit).ok_or_else(err)?;
                implicit += 1;
                u
            }
            Some(c) => c,
        };
        let ms = scale(number, unit_ms(unit).ok_or_else(err)?)
            .ok_or_else(|| ParseError::new(format!("invalid time {field:?}: times go to the millisecond")))?;
        total = total.checked_add(ms).ok_or_else(err)?;
        if !rest.is_empty() {
            rest = &rest[1..];
        }
    }
    Ok(total)
}

/// `number` (digits, maybe with a decimal fraction) times `unit` ms, if
/// that's a whole number of milliseconds that fits.
fn scale(number: &str, unit: u64) -> Option<u64> {
    let (whole, fraction) = number.split_once('.').unwrap_or((number, ""));
    let denominator = 10u128.checked_pow(fraction.len() as u32)?;
    let numerator: u128 = format!("{whole}{fraction}").parse().ok()?;
    let ms = numerator.checked_mul(unit.into())?;
    if ms % denominator != 0 {
        return None;
    }
    u64::try_from(ms / denominator).ok()
}

/// Formats milliseconds compactly with units, e.g. 90 000 -> `1m30s`,
/// 1 500 -> `1.5s`, 0 -> `0`.
fn format_time(mut ms: u64) -> String {
    if ms == 0 {
        return "0".to_string();
    }
    let mut out = String::new();
    for (unit, len) in &UNITS[..3] {
        if ms >= *len {
            out.push_str(&format!("{}{unit}", ms / len));
            ms %= len;
        }
    }
    if ms > 0 {
        out.push_str(&format!("{}s", format_seconds(Duration::from_millis(ms))));
    }
    out
}

/// A time as seconds, with as many decimals as its milliseconds need
/// (`12`, `0.5`, `1.25`).
pub fn format_seconds(time: Duration) -> String {
    let ms = time.as_millis();
    let (secs, frac) = (ms / 1000, ms % 1000);
    if frac == 0 {
        secs.to_string()
    } else {
        format!("{secs}.{}", format!("{frac:03}").trim_end_matches('0'))
    }
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
        let move_ms = time(0, 'm')?;
        if field(1).is_none() {
            return Err(ParseError::new("time control needs a starting reserve"));
        }
        let reserve_ms = time(1, 'm')?;
        let percent = match field(2) {
            None => 100,
            Some(p) => p.parse().map_err(|_| ParseError::new(format!("invalid percent {p:?}")))?,
        };
        let max_reserve_ms = time(3, 'm')?;
        let (turn_limit, time_limit_ms) = match field(4) {
            Some(g) if g.ends_with('t') => {
                let turns = g[..g.len() - 1]
                    .parse()
                    .map_err(|_| ParseError::new(format!("invalid turn limit {g:?}")))?;
                (turns, 0)
            }
            _ => (0, time(4, 'h')?),
        };
        let max_turn_ms = time(5, 'm')?;
        Ok(TimeControl {
            move_ms,
            reserve_ms,
            percent,
            max_reserve_ms,
            turn_limit,
            time_limit_ms,
            max_turn_ms,
            setup_minimum_ms: DEFAULT_SETUP_MINIMUM.as_millis() as u64,
        })
    }
}

/// Canonical form, dropping trailing default fields (`1m30s/5m`).
impl fmt::Display for TimeControl {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let game = if self.turn_limit > 0 {
            format!("{}t", self.turn_limit)
        } else {
            format_time(self.time_limit_ms)
        };
        let mut out = vec![
            format_time(self.move_ms),
            format_time(self.reserve_ms),
            self.percent.to_string(),
            format_time(self.max_reserve_ms),
            game,
            format_time(self.max_turn_ms),
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
        Duration::from_millis(self.move_ms)
    }

    pub fn starting_reserve(&self) -> Duration {
        Duration::from_millis(self.reserve_ms)
    }

    /// The reserve limit; zero for none.
    pub fn max_reserve(&self) -> Duration {
        Duration::from_millis(self.max_reserve_ms)
    }

    /// The game time limit; zero for none.
    pub fn time_limit(&self) -> Duration {
        Duration::from_millis(self.time_limit_ms)
    }

    /// The longest a single turn may take; zero for no limit.
    pub fn max_turn_time(&self) -> Duration {
        Duration::from_millis(self.max_turn_ms)
    }

    /// Whether every time is whole seconds, as arimaa.com and pyrimaa need.
    pub fn is_whole_seconds(&self) -> bool {
        [self.move_ms, self.reserve_ms, self.max_reserve_ms, self.time_limit_ms, self.max_turn_ms]
            .iter()
            .all(|ms| ms % 1000 == 0)
    }

    /// Longest the side to move may take this turn, given its reserve.
    pub fn turn_allowance(&self, reserve: Duration) -> Duration {
        let allowance = self.move_time() + reserve;
        if self.max_turn_ms > 0 { allowance.min(self.max_turn_time()) } else { allowance }
    }

    /// The same time control with `minimum` as the least move time a
    /// setup gets (zero: the plain move time).
    pub fn with_setup_minimum(self, minimum: Duration) -> TimeControl {
        TimeControl { setup_minimum_ms: minimum.as_millis() as u64, ..self }
    }

    pub fn setup_minimum(&self) -> Duration {
        Duration::from_millis(self.setup_minimum_ms)
    }

    /// Parses a time in the time control's syntax (`0`, `10s`, `1:30`;
    /// a bare number is minutes), as the `SetupMinimum` tag holds one.
    pub fn parse_time(text: &str) -> Result<Duration, ParseError> {
        parse_time_field(text.trim(), 'm').map(Duration::from_millis)
    }

    /// Writes a time in the time control's syntax (`0`, `1m30s`, `0.5s`).
    pub fn format_time(time: Duration) -> String {
        format_time(time.as_millis() as u64)
    }

    /// Move time for the setup: the move time, but at least the setup
    /// minimum. By default, with under a minute per move each player gets
    /// a minute for the setup (arimaa.com's match rules).
    pub fn setup_move_time(&self) -> Duration {
        self.move_time().max(self.setup_minimum())
    }

    /// Longest a player may take for the setup, given its reserve. A turn
    /// time limit doesn't cut the setup's minute short.
    pub fn setup_allowance(&self, reserve: Duration) -> Duration {
        let allowance = self.setup_move_time() + reserve;
        if self.max_turn_ms > 0 {
            allowance.min(self.max_turn_time().max(self.setup_move_time()))
        } else {
            allowance
        }
    }

    /// Reserve after a setup that took `used`: time past the setup's move
    /// time comes out of the reserve, and unused time isn't added to it.
    pub fn reserve_after_setup(&self, reserve: Duration, used: Duration) -> Duration {
        reserve.saturating_sub(used.saturating_sub(self.setup_move_time()))
    }

    /// Reserve after a turn that took `used`. Unused move time is added at
    /// `percent`; overtime comes out of the reserve; the result is capped at
    /// the reserve limit. Setups use `reserve_after_setup` instead.
    pub fn reserve_after(&self, reserve: Duration, used: Duration) -> Duration {
        let move_time = self.move_time();
        let updated = if used <= move_time {
            reserve + (move_time - used).mul_f64(f64::from(self.percent) / 100.0)
        } else {
            reserve.saturating_sub(used - move_time)
        };
        if self.max_reserve_ms > 0 { updated.min(self.max_reserve()) } else { updated }
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
        assert_eq!(tc("30s/1s").move_ms, 30_000);
        assert_eq!(tc("5m/1s").move_ms, 300_000);
        assert_eq!(tc("1h/1s").move_ms, 3_600_000);
        assert_eq!(tc("1d/1s").move_ms, 86_400_000);
        assert_eq!(tc("1:30/1s").move_ms, 90_000);
        assert_eq!(tc("2/2").move_ms, 120_000, "bare numbers are minutes");
    }

    #[test]
    fn reserve() {
        assert!("30s".parse::<TimeControl>().is_err());
        assert_eq!(tc("30s/10s").reserve_ms, 10_000);
        assert_eq!(tc("30s/1:30").reserve_ms, 90_000);
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
        assert_eq!(tc("30s/10s").max_reserve_ms, 0);
        assert_eq!(tc("30s/10s/100/0").max_reserve_ms, 0);
        assert_eq!(tc("30s/10s/100/10s").max_reserve_ms, 10_000);
        assert_eq!(tc("30s/10s/100/1:30").max_reserve_ms, 90_000);
    }

    #[test]
    fn turn_and_time_limits() {
        assert_eq!(tc("30s/10s").turn_limit, 0);
        assert_eq!(tc("30s/10s/100/0/0t").turn_limit, 0);
        assert_eq!(tc("30s/10s/100/0/120t").turn_limit, 120);
        assert_eq!(tc("30s/10s").time_limit_ms, 0);
        assert_eq!(tc("30s/10s/100/0/1h").time_limit_ms, 3_600_000);
        assert_eq!(tc("30s/10s/100/0/1:30").time_limit_ms, 5_400_000, "G starts at hours");
    }

    #[test]
    fn max_turn_time() {
        assert_eq!(tc("30s/10s").max_turn_ms, 0);
        assert_eq!(tc("30s/10s/100/0/0/2m").max_turn_ms, 120_000);
        assert_eq!(tc("30s/10s/100/0/0/1:30").max_turn_ms, 90_000);
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
    fn fractions() {
        assert_eq!(tc("0.5s/1s").move_ms, 500);
        assert_eq!(tc("1.5m/0").move_ms, 90_000);
        assert_eq!(tc("0:02.25/0").move_ms, 2_250, "the colon form's seconds");
        assert_eq!(tc("1s/0.125s").reserve_ms, 125);
        assert_eq!(tc("1s/0/100/0/0.5h").time_limit_ms, 1_800_000);
        assert_eq!(tc("0.1s/0/100/0/0/0.3s").max_turn_ms, 300);
        assert_eq!(tc("0.5s/1m0.25s").to_string(), "0.5s/1m0.25s");
        assert_eq!(tc("1.5s/2s").to_string(), "1.5s/2s");
        assert!(tc("30s/1m").is_whole_seconds());
        assert!(!tc("0.5s/1m").is_whole_seconds());
        for bad in ["0.0005s/1s", "1./1s", ".5s/1s", "1.5.5s/1s"] {
            assert!(bad.parse::<TimeControl>().is_err(), "{bad}");
        }
        assert_eq!(tc("1s/0.5").reserve_ms, 30_000, "a bare number is minutes");
    }

    #[test]
    fn format_seconds_trims() {
        let ms = Duration::from_millis;
        assert_eq!(format_seconds(ms(12_000)), "12");
        assert_eq!(format_seconds(ms(500)), "0.5");
        assert_eq!(format_seconds(ms(1_250)), "1.25");
        assert_eq!(format_seconds(ms(7)), "0.007");
        assert_eq!(format_seconds(Duration::from_micros(1_999)), "0.001", "rounded down to the ms");
    }

    #[test]
    fn setup_minimum() {
        let s = Duration::from_secs;
        let blitz = tc("15s/1m");
        assert_eq!(blitz.setup_minimum(), DEFAULT_SETUP_MINIMUM, "the text always means the minute");
        let engines = blitz.with_setup_minimum(Duration::ZERO);
        assert_eq!(engines.setup_move_time(), s(15));
        assert_eq!(engines.setup_allowance(s(60)), s(75));
        assert_eq!(engines.reserve_after_setup(s(60), s(5)), s(60), "unused time still isn't added");
        assert_eq!(engines.reserve_after_setup(s(60), s(20)), s(55), "overtime still uses the reserve");
        assert_eq!(engines.to_string(), "15s/1m", "not part of the text");
        assert_ne!(engines, blitz);
        assert_eq!(blitz.with_setup_minimum(s(30)).setup_move_time(), s(30));
        assert_eq!(TimeControl::parse_time("0").unwrap(), Duration::ZERO);
        assert_eq!(TimeControl::parse_time(" 10s ").unwrap(), s(10));
        assert_eq!(TimeControl::parse_time("1").unwrap(), s(60), "a bare number is minutes");
        assert!(TimeControl::parse_time("soon").is_err());
        assert_eq!(TimeControl::format_time(Duration::ZERO), "0");
        assert_eq!(TimeControl::format_time(s(90)), "1m30s");
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

    #[test]
    fn setup_clock() {
        let s = Duration::from_secs;
        // Under a minute per move: the setup gets a minute.
        let blitz = tc("15s/1m");
        assert_eq!(blitz.setup_move_time(), s(60));
        assert_eq!(blitz.setup_allowance(s(60)), s(120));
        assert_eq!(blitz.reserve_after_setup(s(60), s(20)), s(60), "unused time isn't added");
        assert_eq!(blitz.reserve_after_setup(s(60), s(75)), s(45), "overtime uses the reserve");
        assert_eq!(blitz.reserve_after_setup(s(10), s(90)), s(0));
        // A minute or more per move: the setup gets the move time.
        let slow = tc("2m/5m/50");
        assert_eq!(slow.setup_move_time(), s(120));
        assert_eq!(slow.setup_allowance(s(300)), s(420));
        assert_eq!(slow.reserve_after_setup(s(300), s(30)), s(300));
        assert_eq!(slow.reserve_after_setup(s(300), s(150)), s(270));
        // A turn time limit caps the reserve's share, not the minute.
        assert_eq!(tc("15s/1m/100/0/0/30s").setup_allowance(s(60)), s(60));
        assert_eq!(tc("15s/1m/100/0/0/90s").setup_allowance(s(60)), s(90));
    }
}
