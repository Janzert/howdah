//! Telling the user about a new release. It only asks GitHub for the
//! newest one and compares versions: how to install it depends on how the
//! app was installed, so the user is sent to the release's page.
//!
//! Packages that bring their own updates (Flatpak, a distribution's
//! package) build with `HOWDAH_NO_UPDATE_CHECK=1` in the environment:
//! then the app never asks GitHub, and Settings doesn't offer to.

use serde::Deserialize;

use crate::dto::{ApiError, UpdateView};
use crate::engine_install;

const LATEST_URL: &str = "https://api.github.com/repos/Janzert/howdah/releases/latest";
const MAX_REPLY_BYTES: usize = 1 << 20;

/// Whether this build checks for new releases: unless it was built with
/// `HOWDAH_NO_UPDATE_CHECK` set to anything but empty or `0`. (Cargo
/// rebuilds when that variable changes.)
pub const BUILT_IN: bool = match option_env!("HOWDAH_NO_UPDATE_CHECK") {
    None => true,
    Some(v) => matches!(v.as_bytes(), b"" | b"0"),
};

#[derive(Deserialize)]
struct Release {
    tag_name: String,
    html_url: String,
}

/// A version's numbers, from `0.6.1` or `v0.6.1`. `None` for anything else.
fn numbers(version: &str) -> Option<Vec<u64>> {
    let version = version.strip_prefix('v').unwrap_or(version);
    version.split('.').map(|n| n.parse().ok()).collect()
}

/// The release in a reply from GitHub, if it's newer than `current`.
fn newer_release(reply: &[u8], current: &str) -> Result<Option<UpdateView>, ApiError> {
    let release: Release = serde_json::from_slice(reply)
        .map_err(|e| ApiError::illegal(format!("couldn't read the newest release: {e}")))?;
    let version = release.tag_name.strip_prefix('v').unwrap_or(&release.tag_name);
    let newer = match (numbers(version), numbers(current)) {
        (Some(latest), Some(current)) => latest > current,
        _ => false,
    };
    Ok(newer.then(|| UpdateView { version: version.to_string(), url: release.html_url }))
}

/// The newest release, if it's newer than `current`; never one in a build
/// without the check. Debug builds never ask GitHub (the tests run on
/// them): `HOWDAH_UPDATE_RELEASE` stands in for its reply, to check the app.
pub async fn check(current: &str) -> Result<Option<UpdateView>, ApiError> {
    if !BUILT_IN {
        return Ok(None);
    }
    if cfg!(debug_assertions) {
        return match std::env::var("HOWDAH_UPDATE_RELEASE") {
            Ok(reply) => newer_release(reply.as_bytes(), current),
            Err(_) => Ok(None),
        };
    }
    let http = engine_install::http_client()?;
    newer_release(&engine_install::fetch(&http, LATEST_URL, MAX_REPLY_BYTES).await?, current)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn reply(tag: &str) -> Vec<u8> {
        format!(r#"{{"tag_name":"{tag}","html_url":"https://example.com/{tag}","assets":[]}}"#).into_bytes()
    }

    fn newer(tag: &str, current: &str) -> Option<String> {
        newer_release(&reply(tag), current).unwrap().map(|u| u.version)
    }

    #[test]
    fn a_newer_release_is_found() {
        assert_eq!(newer("v0.6.2", "0.6.1").as_deref(), Some("0.6.2"));
        assert_eq!(newer("v0.10.0", "0.9.3").as_deref(), Some("0.10.0"));
        assert_eq!(newer("1.0.0", "0.9.3").as_deref(), Some("1.0.0"));
    }

    #[test]
    fn the_same_or_an_older_release_is_not() {
        assert_eq!(newer("v0.6.1", "0.6.1"), None);
        assert_eq!(newer("v0.6.0", "0.6.1"), None);
    }

    #[test]
    fn a_tag_that_is_no_version_is_not() {
        assert_eq!(newer("nightly", "0.6.1"), None);
        assert_eq!(newer("v0.7.0-rc1", "0.6.1"), None);
    }

    #[test]
    fn the_release_page_is_given() {
        let update = newer_release(&reply("v0.7.0"), "0.6.1").unwrap().unwrap();
        assert_eq!(update.url, "https://example.com/v0.7.0");
    }

    #[test]
    fn an_unreadable_reply_is_an_error() {
        assert!(newer_release(b"<html>", "0.6.1").is_err());
    }
}
