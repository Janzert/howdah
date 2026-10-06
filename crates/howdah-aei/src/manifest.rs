//! Engine manifests: one release of an AEI engine described in JSON, with
//! where to download it for each platform, how to start it and the options
//! it takes. The format is AEI's `ENGINE_MANIFEST.md`.
//!
//! This module reads and checks manifests; fetching and unpacking the files
//! is the caller's business.

use std::cmp::Ordering;
use std::collections::{BTreeMap, HashSet};

use serde::{Deserialize, Serialize};

/// The manifest format version this reads.
pub const MANIFEST_VERSION: u32 = 1;

/// One release of an engine.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Manifest {
    pub manifest_version: u32,
    /// Stable across releases (`github.com/Janzert/OpFor`).
    pub id: String,
    pub name: String,
    pub version: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub author: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub homepage: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub license: Option<String>,
    /// Where the newest manifest is published.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub update_url: Option<String>,
    /// Arguments that start the engine in AEI mode.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub args: Vec<String>,
    /// The files, keyed by platform (`linux-x86_64`; see [`platform`]).
    /// Empty for an engine installed some other way (a developer's own
    /// build): no download is offered.
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub downloads: BTreeMap<String, Download>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub options: Vec<OptionSpec>,
}

/// The file for one platform.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Download {
    pub url: String,
    /// The file's SHA-256 digest in hex.
    pub sha256: String,
    /// Absent when the file is the executable itself.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub archive: Option<Archive>,
    /// The executable's path inside an archive (`/` separators), or the
    /// name to save a bare executable as.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub program: Option<String>,
    /// Replaces the manifest's `args` on this platform.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub args: Option<Vec<String>>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum Archive {
    #[serde(rename = "zip")]
    Zip,
    #[serde(rename = "tar.gz")]
    TarGz,
}

/// An option the engine takes with `setoption`.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct OptionSpec {
    pub name: String,
    #[serde(flatten)]
    pub kind: OptionKind,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "lowercase")]
pub enum OptionKind {
    Check {
        #[serde(default, skip_serializing_if = "Option::is_none")]
        default: Option<bool>,
    },
    Spin {
        #[serde(default, skip_serializing_if = "Option::is_none")]
        default: Option<i64>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        min: Option<i64>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        max: Option<i64>,
    },
    Float {
        #[serde(default, skip_serializing_if = "Option::is_none")]
        default: Option<f64>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        min: Option<f64>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        max: Option<f64>,
    },
    Combo {
        #[serde(default, skip_serializing_if = "Option::is_none")]
        default: Option<String>,
        choices: Vec<String>,
    },
    String {
        #[serde(default, skip_serializing_if = "Option::is_none")]
        default: Option<String>,
    },
    /// A string naming a file; a GUI can offer a file picker.
    File {
        #[serde(default, skip_serializing_if = "Option::is_none")]
        default: Option<String>,
    },
    /// A string naming a directory; a GUI can offer a directory picker.
    Path {
        #[serde(default, skip_serializing_if = "Option::is_none")]
        default: Option<String>,
    },
    /// An action, sent as `setoption name <name>` with no value.
    Button,
}

#[derive(Debug, thiserror::Error, PartialEq, Eq)]
pub enum ManifestError {
    #[error("not a valid engine manifest: {0}")]
    Syntax(String),
    #[error("engine manifest version {0} isn't supported (this reads version {MANIFEST_VERSION})")]
    Version(u32),
    #[error("engine manifest: {0}")]
    Invalid(String),
}

/// This computer's platform key in `downloads`: `<os>-<arch>`, as Rust
/// names them (`linux-x86_64`, `windows-x86_64`, `macos-aarch64`).
pub fn platform() -> String {
    format!("{}-{}", std::env::consts::OS, std::env::consts::ARCH)
}

impl Manifest {
    /// Reads and checks a manifest.
    pub fn parse(text: &str) -> Result<Manifest, ManifestError> {
        // The version first, so a newer format says so rather than failing
        // on whatever changed.
        let value: serde_json::Value =
            serde_json::from_str(text).map_err(|e| ManifestError::Syntax(e.to_string()))?;
        match value.get("manifest_version").and_then(serde_json::Value::as_u64) {
            Some(v) if v == u64::from(MANIFEST_VERSION) => {}
            Some(v) => return Err(ManifestError::Version(u32::try_from(v).unwrap_or(u32::MAX))),
            None => return Err(ManifestError::Invalid("`manifest_version` is missing".into())),
        }
        let manifest: Manifest =
            serde_json::from_value(value).map_err(|e| ManifestError::Syntax(e.to_string()))?;
        manifest.check()?;
        Ok(manifest)
    }

    /// The download for this computer, if the release has one.
    pub fn download(&self) -> Option<&Download> {
        self.downloads.get(&platform())
    }

    /// The arguments that start the engine from `download`.
    pub fn args_for<'a>(&'a self, download: &'a Download) -> &'a [String] {
        download.args.as_deref().unwrap_or(&self.args)
    }

    fn check(&self) -> Result<(), ManifestError> {
        let invalid = |m: String| Err(ManifestError::Invalid(m));
        for (key, value) in [("id", &self.id), ("name", &self.name), ("version", &self.version)] {
            if value.trim().is_empty() {
                return invalid(format!("`{key}` is empty"));
            }
        }
        for (platform, d) in &self.downloads {
            d.check().map_err(|m| ManifestError::Invalid(format!("download for {platform}: {m}")))?;
        }
        if let Some(url) = &self.update_url
            && !is_https(url)
        {
            return invalid(format!("`update_url` isn't https: {url}"));
        }
        let mut names = HashSet::new();
        for o in &self.options {
            o.check().map_err(|m| ManifestError::Invalid(format!("option {:?}: {m}", o.name)))?;
            if !names.insert(o.name.as_str()) {
                return invalid(format!("option {:?} is listed twice", o.name));
            }
        }
        Ok(())
    }
}

impl Download {
    /// The executable's path relative to where the download is unpacked or
    /// saved, as `/`-separated parts.
    pub fn program_path(&self) -> String {
        match &self.program {
            Some(p) => p.clone(),
            None => self.url.rsplit('/').next().unwrap_or_default().to_string(),
        }
    }

    fn check(&self) -> Result<(), String> {
        if !is_https(&self.url) {
            return Err(format!("the url isn't https: {}", self.url));
        }
        if self.sha256.len() != 64 || !self.sha256.bytes().all(|b| b.is_ascii_hexdigit()) {
            return Err("`sha256` isn't 64 hexadecimal digits".into());
        }
        if self.archive.is_some() && self.program.is_none() {
            return Err("an archive needs `program`, the executable's path inside it".into());
        }
        let program = self.program_path();
        if !is_safe_relative_path(&program) {
            return Err(format!("the program path isn't a plain relative path: {program:?}"));
        }
        Ok(())
    }
}

impl OptionSpec {
    fn check(&self) -> Result<(), String> {
        if self.name.is_empty() || self.name.contains(char::is_whitespace) {
            return Err("the name needs to be one word".into());
        }
        match &self.kind {
            OptionKind::Spin { default, min, max } => {
                if let (Some(lo), Some(hi)) = (min, max)
                    && lo > hi
                {
                    return Err(format!("min {lo} is above max {hi}"));
                }
                if let Some(d) = default
                    && (min.is_some_and(|lo| *d < lo) || max.is_some_and(|hi| *d > hi))
                {
                    return Err(format!("the default {d} is out of bounds"));
                }
            }
            OptionKind::Combo { default, choices } => {
                if choices.is_empty() {
                    return Err("a combo needs choices".into());
                }
                if let Some(d) = default
                    && !choices.contains(d)
                {
                    return Err(format!("the default {d:?} isn't one of the choices"));
                }
            }
            OptionKind::Float { default, min, max } => {
                if [default, min, max].into_iter().flatten().any(|x| !x.is_finite()) {
                    return Err("bounds and default need to be finite".into());
                }
                if let (Some(lo), Some(hi)) = (min, max)
                    && lo > hi
                {
                    return Err(format!("min {lo} is above max {hi}"));
                }
                if let Some(d) = default
                    && (min.is_some_and(|lo| *d < lo) || max.is_some_and(|hi| *d > hi))
                {
                    return Err(format!("the default {d} is out of bounds"));
                }
            }
            OptionKind::Check { .. }
            | OptionKind::String { .. }
            | OptionKind::File { .. }
            | OptionKind::Path { .. }
            | OptionKind::Button => {}
        }
        Ok(())
    }
}

fn is_https(url: &str) -> bool {
    url.get(..8).is_some_and(|s| s.eq_ignore_ascii_case("https://")) && url.len() > 8
}

/// A path that stays inside the directory it's relative to: not absolute,
/// no `..`, no empty parts, and no backslashes or drive letters (which
/// Windows would read as separators or roots).
pub fn is_safe_relative_path(path: &str) -> bool {
    !path.is_empty()
        && !path.starts_with('/')
        && !path.contains('\\')
        && !path.contains(':')
        && path.split('/').all(|part| !part.is_empty() && part != "." && part != "..")
}

/// Compares two versions when the format allows it: dotted numbers
/// (`2026.10.1`) by their parts, anything else only as equal or not
/// (`None` when they differ and can't be ordered).
pub fn compare_versions(a: &str, b: &str) -> Option<Ordering> {
    fn parts(v: &str) -> Option<Vec<u64>> {
        v.trim().trim_start_matches(['v', 'V']).split('.').map(|p| p.parse().ok()).collect()
    }
    match (parts(a), parts(b)) {
        (Some(mut x), Some(mut y)) => {
            // 1.2 and 1.2.0 are the same version.
            let len = x.len().max(y.len());
            x.resize(len, 0);
            y.resize(len, 0);
            Some(x.cmp(&y))
        }
        _ => (a == b).then_some(Ordering::Equal),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const DIGEST: &str = "9f86d081884c7d659a2feaa0c55ad015a3bf4f1b2b0b822cd15d6c15b0f00a08";

    fn example() -> String {
        format!(
            r#"{{
              "manifest_version": 1,
              "id": "github.com/Janzert/OpFor",
              "name": "OpFor",
              "version": "2026.10.1",
              "license": "MIT",
              "update_url": "https://github.com/Janzert/OpFor/releases/latest/download/engine.json",
              "future_key": {{ "ignored": true }},
              "downloads": {{
                "linux-x86_64": {{
                  "url": "https://example.com/opfor-linux.tar.gz",
                  "sha256": "{DIGEST}", "archive": "tar.gz", "program": "opfor/bot_opfor"
                }},
                "windows-x86_64": {{
                  "url": "https://example.com/bot_opfor.exe", "sha256": "{DIGEST}", "args": ["aei"]
                }}
              }},
              "options": [
                {{ "name": "hash", "type": "spin", "default": 10, "min": 1, "description": "MB" }},
                {{ "name": "verbose", "type": "check", "default": false }},
                {{ "name": "style", "type": "combo", "choices": ["solid", "wild"], "default": "solid" }},
                {{ "name": "aggression", "type": "float", "default": 0.75, "min": 0, "max": 1.5 }},
                {{ "name": "book", "type": "file", "default": "" }},
                {{ "name": "tablebases", "type": "path" }},
                {{ "name": "clear_hash", "type": "button" }}
              ]
            }}"#
        )
    }

    #[test]
    fn reads_the_example() {
        let m = Manifest::parse(&example()).unwrap();
        assert_eq!((m.name.as_str(), m.version.as_str()), ("OpFor", "2026.10.1"));
        let linux = &m.downloads["linux-x86_64"];
        assert_eq!(linux.archive, Some(Archive::TarGz));
        assert_eq!(linux.program_path(), "opfor/bot_opfor");
        assert!(m.args_for(linux).is_empty());
        let windows = &m.downloads["windows-x86_64"];
        assert_eq!(windows.program_path(), "bot_opfor.exe", "a bare executable keeps its file name");
        assert_eq!(m.args_for(windows), ["aei"]);
        assert_eq!(m.options[0].kind, OptionKind::Spin { default: Some(10), min: Some(1), max: None });
        assert_eq!(
            m.options[2].kind,
            OptionKind::Combo { default: Some("solid".into()), choices: vec!["solid".into(), "wild".into()] }
        );
    }

    #[test]
    fn reads_the_newer_option_types() {
        let m = Manifest::parse(&example()).unwrap();
        let kinds: Vec<&OptionKind> = m.options[3..].iter().map(|o| &o.kind).collect();
        assert_eq!(
            kinds,
            [
                &OptionKind::Float { default: Some(0.75), min: Some(0.0), max: Some(1.5) },
                &OptionKind::File { default: Some(String::new()) },
                &OptionKind::Path { default: None },
                &OptionKind::Button,
            ]
        );
    }

    #[test]
    fn downloads_are_optional() {
        // A developer's own build: options and arguments, nothing to download.
        let m = Manifest::parse(
            r#"{ "manifest_version": 1, "id": "local/mybot", "name": "MyBot", "version": "dev",
                 "args": ["aei"], "options": [{ "name": "hash", "type": "spin" }] }"#,
        )
        .unwrap();
        assert!(m.downloads.is_empty());
        assert_eq!(m.download(), None);
        assert_eq!(m.args, ["aei"]);
    }

    #[test]
    fn round_trips_through_json() {
        let m = Manifest::parse(&example()).unwrap();
        let again = Manifest::parse(&serde_json::to_string(&m).unwrap()).unwrap();
        assert_eq!(again, m);
    }

    #[test]
    fn refuses_other_format_versions() {
        let text = example().replace("\"manifest_version\": 1", "\"manifest_version\": 2");
        assert_eq!(Manifest::parse(&text), Err(ManifestError::Version(2)));
        let text = example().replace("\"manifest_version\": 1,", "");
        assert!(matches!(Manifest::parse(&text), Err(ManifestError::Invalid(_))));
    }

    #[test]
    fn refuses_unsafe_or_broken_downloads() {
        let bad = |from: &str, to: &str| {
            let text = example().replace(from, to);
            assert_ne!(text, example(), "{from} not in the example");
            assert!(matches!(Manifest::parse(&text), Err(ManifestError::Invalid(_))), "{to}");
        };
        bad("https://example.com/opfor-linux", "http://example.com/opfor-linux");
        bad(&format!("\"sha256\": \"{DIGEST}\", \"archive\""), "\"sha256\": \"abc\", \"archive\"");
        bad("\"program\": \"opfor/bot_opfor\"", "\"program\": \"../bot_opfor\"");
        bad("\"program\": \"opfor/bot_opfor\"", "\"program\": \"/usr/bin/bot_opfor\"");
        bad(", \"program\": \"opfor/bot_opfor\"", "");
        bad("\"name\": \"OpFor\"", "\"name\": \" \"");
        bad("\"update_url\": \"https://", "\"update_url\": \"http://");
    }

    #[test]
    fn refuses_inconsistent_options() {
        let bad = |from: &str, to: &str| {
            let text = example().replace(from, to);
            assert_ne!(text, example(), "{from} not in the example");
            assert!(matches!(Manifest::parse(&text), Err(ManifestError::Invalid(_))), "{to}");
        };
        bad("\"default\": 10, \"min\": 1", "\"default\": 0, \"min\": 1");
        bad("\"default\": \"solid\"", "\"default\": \"calm\"");
        bad("\"name\": \"verbose\"", "\"name\": \"hash\"");
        bad("\"name\": \"verbose\"", "\"name\": \"two words\"");
        bad("\"default\": 0.75, \"min\": 0", "\"default\": 2.5, \"min\": 0");
        // An unknown type is a syntax error rather than silently dropped.
        let text = example().replace("\"type\": \"check\"", "\"type\": \"slider\"");
        assert!(matches!(Manifest::parse(&text), Err(ManifestError::Syntax(_))));
    }

    #[test]
    fn safe_paths() {
        for ok in ["bot", "opfor/bot_opfor", "a.b/c-d_e.exe"] {
            assert!(is_safe_relative_path(ok), "{ok}");
        }
        for bad in ["", "/bot", "../bot", "a/../b", "a//b", "./bot", "a\\b", "C:bot", "a/"] {
            assert!(!is_safe_relative_path(bad), "{bad}");
        }
    }

    #[test]
    fn versions() {
        assert_eq!(compare_versions("2026.10.1", "2026.9.30"), Some(Ordering::Greater));
        assert_eq!(compare_versions("v1.2", "1.2.0"), Some(Ordering::Equal));
        assert_eq!(compare_versions("1.10", "1.9"), Some(Ordering::Greater));
        assert_eq!(compare_versions("beta", "beta"), Some(Ordering::Equal));
        assert_eq!(compare_versions("beta", "1.0"), None);
    }

    #[test]
    fn platform_names_follow_rust() {
        let p = platform();
        assert!(p.starts_with(std::env::consts::OS) && p.ends_with(std::env::consts::ARCH), "{p}");
    }
}
