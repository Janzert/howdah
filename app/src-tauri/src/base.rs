//! The base database: arimaa.com's game archive, built with `howdah-db
//! publish`) and published with its
//! manifest in the howdah-game-database repository's releases
//! (`docs/DATABASE.md`, "Step 5"). The app offers it in the Games panel:
//! it works out the files its database needs (`base::plan`: the delta
//! starting closest before the time it reached, else the base and the
//! delta after it), downloads each, checks it against the manifest,
//! unpacks it beside the game database and merges it in, and remembers
//! the time reached after each, so only newer games are offered later.
//!
//! The manifest is fetched at most once a day (`CHECK_EVERY`) and kept
//! in the database's `meta` between, so opening the Games panel or
//! restarting doesn't ask GitHub again; the panel's refresh button
//! checks at once, and installing always fetches it afresh.
//!
//! `HOWDAH_BASE_URL` names another manifest in debug builds, and may be
//! plain http to this computer, for checking the app against a local
//! server; `off` checks nothing, as the end-to-end tests do.

use std::path::Path;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

use howdah_db::base::{self, Manifest, Pack};
use serde::Serialize;
use ts_rs::TS;

use crate::backend::{Events, emit};
use crate::dto::ApiError;
use crate::engine_install;
use crate::library::Library;

/// Where the newest base database's manifest is: the howdah-game-database
/// repository's `current` release, whose files are swapped as new ones
/// are built. The files it names sit beside it, each with its date in
/// its name, so a new one is uploaded before the manifest naming it and
/// an old one removed only after the manifest stops naming it.
pub const MANIFEST_URL: &str =
    "https://github.com/Janzert/howdah-game-database/releases/download/current/base.json";

/// Event carrying [`BaseProgress`] while a base database is installed.
pub const BASE_PROGRESS: &str = "library://base";

const MAX_MANIFEST_BYTES: usize = 64 << 10;
/// Larger than any base is likely to be, as a guard against a bad manifest.
const MAX_DOWNLOAD_BYTES: u64 = 2 << 30;
const MAX_UNPACKED_BYTES: u64 = 8 << 30;

/// How often the manifest is fetched, at most, unless asked.
const CHECK_EVERY: i64 = 24 * 60 * 60;

/// The `meta` keys the last manifest fetched is kept under, with when
/// and from where.
const MANIFEST_KEPT: &str = "base_manifest";
const MANIFEST_CHECKED: &str = "base_manifest_checked";
const MANIFEST_FROM: &str = "base_manifest_url";

/// The `meta` keys the installed base is recorded under: the time
/// reached (the `at` of the last pack merged), and its games and date.
pub(crate) const BASE_AT: &str = "base_at";
const BASE_GAMES: &str = "base_games";
const BASE_THROUGH: &str = "base_through";

/// The base database the user has, and the newest one there is.
#[derive(Clone, Debug, Default, Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct BaseStatus {
    /// What the game database has from a base, if anything.
    pub installed: Option<BaseInfo>,
    /// The newest base, when the manifest could be read.
    pub available: Option<BaseInfo>,
    /// There's something newer than what's installed (or nothing is
    /// installed), and this version of Howdah can read it.
    pub offer: bool,
    /// Why the available one can't be installed, or the manifest read.
    pub problem: Option<String>,
    /// An install is running.
    pub installing: bool,
}

#[derive(Clone, Debug, Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct BaseInfo {
    #[ts(type = "number")]
    pub games: i64,
    /// The newest game's date, `YYYY.MM.DD`.
    pub through: Option<String>,
    /// What there is to download, in bytes (for the available one).
    #[ts(type = "number | null")]
    pub size: Option<u64>,
    /// When only deltas are needed, the games they add.
    #[ts(type = "number | null")]
    pub new_games: Option<i64>,
}

/// How an install is going (the `library://base` event).
#[derive(Clone, Debug, Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct BaseProgress {
    /// `downloading`, `checking`, `unpacking`, `merging`, `done` or `failed`.
    pub phase: String,
    /// Bytes downloaded so far, and in all.
    #[ts(type = "number")]
    pub done: u64,
    #[ts(type = "number")]
    pub total: u64,
    /// The error, when it failed; what was added, when it's done.
    pub detail: Option<String>,
}

/// The manifest's address, or `None` in a debug build with
/// `HOWDAH_BASE_URL` set to `off`.
fn manifest_url() -> Option<String> {
    match std::env::var("HOWDAH_BASE_URL") {
        Ok(url) if cfg!(debug_assertions) => (url != "off" && !url.is_empty()).then_some(url),
        _ => Some(MANIFEST_URL.into()),
    }
}

fn now() -> i64 {
    std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map_or(0, |d| d.as_secs() as i64)
}

/// The manifest kept from the last fetch from `url`, and when that was.
fn kept_manifest(library: &Library, url: &str) -> Option<(Manifest, i64)> {
    if library.meta(MANIFEST_FROM).as_deref() != Some(url) {
        return None;
    }
    let checked = library.meta(MANIFEST_CHECKED)?.parse().ok()?;
    let m = serde_json::from_str(&library.meta(MANIFEST_KEPT)?).ok()?;
    Some((m, checked))
}

/// Fetches the manifest and keeps it.
async fn fetch_and_keep(library: &Library, url: &str) -> Result<Manifest, ApiError> {
    let m = fetch_manifest(&http()?, url).await?;
    let json = serde_json::to_string(&m).map_err(|e| ApiError::state(e.to_string()))?;
    let kept = library.with_db_result(|db| {
        db.set_meta(MANIFEST_KEPT, &json)?;
        db.set_meta(MANIFEST_CHECKED, &now().to_string())?;
        db.set_meta(MANIFEST_FROM, url)
    });
    if let Err(e) = kept {
        eprintln!("couldn't keep the game database's manifest: {e}");
    }
    Ok(m)
}

/// Whether `url` may be fetched: https, or in a debug build plain http to
/// this computer.
fn allowed(url: &str) -> bool {
    let lower = url.to_ascii_lowercase();
    lower.starts_with("https://")
        || (cfg!(debug_assertions)
            && (lower.starts_with("http://127.0.0.1") || lower.starts_with("http://localhost")))
}

fn http() -> Result<reqwest::Client, ApiError> {
    if cfg!(debug_assertions) {
        // The release client refuses plain http; a local server is fine here.
        return reqwest::Client::builder()
            .user_agent(howdah_gameroom::user_agent())
            .build()
            .map_err(|e| ApiError::state(format!("couldn't set up downloads: {e}")));
    }
    engine_install::http_client()
}

async fn fetch_manifest(http: &reqwest::Client, url: &str) -> Result<Manifest, ApiError> {
    if !allowed(url) {
        return Err(ApiError::illegal(format!("only https addresses are used: {url}")));
    }
    let failed = |e: reqwest::Error| ApiError::state(format!("couldn't fetch {url}: {e}"));
    let resp = http.get(url).send().await.map_err(failed)?;
    if !resp.status().is_success() {
        return Err(ApiError::state(format!("couldn't fetch {url}: HTTP {}", resp.status().as_u16())));
    }
    let body = resp.bytes().await.map_err(failed)?;
    if body.len() > MAX_MANIFEST_BYTES {
        return Err(ApiError::state(format!("{url} is too large for a manifest")));
    }
    let m: Manifest = serde_json::from_slice(&body)
        .map_err(|e| ApiError::state(format!("the game database's manifest doesn't read: {e}")))?;
    if m.format != base::MANIFEST_FORMAT {
        return Err(ApiError::state(
            "the game database's manifest is in a newer format; update Howdah first",
        ));
    }
    let packs = || std::iter::once(&m.base).chain(&m.deltas);
    if packs().any(|p| p.file.contains(['/', '\\']) || p.file.is_empty()) {
        return Err(ApiError::state("the game database's manifest names a bad file"));
    }
    if packs().any(|p| p.size > MAX_DOWNLOAD_BYTES || p.unpacked_size > MAX_UNPACKED_BYTES) {
        return Err(ApiError::state("the game database's manifest gives an unlikely size"));
    }
    m.check().map_err(|e| ApiError::state(format!("the game database's manifest: {e}")))?;
    Ok(m)
}

/// The file's address: beside the manifest.
fn file_url(manifest_url: &str, file: &str) -> String {
    match manifest_url.rsplit_once('/') {
        Some((dir, _)) => format!("{dir}/{file}"),
        None => file.to_string(),
    }
}

pub struct Base {
    installing: AtomicBool,
}

impl Default for Base {
    fn default() -> Self {
        Base { installing: AtomicBool::new(false) }
    }
}

impl Base {
    /// What's installed and what's available: from the manifest kept
    /// from the last check if that was under a day ago and not `check`,
    /// else fetched (falling back on the kept one if that fails).
    pub async fn status(&self, library: &Library, check: bool) -> BaseStatus {
        let installed = library.meta(BASE_GAMES).and_then(|g| g.parse().ok()).map(|games| BaseInfo {
            games,
            through: library.meta(BASE_THROUGH).filter(|t| !t.is_empty()),
            size: None,
            new_games: None,
        });
        let mut status = BaseStatus {
            installed,
            installing: self.installing.load(Ordering::Relaxed),
            ..Default::default()
        };
        if !library.is_open() {
            status.problem = Some("there's no game database".into());
            return status;
        }
        let Some(url) = manifest_url() else { return status };
        let kept = kept_manifest(library, &url);
        let manifest = match kept {
            Some((m, checked)) if !check && now() - checked < CHECK_EVERY => Ok(m),
            _ => match fetch_and_keep(library, &url).await {
                Ok(m) => Ok(m),
                Err(e) => match kept {
                    Some((m, _)) => {
                        status.problem = Some(e.message);
                        Ok(m)
                    }
                    None => Err(e),
                },
            },
        };
        match manifest {
            Ok(m) => match plan(&m, library) {
                Ok(packs) => {
                    let only_deltas = packs.first().is_some_and(|p| p.since.is_some());
                    let had = status.installed.as_ref().map_or(0, |i| i.games);
                    status.available = Some(BaseInfo {
                        games: m.games,
                        through: m.through.clone(),
                        size: Some(packs.iter().map(|p| p.size).sum()),
                        // A delta may repeat games the database has.
                        new_games: only_deltas.then(|| (m.games - had).max(0)),
                    });
                    status.offer = !packs.is_empty();
                }
                Err(e) => status.problem = Some(e.message),
            },
            Err(e) => status.problem = Some(e.message),
        }
        status
    }

    /// Downloads, checks, unpacks and merges the newest base, sending
    /// [`BASE_PROGRESS`] events. Returns what was added, in words.
    pub async fn install(&self, library: Arc<Library>, events: Events) -> Result<String, ApiError> {
        if self.installing.swap(true, Ordering::AcqRel) {
            return Err(ApiError::state("the game database is being installed already"));
        }
        let result = install(&library, &events).await;
        self.installing.store(false, Ordering::Release);
        let progress = match &result {
            Ok(added) => {
                BaseProgress { phase: "done".into(), done: 0, total: 0, detail: Some(added.clone()) }
            }
            Err(e) => {
                BaseProgress { phase: "failed".into(), done: 0, total: 0, detail: Some(e.message.clone()) }
            }
        };
        emit(&events, BASE_PROGRESS, progress);
        result
    }
}

/// The packs the library needs, refusing any this build can't read.
fn plan(manifest: &Manifest, library: &Library) -> Result<Vec<Pack>, ApiError> {
    let at = library.meta(BASE_AT).and_then(|a| a.parse().ok());
    let packs = base::plan(manifest, at)
        .map_err(|e| ApiError::state(format!("the game database's manifest: {e}")))?;
    if packs.iter().any(|p| p.schema > howdah_db::schema::VERSION) {
        return Err(ApiError::state("the newest game database needs a newer Howdah"));
    }
    Ok(packs.into_iter().cloned().collect())
}

async fn install(library: &Arc<Library>, events: &Events) -> Result<String, ApiError> {
    let url = manifest_url().ok_or_else(|| ApiError::state("no game database to download in this build"))?;
    let dir = library
        .path()
        .and_then(Path::parent)
        .map(|d| d.join("base-download"))
        .ok_or_else(|| ApiError::state("there's no game database"))?;
    let http = http()?;
    let manifest = fetch_and_keep(library, &url).await?;
    let packs = plan(&manifest, library)?;
    if packs.is_empty() {
        return Ok("nothing new".into());
    }
    std::fs::create_dir_all(&dir).map_err(io("making a folder for the download"))?;
    let total: u64 = packs.iter().map(|p| p.size).sum();
    let result = async {
        let mut sum = howdah_db::MergeReport::default();
        let mut done = 0;
        for pack in packs {
            let packed = dir.join(&pack.file);
            download(&http, &file_url(&url, &pack.file), &packed, &pack, done, total, events).await?;
            done += pack.size;
            let unpacked = dir.join("base.db");
            let (lib, m, ev) = (library.clone(), manifest.clone(), events.clone());
            let report = tauri::async_runtime::spawn_blocking(move || {
                unpack_and_merge(&lib, &pack, &m, &packed, &unpacked, &ev)
            })
            .await
            .map_err(|e| ApiError::state(format!("installing the game database stopped: {e}")))??;
            sum.games_added += report.games_added;
            sum.games_filled += report.games_filled;
        }
        let mut added = format!("{} games added", thousands(sum.games_added));
        if sum.games_filled > 0 {
            added.push_str(&format!(", {} filled in", thousands(sum.games_filled)));
        }
        Ok(added)
    }
    .await;
    let _ = std::fs::remove_dir_all(&dir);
    result
}

fn io(what: &'static str) -> impl Fn(std::io::Error) -> ApiError {
    move |e| ApiError::state(format!("{what}: {e}"))
}

fn progress(events: &Events, phase: &str, done: u64, total: u64) {
    emit(events, BASE_PROGRESS, BaseProgress { phase: phase.into(), done, total, detail: None });
}

/// Streams `url` into `path`, refusing more than the pack's size. The
/// progress counts from `before`, out of `total`, for every file.
async fn download(
    http: &reqwest::Client,
    url: &str,
    path: &Path,
    pack: &Pack,
    before: u64,
    total: u64,
    events: &Events,
) -> Result<(), ApiError> {
    let size = pack.size;
    use std::io::Write;
    if !allowed(url) {
        return Err(ApiError::illegal(format!("only https addresses are used: {url}")));
    }
    let failed = |e: reqwest::Error| ApiError::state(format!("couldn't download the game database: {e}"));
    let mut resp = http.get(url).send().await.map_err(failed)?;
    if !resp.status().is_success() {
        return Err(ApiError::state(format!(
            "couldn't download the game database: HTTP {}",
            resp.status().as_u16()
        )));
    }
    let mut file = std::io::BufWriter::new(std::fs::File::create(path).map_err(io("saving the download"))?);
    let mut done = 0u64;
    let mut reported = 0u64;
    progress(events, "downloading", before, total);
    while let Some(chunk) = resp.chunk().await.map_err(failed)? {
        done += chunk.len() as u64;
        if done > size {
            return Err(ApiError::state("the download is larger than its manifest says"));
        }
        file.write_all(&chunk).map_err(io("saving the download"))?;
        // About a hundred updates in all.
        if done - reported >= total / 100 {
            reported = done;
            progress(events, "downloading", before + done, total);
        }
    }
    file.flush().map_err(io("saving the download"))?;
    Ok(())
}

fn unpack_and_merge(
    library: &Library,
    pack: &Pack,
    manifest: &Manifest,
    packed: &Path,
    unpacked: &Path,
    events: &Events,
) -> Result<howdah_db::MergeReport, ApiError> {
    let db = |e: howdah_db::Error| ApiError::state(format!("installing the game database: {e}"));
    progress(events, "checking", 0, 0);
    base::check(packed, pack).map_err(db)?;
    progress(events, "unpacking", 0, 0);
    base::unpack(packed, unpacked, pack.unpacked_size).map_err(db)?;
    let _ = std::fs::remove_file(packed);
    progress(events, "merging", 0, 0);
    let report = library.merge_base(unpacked, pack, manifest).map_err(db);
    let _ = std::fs::remove_file(unpacked);
    report
}

/// `425129` as `425,129`.
fn thousands(n: i64) -> String {
    let digits = n.unsigned_abs().to_string();
    let mut out = String::new();
    for (i, c) in digits.chars().enumerate() {
        if i > 0 && (digits.len() - i).is_multiple_of(3) {
            out.push(',');
        }
        out.push(c);
    }
    if n < 0 { format!("-{out}") } else { out }
}

impl Library {
    /// Merges an unpacked base or delta and records the time it reaches.
    pub fn merge_base(
        &self,
        path: &Path,
        pack: &Pack,
        manifest: &Manifest,
    ) -> howdah_db::Result<howdah_db::MergeReport> {
        // A base the manifest's newest games come after (its delta still
        // to come) has only its own count, and no date to give.
        let newest = pack.at >= manifest.at;
        let games = if newest { manifest.games } else { pack.games };
        let through = if newest { manifest.through.as_deref() } else { None };
        self.with_db_result(|db| {
            let report = db.merge_from(path, "base")?;
            db.set_meta(BASE_AT, &pack.at.to_string())?;
            db.set_meta(BASE_GAMES, &games.to_string())?;
            db.set_meta(BASE_THROUGH, through.unwrap_or(""))?;
            Ok(report)
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_file_is_beside_the_manifest() {
        assert_eq!(
            file_url("https://github.com/J/g/releases/download/current/base.json", "g.db.zst"),
            "https://github.com/J/g/releases/download/current/g.db.zst"
        );
    }

    #[test]
    fn counts_have_separators() {
        assert_eq!(thousands(425_129), "425,129");
        assert_eq!(thousands(999), "999");
        assert_eq!(thousands(1_000_000), "1,000,000");
    }

    #[test]
    fn only_https_or_local_http_in_debug() {
        assert!(allowed("https://github.com/x"));
        assert!(!allowed("http://example.com/x"));
        assert_eq!(allowed("http://127.0.0.1:8000/base.json"), cfg!(debug_assertions));
    }

    #[test]
    fn a_kept_manifest_is_used_only_for_its_address() {
        let library = Library::in_memory();
        let m: Manifest = serde_json::from_str(
            r#"{"format":1,"at":20,"games":2,"through":null,
                "base":{"file":"b","size":1,"sha256":"","unpackedSize":1,"schema":1,"games":2,"at":20}}"#,
        )
        .unwrap();
        assert!(kept_manifest(&library, "https://a/base.json").is_none());
        let json = serde_json::to_string(&m).unwrap();
        library
            .with_db_result(|db| {
                db.set_meta(MANIFEST_KEPT, &json)?;
                db.set_meta(MANIFEST_CHECKED, "123")?;
                db.set_meta(MANIFEST_FROM, "https://a/base.json")
            })
            .unwrap();
        assert_eq!(kept_manifest(&library, "https://a/base.json"), Some((m, 123)));
        assert!(kept_manifest(&library, "https://b/base.json").is_none());
    }
}
