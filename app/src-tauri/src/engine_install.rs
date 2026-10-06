//! Installing engines from manifests (AEI's `ENGINE_MANIFEST.md`; the plan
//! is in `docs/ENGINES.md`). The catalog keeps the manifests the user has
//! added, with where each came from, in `engine-manifests.json`; installing
//! downloads this platform's file, checks its digest, unpacks it into its
//! own directory (`<root>/<id>/<version>/`) and registers the engine.

use std::io::{Cursor, Read};
use std::path::{Path, PathBuf};
use std::time::Duration;

use howdah_aei::manifest::{Archive, Download, Manifest, is_safe_relative_path, platform};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

use crate::dto::{
    ApiError, EngineCatalogView, EngineSpec, InstalledFrom, ManifestOptionView, ManifestView, SuggestedEngine,
};
use crate::engines::EngineRegistry;

/// Manifests offered before the user adds any: the engines Howdah was made
/// with, at the address of their newest release's manifest.
pub const SUGGESTED: &[(&str, &str)] = &[
    ("Sharp", "https://github.com/Janzert/arimaasharp/releases/latest/download/engine.json"),
    ("OpFor", "https://github.com/Janzert/OpFor/releases/latest/download/engine.json"),
];

/// Largest manifest and engine file fetched.
const MAX_MANIFEST_BYTES: usize = 1 << 20;
const MAX_DOWNLOAD_BYTES: usize = 512 << 20;

/// A manifest the user added, with the URL it came from.
#[derive(Clone, Debug, Serialize, Deserialize)]
struct Entry {
    manifest: Manifest,
    /// Where it was fetched; `None` when it came from a file.
    #[serde(default)]
    source: Option<String>,
}

pub struct EngineCatalog {
    path: PathBuf,
    /// Where engines are installed.
    root: PathBuf,
    entries: Vec<Entry>,
    http: Option<reqwest::Client>,
}

impl EngineCatalog {
    /// Loads the catalog from `path` (empty if missing or unreadable);
    /// engines go under `root`.
    pub fn load(path: PathBuf, root: PathBuf) -> EngineCatalog {
        let entries = std::fs::read_to_string(&path)
            .ok()
            .and_then(|text| serde_json::from_str(&text).ok())
            .unwrap_or_default();
        EngineCatalog { path, root, entries, http: None }
    }

    /// The manifests, as the Engines dialog shows them, with what's
    /// installed from each in `engines`.
    pub fn view(&self, engines: &[EngineSpec]) -> EngineCatalogView {
        let manifests: Vec<ManifestView> = self.entries.iter().map(|e| manifest_view(e, engines)).collect();
        let suggested = SUGGESTED
            .iter()
            .filter(|(_, url)| !self.entries.iter().any(|e| e.source.as_deref() == Some(*url)))
            .map(|(name, url)| SuggestedEngine { name: name.to_string(), url: url.to_string() })
            .collect();
        EngineCatalogView { platform: platform(), manifests, suggested }
    }

    fn manifest(&self, id: &str) -> Result<&Entry, ApiError> {
        self.entries
            .iter()
            .find(|e| e.manifest.id == id)
            .ok_or_else(|| ApiError::state(format!("no engine manifest {id:?}")))
    }

    /// Adds a manifest, replacing one with the same id (a newer release).
    fn add(&mut self, manifest: Manifest, source: Option<String>) -> Result<(), ApiError> {
        // A manifest read from a file keeps the URL of the one it replaces.
        let source = source.or_else(|| {
            self.entries.iter().find(|e| e.manifest.id == manifest.id).and_then(|e| e.source.clone())
        });
        self.entries.retain(|e| e.manifest.id != manifest.id);
        self.entries.push(Entry { manifest, source });
        self.write()
    }

    /// Forgets a manifest. An engine installed from it stays.
    pub fn remove(&mut self, id: &str) -> Result<(), ApiError> {
        self.entries.retain(|e| e.manifest.id != id);
        self.write()
    }

    /// Adds a manifest's text (from a file).
    pub fn add_text(&mut self, text: &str) -> Result<String, ApiError> {
        let manifest = Manifest::parse(text).map_err(ApiError::illegal)?;
        let id = manifest.id.clone();
        self.add(manifest, None)?;
        Ok(id)
    }

    fn write(&self) -> Result<(), ApiError> {
        let io = |e: std::io::Error| ApiError::state(format!("couldn't save the engine manifests: {e}"));
        if let Some(dir) = self.path.parent() {
            std::fs::create_dir_all(dir).map_err(io)?;
        }
        let json = serde_json::to_string_pretty(&self.entries).expect("manifests serialize");
        std::fs::write(&self.path, json).map_err(io)
    }

    fn http(&mut self) -> Result<reqwest::Client, ApiError> {
        if self.http.is_none() {
            self.http = Some(http_client()?);
        }
        Ok(self.http.clone().expect("just set"))
    }

    /// Where to fetch a manifest's newest release from.
    fn update_url(&self, id: &str) -> Result<String, ApiError> {
        let e = self.manifest(id)?;
        e.manifest
            .update_url
            .clone()
            .or_else(|| e.source.clone())
            .ok_or_else(|| ApiError::state(format!("{} doesn't say where its updates are", e.manifest.name)))
    }
}

/// The client for manifests and engine files: https only, also when
/// following redirects (GitHub's `releases/latest` redirects twice).
fn http_client() -> Result<reqwest::Client, ApiError> {
    let policy = reqwest::redirect::Policy::custom(|attempt| {
        if attempt.url().scheme() != "https" {
            attempt.error("a redirect left https")
        } else if attempt.previous().len() >= 10 {
            attempt.error("too many redirects")
        } else {
            attempt.follow()
        }
    });
    reqwest::Client::builder()
        .user_agent(howdah_gameroom::user_agent())
        .redirect(policy)
        .connect_timeout(Duration::from_secs(20))
        .timeout(Duration::from_secs(600))
        .build()
        .map_err(|e| ApiError::state(format!("couldn't set up downloads: {e}")))
}

/// Fetches `url` (https only), up to `max` bytes.
async fn fetch(http: &reqwest::Client, url: &str, max: usize) -> Result<Vec<u8>, ApiError> {
    if !url.to_ascii_lowercase().starts_with("https://") {
        return Err(ApiError::illegal(format!("only https addresses are used: {url}")));
    }
    let failed = |e: reqwest::Error| ApiError::state(format!("couldn't fetch {url}: {e}"));
    let mut resp = http.get(url).send().await.map_err(failed)?;
    if !resp.status().is_success() {
        return Err(ApiError::state(format!("couldn't fetch {url}: HTTP {}", resp.status().as_u16())));
    }
    let too_big = || ApiError::state(format!("{url} is larger than {} MB", max >> 20));
    if resp.content_length().is_some_and(|n| n > max as u64) {
        return Err(too_big());
    }
    let mut body = Vec::new();
    while let Some(chunk) = resp.chunk().await.map_err(failed)? {
        if body.len() + chunk.len() > max {
            return Err(too_big());
        }
        body.extend_from_slice(&chunk);
    }
    Ok(body)
}

/// Fetches and reads the manifest at `url`.
pub async fn fetch_manifest(http: &reqwest::Client, url: &str) -> Result<Manifest, ApiError> {
    let body = fetch(http, url, MAX_MANIFEST_BYTES).await?;
    let text = String::from_utf8(body).map_err(|_| ApiError::illegal(format!("{url} isn't a text file")))?;
    Manifest::parse(&text).map_err(ApiError::illegal)
}

/// Adds the manifest at `url`, replacing one with the same id. Returns its id.
pub async fn add_url(catalog: &std::sync::Mutex<EngineCatalog>, url: &str) -> Result<String, ApiError> {
    let url = url.trim();
    let http = lock(catalog).http()?;
    let manifest = fetch_manifest(&http, url).await?;
    let id = manifest.id.clone();
    lock(catalog).add(manifest, Some(url.to_string()))?;
    Ok(id)
}

/// Fetches manifest `id` again from where its updates are published, so a
/// newer release shows (nothing is installed).
pub async fn refresh(catalog: &std::sync::Mutex<EngineCatalog>, id: &str) -> Result<(), ApiError> {
    let (url, http) = {
        let mut c = lock(catalog);
        (c.update_url(id)?, c.http()?)
    };
    let manifest = fetch_manifest(&http, &url).await?;
    if manifest.id != id {
        return Err(ApiError::illegal(format!("{url} describes {:?}, not {id:?}", manifest.id)));
    }
    let mut c = lock(catalog);
    let source = c.manifest(id)?.source.clone();
    c.add(manifest, source)
}

/// Installs manifest `id`'s release for this platform and registers it in
/// `registry`: a new engine, or the one installed from the same manifest
/// before, updated (keeping its id and options). Returns the engine.
pub async fn install(
    catalog: &std::sync::Mutex<EngineCatalog>,
    registry: &std::sync::Mutex<EngineRegistry>,
    id: &str,
) -> Result<EngineSpec, ApiError> {
    let (manifest, root, http) = {
        let mut c = lock(catalog);
        (c.manifest(id)?.manifest.clone(), c.root.clone(), c.http()?)
    };
    let download = manifest.download().cloned().ok_or_else(|| {
        ApiError::state(format!("{} has no download for this computer ({})", manifest.name, platform()))
    })?;
    let bytes = fetch(&http, &download.url, MAX_DOWNLOAD_BYTES).await?;
    // Unpacking writes many files; keep it off the async threads.
    let dir = version_dir(&root, &manifest);
    let download2 = download.clone();
    let program = tokio::task::spawn_blocking(move || unpack_verified(&bytes, &download2, &dir))
        .await
        .map_err(|e| ApiError::state(format!("installing stopped: {e}")))??;
    let mut registry = lock(registry);
    register(&mut registry, &manifest, &download, &program)
}

/// `<root>/<id>/<version>`, with both made safe as directory names.
fn version_dir(root: &Path, manifest: &Manifest) -> PathBuf {
    root.join(dir_name(&manifest.id)).join(dir_name(&manifest.version))
}

fn dir_name(s: &str) -> String {
    let name: String =
        s.chars().map(|c| if c.is_ascii_alphanumeric() || "-_.".contains(c) { c } else { '_' }).collect();
    // Not `.` or `..`, nor hidden.
    if name.starts_with('.') { format!("_{name}") } else { name }
}

/// Adds the installed engine to the registry, or updates the one installed
/// from the same manifest.
fn register(
    registry: &mut EngineRegistry,
    manifest: &Manifest,
    download: &Download,
    program: &Path,
) -> Result<EngineSpec, ApiError> {
    let installed = InstalledFrom { manifest: manifest.id.clone(), version: manifest.version.clone() };
    let existing =
        registry.list().into_iter().find(|e| e.installed.as_ref().is_some_and(|i| i.manifest == manifest.id));
    let spec = EngineSpec {
        id: existing.as_ref().map(|e| e.id.clone()).unwrap_or_default(),
        name: existing.as_ref().map_or_else(|| manifest.name.clone(), |e| e.name.clone()),
        program: program.display().to_string(),
        args: manifest.args_for(download).to_vec(),
        working_dir: program.parent().map(|d| d.display().to_string()),
        options: existing.map(|e| e.options).unwrap_or_default(),
        installed: Some(installed),
    };
    registry.save(spec)
}

/// Checks `bytes` against the download's digest, unpacks them into `dir`
/// (replacing what's there) and returns the program's path.
fn unpack_verified(bytes: &[u8], download: &Download, dir: &Path) -> Result<PathBuf, ApiError> {
    let digest = hex(&Sha256::digest(bytes));
    if !digest.eq_ignore_ascii_case(&download.sha256) {
        return Err(ApiError::illegal(format!(
            "the download doesn't match its manifest (SHA-256 {digest}, expected {})",
            download.sha256
        )));
    }
    let io = |what: &str, e: std::io::Error| ApiError::state(format!("couldn't {what}: {e}"));
    let parent = dir.parent().ok_or_else(|| ApiError::state("no install directory"))?;
    std::fs::create_dir_all(parent).map_err(|e| io("create the engine directory", e))?;
    // Unpack beside the final directory, then swap it in, so a failure
    // leaves no half-installed engine.
    let staging = parent.join(format!(".installing-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&staging);
    std::fs::create_dir_all(&staging).map_err(|e| io("create the engine directory", e))?;
    let result = unpack(bytes, download, &staging);
    if let Err(e) = result {
        let _ = std::fs::remove_dir_all(&staging);
        return Err(e);
    }
    if dir.exists() {
        std::fs::remove_dir_all(dir).map_err(|e| io("replace the earlier copy", e))?;
    }
    std::fs::rename(&staging, dir).map_err(|e| io("finish installing", e))?;
    let program = dir.join(download.program_path());
    if !program.is_file() {
        return Err(ApiError::illegal(format!(
            "the download has no program at {:?}",
            download.program_path()
        )));
    }
    make_executable(&program).map_err(|e| io("make the program executable", e))?;
    Ok(program)
}

fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}

/// Writes the download's files into `dir`.
fn unpack(bytes: &[u8], download: &Download, dir: &Path) -> Result<(), ApiError> {
    match download.archive {
        None => write_file(dir, &download.program_path(), bytes, None),
        Some(Archive::Zip) => unpack_zip(bytes, dir),
        Some(Archive::TarGz) => unpack_tar_gz(bytes, dir),
    }
}

fn bad_archive(msg: impl std::fmt::Display) -> ApiError {
    ApiError::illegal(format!("the download's archive can't be used: {msg}"))
}

/// An archive entry's path, made relative (a leading `./` dropped), or an
/// error when it would leave the directory.
fn entry_path(raw: &str) -> Result<String, ApiError> {
    let mut path = raw.trim_end_matches('/');
    while let Some(rest) = path.strip_prefix("./") {
        path = rest;
    }
    if !is_safe_relative_path(path) {
        return Err(bad_archive(format!("an entry would be written outside its directory: {raw:?}")));
    }
    Ok(path.to_string())
}

fn write_file(dir: &Path, rel: &str, contents: &[u8], mode: Option<u32>) -> Result<(), ApiError> {
    let path = dir.join(rel);
    let io = |e: std::io::Error| ApiError::state(format!("couldn't write {rel}: {e}"));
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).map_err(io)?;
    }
    std::fs::write(&path, contents).map_err(io)?;
    #[cfg(unix)]
    if let Some(mode) = mode {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(&path, std::fs::Permissions::from_mode(mode & 0o755)).map_err(io)?;
    }
    let _ = mode;
    Ok(())
}

fn unpack_zip(bytes: &[u8], dir: &Path) -> Result<(), ApiError> {
    let mut zip = zip::ZipArchive::new(Cursor::new(bytes)).map_err(bad_archive)?;
    for i in 0..zip.len() {
        let mut file = zip.by_index(i).map_err(bad_archive)?;
        let rel = entry_path(file.name())?;
        if file.is_symlink() {
            return Err(bad_archive(format!("links aren't allowed: {rel}")));
        }
        if file.is_dir() {
            std::fs::create_dir_all(dir.join(&rel))
                .map_err(|e| ApiError::state(format!("couldn't create {rel}: {e}")))?;
            continue;
        }
        let mut contents = Vec::new();
        file.read_to_end(&mut contents).map_err(bad_archive)?;
        write_file(dir, &rel, &contents, file.unix_mode())?;
    }
    Ok(())
}

fn unpack_tar_gz(bytes: &[u8], dir: &Path) -> Result<(), ApiError> {
    let mut tar = tar::Archive::new(flate2::read::GzDecoder::new(bytes));
    for entry in tar.entries().map_err(bad_archive)? {
        let mut entry = entry.map_err(bad_archive)?;
        let kind = entry.header().entry_type();
        let raw = entry.path().map_err(bad_archive)?.to_string_lossy().replace('\\', "/");
        // The archive's own top directory (`.` or `./`).
        if kind.is_dir() && raw.trim_start_matches("./").trim_end_matches('/').trim_matches('.').is_empty() {
            continue;
        }
        let rel = entry_path(&raw)?;
        if kind.is_dir() {
            std::fs::create_dir_all(dir.join(&rel))
                .map_err(|e| ApiError::state(format!("couldn't create {rel}: {e}")))?;
        } else if kind.is_file() {
            let mut contents = Vec::new();
            entry.read_to_end(&mut contents).map_err(bad_archive)?;
            write_file(dir, &rel, &contents, entry.header().mode().ok())?;
        } else if kind.is_pax_global_extensions() || kind.is_pax_local_extensions() {
            // Metadata about other entries.
        } else {
            return Err(bad_archive(format!("only files and directories are allowed: {rel}")));
        }
    }
    Ok(())
}

fn make_executable(path: &Path) -> std::io::Result<()> {
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let mode = std::fs::metadata(path)?.permissions().mode();
        std::fs::set_permissions(path, std::fs::Permissions::from_mode(mode | 0o755))?;
    }
    let _ = path;
    Ok(())
}

fn manifest_view(e: &Entry, engines: &[EngineSpec]) -> ManifestView {
    let m = &e.manifest;
    let engine = engines.iter().find(|s| s.installed.as_ref().is_some_and(|i| i.manifest == m.id));
    ManifestView {
        id: m.id.clone(),
        name: m.name.clone(),
        version: m.version.clone(),
        author: m.author.clone(),
        description: m.description.clone(),
        homepage: m.homepage.clone(),
        license: m.license.clone(),
        source: e.source.clone(),
        updatable: m.update_url.is_some() || e.source.is_some(),
        downloadable: m.download().is_some(),
        installed_version: engine.and_then(|s| s.installed.as_ref()).map(|i| i.version.clone()),
        engine_id: engine.map(|s| s.id.clone()),
        options: m.options.iter().map(ManifestOptionView::from).collect(),
    }
}

fn lock<T>(m: &std::sync::Mutex<T>) -> std::sync::MutexGuard<'_, T> {
    m.lock().unwrap_or_else(|e| e.into_inner())
}

#[cfg(test)]
mod tests {
    use std::io::Write;

    use super::*;

    fn digest(bytes: &[u8]) -> String {
        hex(&Sha256::digest(bytes))
    }

    fn download(bytes: &[u8], archive: Option<Archive>, program: Option<&str>) -> Download {
        Download {
            url: "https://example.com/engine".into(),
            sha256: digest(bytes),
            archive,
            program: program.map(String::from),
            args: None,
        }
    }

    fn tar_gz(entries: &[(&str, &[u8], u32)]) -> Vec<u8> {
        let gz = flate2::write::GzEncoder::new(Vec::new(), flate2::Compression::fast());
        let mut b = tar::Builder::new(gz);
        for (path, data, mode) in entries {
            let mut h = tar::Header::new_gnu();
            h.set_size(data.len() as u64);
            h.set_mode(*mode);
            h.set_entry_type(tar::EntryType::Regular);
            // Written raw, so the tests can include paths `set_path` refuses.
            h.as_old_mut().name[..path.len()].copy_from_slice(path.as_bytes());
            h.set_cksum();
            b.append(&h, *data).unwrap();
        }
        b.into_inner().unwrap().finish().unwrap()
    }

    fn zip(entries: &[(&str, &[u8])]) -> Vec<u8> {
        let mut w = zip::ZipWriter::new(Cursor::new(Vec::new()));
        for (path, data) in entries {
            w.start_file(*path, zip::write::SimpleFileOptions::default().unix_permissions(0o755)).unwrap();
            w.write_all(data).unwrap();
        }
        w.finish().unwrap().into_inner()
    }

    #[test]
    fn unpacks_a_tar_gz_with_a_leading_dot() {
        let tmp = tempfile::tempdir().unwrap();
        let bytes = tar_gz(&[("./opfor/bot_opfor", b"#!/bin/sh\n", 0o755), ("./opfor/README", b"hi", 0o644)]);
        let dir = tmp.path().join("opfor/1.0");
        let program =
            unpack_verified(&bytes, &download(&bytes, Some(Archive::TarGz), Some("opfor/bot_opfor")), &dir)
                .unwrap();
        assert_eq!(program, dir.join("opfor/bot_opfor"));
        assert_eq!(std::fs::read(dir.join("opfor/README")).unwrap(), b"hi");
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            assert_eq!(std::fs::metadata(&program).unwrap().permissions().mode() & 0o111, 0o111);
        }
    }

    #[test]
    fn unpacks_a_zip_and_a_bare_program() {
        let tmp = tempfile::tempdir().unwrap();
        let bytes = zip(&[("sharp/sharp.exe", b"MZ"), ("sharp/LICENSE", b"BSD")]);
        let dir = tmp.path().join("sharp/1");
        let program =
            unpack_verified(&bytes, &download(&bytes, Some(Archive::Zip), Some("sharp/sharp.exe")), &dir)
                .unwrap();
        assert_eq!(std::fs::read(program).unwrap(), b"MZ");
        assert_eq!(std::fs::read(dir.join("sharp/LICENSE")).unwrap(), b"BSD");

        let bytes = b"\x7fELF...";
        let dir = tmp.path().join("bare/1");
        let program = unpack_verified(bytes, &download(bytes, None, None), &dir).unwrap();
        assert_eq!(program, dir.join("engine"), "named after the url");
    }

    #[test]
    fn refuses_a_wrong_digest_and_leaves_nothing() {
        let tmp = tempfile::tempdir().unwrap();
        let bytes = b"engine";
        let mut d = download(bytes, None, Some("bot"));
        d.sha256 = digest(b"something else");
        let dir = tmp.path().join("bot/1");
        let err = unpack_verified(bytes, &d, &dir).unwrap_err();
        assert!(err.to_string().contains("doesn't match"), "{err}");
        assert!(!dir.exists());
    }

    #[test]
    fn refuses_entries_that_leave_the_directory() {
        let tmp = tempfile::tempdir().unwrap();
        for bytes in [
            tar_gz(&[("../evil", b"x", 0o644), ("bot", b"x", 0o755)]),
            tar_gz(&[("/etc/evil", b"x", 0o644), ("bot", b"x", 0o755)]),
        ] {
            let dir = tmp.path().join("t/1");
            let err = unpack_verified(&bytes, &download(&bytes, Some(Archive::TarGz), Some("bot")), &dir);
            assert!(err.is_err());
            assert!(
                !dir.exists() && !tmp.path().join("evil").exists() && !tmp.path().join("t/evil").exists()
            );
        }
        let bytes = zip(&[("a/../../evil", b"x"), ("bot", b"x")]);
        let dir = tmp.path().join("z/1");
        assert!(unpack_verified(&bytes, &download(&bytes, Some(Archive::Zip), Some("bot")), &dir).is_err());
        assert!(!tmp.path().join("evil").exists());
    }

    #[test]
    fn refuses_links_in_a_tar() {
        let gz = flate2::write::GzEncoder::new(Vec::new(), flate2::Compression::fast());
        let mut b = tar::Builder::new(gz);
        let mut h = tar::Header::new_gnu();
        h.set_entry_type(tar::EntryType::Symlink);
        h.set_size(0);
        b.append_link(&mut h, "bot", "/bin/sh").unwrap();
        let bytes = b.into_inner().unwrap().finish().unwrap();
        let tmp = tempfile::tempdir().unwrap();
        let dir = tmp.path().join("l/1");
        assert!(unpack_verified(&bytes, &download(&bytes, Some(Archive::TarGz), Some("bot")), &dir).is_err());
    }

    #[test]
    fn a_missing_program_is_an_error() {
        let tmp = tempfile::tempdir().unwrap();
        let bytes = tar_gz(&[("other", b"x", 0o755)]);
        let dir = tmp.path().join("m/1");
        let err =
            unpack_verified(&bytes, &download(&bytes, Some(Archive::TarGz), Some("bot")), &dir).unwrap_err();
        assert!(err.to_string().contains("no program"), "{err}");
    }

    fn manifest(version: &str) -> Manifest {
        Manifest::parse(&format!(
            r#"{{ "manifest_version": 1, "id": "github.com/x/bot", "name": "Bot", "version": "{version}",
                 "args": ["aei"], "downloads": {{ "{}": {{ "url": "https://example.com/bot",
                 "sha256": "{}" }} }} }}"#,
            platform(),
            "0".repeat(64)
        ))
        .unwrap()
    }

    #[test]
    fn registering_again_updates_the_same_engine() {
        let tmp = tempfile::tempdir().unwrap();
        let mut registry = EngineRegistry::load(tmp.path().join("engines.json"));
        let m1 = manifest("1.0");
        let d = m1.download().unwrap().clone();
        let first = register(&mut registry, &m1, &d, &tmp.path().join("bot/1.0/bot")).unwrap();
        assert_eq!(first.args, ["aei"]);
        assert_eq!(first.working_dir.as_deref(), Some(tmp.path().join("bot/1.0").to_str().unwrap()));
        // The user renames it and sets an option; an update keeps both.
        let mut edited = first.clone();
        edited.name = "My bot".into();
        edited.options = vec![crate::dto::EngineOption { name: "hash".into(), value: "64".into() }];
        registry.save(edited).unwrap();
        let m2 = manifest("2.0");
        let second = register(&mut registry, &m2, &d, &tmp.path().join("bot/2.0/bot")).unwrap();
        assert_eq!(second.id, first.id);
        assert_eq!(second.name, "My bot");
        assert_eq!(second.options.len(), 1);
        assert_eq!(second.installed.unwrap().version, "2.0");
        assert_eq!(registry.list().len(), 1);
    }

    #[test]
    fn the_catalog_keeps_one_entry_per_id_and_its_source() {
        let tmp = tempfile::tempdir().unwrap();
        let path = tmp.path().join("engine-manifests.json");
        let mut c = EngineCatalog::load(path.clone(), tmp.path().join("engines"));
        c.add(manifest("1.0"), Some("https://example.com/engine.json".into())).unwrap();
        // A newer release read from a file keeps the URL it came from.
        let text = serde_json::to_string(&manifest("2.0")).unwrap();
        c.add_text(&text).unwrap();
        let c = EngineCatalog::load(path, tmp.path().join("engines"));
        let view = c.view(&[]);
        assert_eq!(view.manifests.len(), 1);
        assert_eq!(view.manifests[0].version, "2.0");
        assert_eq!(view.manifests[0].source.as_deref(), Some("https://example.com/engine.json"));
        assert!(view.manifests[0].downloadable);
        assert_eq!(view.suggested.len(), SUGGESTED.len());
    }

    #[test]
    fn directory_names_are_safe() {
        assert_eq!(dir_name("github.com/Janzert/OpFor"), "github.com_Janzert_OpFor");
        assert_eq!(dir_name(".."), "_..");
        assert_eq!(dir_name("1.0 beta"), "1.0_beta");
    }
}
