//! Whether the system writes the time of day on a 12- or a 24-hour clock.
//!
//! The webview formats times by its language alone, which misses a
//! clock setting made apart from it: the desktop's clock setting or
//! `LC_TIME` on Linux, the regional time format on Windows, the 24-hour
//! switch on macOS. This asks the system; what it can't tell is left to
//! the UI, which falls back to the webview's own default.

use crate::dto::SystemClock;

/// What the system says. Cheap enough to ask each time a window opens.
pub fn system_clock() -> SystemClock {
    let (hour24, locale) = read();
    SystemClock { hour24, locale }
}

#[cfg(target_os = "linux")]
fn read() -> (Option<bool>, Option<String>) {
    let desktop = std::env::var("XDG_CURRENT_DESKTOP").unwrap_or_default();
    let var = |name: &str| std::env::var(name).ok().filter(|v| !v.is_empty());
    let lc_time = var("LC_ALL").or_else(|| var("LC_TIME")).or_else(|| var("LANG"));
    (desktop_hour24(&desktop), lc_time.as_deref().and_then(locale_tag))
}

/// The desktop's own clock setting, where it has one in GSettings. KDE
/// sets `LC_TIME` instead, and the others have none for the whole desktop.
#[cfg(target_os = "linux")]
fn desktop_hour24(desktop: &str) -> Option<bool> {
    use gtk::gio::prelude::*;
    use gtk::gio::{Settings, SettingsSchemaSource};

    let names: Vec<String> = desktop.split(':').map(str::to_ascii_lowercase).collect();
    let is = |name: &str| names.iter().any(|n| n == name);
    // Asking for a schema or key that isn't installed aborts, so look first.
    let has = |schema: &str, key: &str| {
        SettingsSchemaSource::default().and_then(|s| s.lookup(schema, true)).is_some_and(|s| s.has_key(key))
    };
    if is("x-cinnamon") || is("cinnamon") {
        let schema = "org.cinnamon.desktop.interface";
        return has(schema, "clock-use-24h").then(|| Settings::new(schema).boolean("clock-use-24h"));
    }
    if is("gnome") || is("unity") || is("budgie") {
        let schema = "org.gnome.desktop.interface";
        if has(schema, "clock-format") {
            return match Settings::new(schema).string("clock-format").as_str() {
                "24h" => Some(true),
                "12h" => Some(false),
                _ => None,
            };
        }
    }
    None
}

#[cfg(target_os = "windows")]
fn read() -> (Option<bool>, Option<String>) {
    #[link(name = "kernel32")]
    unsafe extern "system" {
        fn GetLocaleInfoEx(locale_name: *const u16, lc_type: u32, data: *mut u16, len: i32) -> i32;
    }
    const LOCALE_STIMEFORMAT: u32 = 0x1003;
    let mut buf = [0u16; 80]; // the documented maximum for this value
    // SAFETY: a null name is the user's default locale, and `buf` is
    // writable for the length given.
    let n =
        unsafe { GetLocaleInfoEx(std::ptr::null(), LOCALE_STIMEFORMAT, buf.as_mut_ptr(), buf.len() as i32) };
    if n <= 0 {
        return (None, None);
    }
    let picture = String::from_utf16_lossy(&buf[..n as usize - 1]);
    (picture_hour24(&picture), None)
}

/// Reads a Windows time picture (`h:mm:ss tt`, `HH:mm:ss`): `H` is the
/// 24-hour clock's hour and `h` the 12-hour clock's. Text in single
/// quotes is literal.
#[cfg(any(target_os = "windows", test))]
fn picture_hour24(picture: &str) -> Option<bool> {
    let mut quoted = false;
    for c in picture.chars() {
        match c {
            '\'' => quoted = !quoted,
            'H' if !quoted => return Some(true),
            'h' if !quoted => return Some(false),
            _ => {}
        }
    }
    None
}

#[cfg(target_os = "macos")]
fn read() -> (Option<bool>, Option<String>) {
    // The global preferences: the 24-hour switch in Date & Time writes
    // one of the two `Force` keys when it differs from the region's own
    // clock, and `AppleLocale` is the region.
    let global = |key: &str| {
        let out = std::process::Command::new("/usr/bin/defaults").args(["read", "-g", key]).output().ok()?;
        out.status.success().then(|| String::from_utf8_lossy(&out.stdout).trim().to_string())
    };
    let forced = |key: &str| global(key).is_some_and(|v| v == "1");
    let hour24 = if forced("AppleICUForce24HourTime") {
        Some(true)
    } else if forced("AppleICUForce12HourTime") {
        Some(false)
    } else {
        None
    };
    (hour24, global("AppleLocale").as_deref().and_then(locale_tag))
}

#[cfg(not(any(target_os = "linux", target_os = "windows", target_os = "macos")))]
fn read() -> (Option<bool>, Option<String>) {
    (None, None)
}

/// A POSIX or Apple locale name (`de_DE.UTF-8`, `sr_RS@latin`,
/// `en_US@calendar=gregorian`) as the language tag the UI can format by
/// (`de-DE`). `C` and `POSIX` name no place, so they give none.
#[cfg(any(not(target_os = "windows"), test))]
fn locale_tag(name: &str) -> Option<String> {
    let base = name.split(['.', '@']).next().unwrap_or_default();
    let ok = !base.is_empty()
        && !matches!(base, "C" | "POSIX")
        && base.split('_').all(|p| !p.is_empty() && p.chars().all(|c| c.is_ascii_alphanumeric()));
    ok.then(|| base.replace('_', "-"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn locale_names_become_language_tags() {
        assert_eq!(locale_tag("de_DE.UTF-8").as_deref(), Some("de-DE"));
        assert_eq!(locale_tag("sr_RS@latin").as_deref(), Some("sr-RS"));
        assert_eq!(locale_tag("en_US@calendar=gregorian").as_deref(), Some("en-US"));
        assert_eq!(locale_tag("fr").as_deref(), Some("fr"));
        assert_eq!(locale_tag("C"), None);
        assert_eq!(locale_tag("C.UTF-8"), None);
        assert_eq!(locale_tag("POSIX"), None);
        assert_eq!(locale_tag(""), None);
        assert_eq!(locale_tag("../etc"), None);
    }

    #[test]
    fn windows_time_pictures() {
        assert_eq!(picture_hour24("h:mm:ss tt"), Some(false));
        assert_eq!(picture_hour24("HH:mm:ss"), Some(true));
        assert_eq!(picture_hour24("'Hour' h tt"), Some(false));
        assert_eq!(picture_hour24("mm:ss"), None);
    }

    /// Whatever this machine says, asking mustn't fail.
    #[test]
    fn asks_the_system() {
        let clock = system_clock();
        if let Some(locale) = clock.locale {
            assert!(!locale.is_empty());
        }
    }

    #[cfg(target_os = "linux")]
    #[test]
    fn other_desktops_have_no_setting() {
        assert_eq!(desktop_hour24("KDE"), None);
        assert_eq!(desktop_hour24(""), None);
    }
}
