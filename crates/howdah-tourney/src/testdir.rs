//! A test's directory:
//!
//! - `test.toml`: the spec, with each engine's program pointing at its
//!   snapshot.
//! - `engines/<name>/`: the snapshots.
//! - `results.jsonl`: the results log.
//! - `games/`: a record per game.
//! - `logs/`: engine transcripts per game, if the spec asks for them.
//! - `spsa-points.jsonl` and `spsa-history.jsonl`: an SPSA run's state
//!   (see `spsa`).
//!
//! A pool's `test.toml` is written again as engines are added or retired.

use std::path::{Path, PathBuf};

use howdah_aei::EngineConfig;
use sha2::{Digest, Sha256};

use crate::error::TourneyError;
use crate::results::{GameEntry, read_results};
use crate::spec::{EngineSpec, TestSpec};

pub const SPEC_FILE: &str = "test.toml";
pub const RESULTS_FILE: &str = "results.jsonl";

#[derive(Clone, Debug)]
pub struct TestDir {
    pub path: PathBuf,
    pub spec: TestSpec,
}

impl TestDir {
    /// Makes a test's directory from a spec, snapshotting its engines.
    /// Relative paths in the spec are taken from `base` (the spec file's
    /// directory). `dir` must not exist yet, or be empty.
    pub fn create(spec: TestSpec, base: &Path, dir: &Path) -> Result<TestDir, TourneyError> {
        spec.validate()?;
        if std::fs::read_dir(dir).is_ok_and(|mut d| d.next().is_some()) {
            return Err(TourneyError::Spec(format!("{} already exists and isn't empty", dir.display())));
        }
        let result = TestDir::create_in(spec, base, dir);
        if result.is_err() {
            // Leave nothing half made; the directory was empty or missing.
            let _ = std::fs::remove_dir_all(dir);
        }
        result
    }

    fn create_in(mut spec: TestSpec, base: &Path, dir: &Path) -> Result<TestDir, TourneyError> {
        std::fs::create_dir_all(dir).map_err(TourneyError::io(dir))?;
        let path = absolute(dir)?;
        for engine in &mut spec.engines {
            snapshot(&path, engine, base)?;
        }
        if let Some(spsa) = &mut spec.spsa {
            spsa.seed.get_or_insert_with(new_seed);
        }
        if let Some(pool) = &mut spec.pool {
            pool.seed.get_or_insert_with(new_seed);
        }
        TestDir::write_spec(&path, &spec)?;
        Ok(TestDir { path, spec })
    }

    /// Snapshots an engine to add to the test (a pool), as making the
    /// test does: relative paths are taken from `base`. Refuses one whose
    /// snapshot directory is taken.
    pub(crate) fn snapshot_engine(
        &self,
        mut engine: EngineSpec,
        base: &Path,
    ) -> Result<EngineSpec, TourneyError> {
        let snap = self.path.join("engines").join(slug(&engine.name));
        if snap.exists() {
            return Err(TourneyError::Refused(format!(
                "{} has an engine named {:?} already",
                self.spec.name, engine.name
            )));
        }
        let made = snapshot(&self.path, &mut engine, base);
        if made.is_err() {
            let _ = std::fs::remove_dir_all(&snap);
        }
        made.map(|()| engine)
    }

    /// Writes a test's spec, replacing the one there as a whole.
    pub(crate) fn write_spec(path: &Path, spec: &TestSpec) -> Result<(), TourneyError> {
        let spec_path = path.join(SPEC_FILE);
        let tmp = spec_path.with_extension("toml.tmp");
        std::fs::write(&tmp, spec.to_toml()).map_err(TourneyError::io(&tmp))?;
        std::fs::rename(&tmp, &spec_path).map_err(TourneyError::io(&spec_path))
    }

    /// Opens an existing test's directory.
    pub fn open(dir: &Path) -> Result<TestDir, TourneyError> {
        let path = absolute(dir)?;
        let spec_path = path.join(SPEC_FILE);
        let text = std::fs::read_to_string(&spec_path).map_err(TourneyError::io(&spec_path))?;
        let spec =
            TestSpec::parse(&text).map_err(|message| TourneyError::Parse { path: spec_path, message })?;
        Ok(TestDir { path, spec })
    }

    pub fn results_path(&self) -> PathBuf {
        self.path.join(RESULTS_FILE)
    }

    pub fn results(&self) -> Result<Vec<GameEntry>, TourneyError> {
        read_results(&self.results_path())
    }

    /// How to start engine `index`: its snapshot, run in its snapshot's
    /// directory unless the spec names another.
    pub fn engine_config(&self, index: usize) -> EngineConfig {
        self.config_of(self.spec.engine(index))
    }

    /// How to start an engine of the test's (a pool's, which may have been
    /// added since the spec was read).
    pub fn config_of(&self, e: &EngineSpec) -> EngineConfig {
        let program = self.path.join(&e.program);
        let dir = match &e.working_dir {
            Some(wd) => self.path.join(wd),
            None => program.parent().expect("a snapshot is in a directory").to_path_buf(),
        };
        // Its own process group, so a Ctrl-C meant for the runner (which
        // finishes the running games) doesn't kill the engines playing them.
        EngineConfig::new(program).args(e.args.iter().cloned()).working_dir(dir).own_process_group()
    }

    /// Where game `game` of pair `pair` is recorded, relative to the
    /// directory.
    pub fn record_name(pair: u32, game: u8) -> String {
        format!("games/{pair:04}-{game}.txt")
    }

    /// Where the transcript of engine (or side) `name` of a game goes.
    pub fn log_path(&self, pair: u32, game: u8, name: &str) -> PathBuf {
        self.path.join(format!("logs/{pair:04}-{game}-{}.log", slug(name)))
    }
}

/// Copies an engine's program (or its directory) into `dir`'s
/// `engines/<name>/`, and points the spec at the copy.
fn snapshot(dir: &Path, engine: &mut EngineSpec, base: &Path) -> Result<(), TourneyError> {
    let source = base.join(&engine.program);
    let source = std::path::absolute(&source).map_err(TourneyError::io(&source))?;
    if !source.is_file() {
        return Err(TourneyError::Spec(format!("{} isn't a file", source.display())));
    }
    let rel = Path::new("engines").join(slug(&engine.name));
    let snap = dir.join(&rel);
    std::fs::create_dir_all(&snap).map_err(TourneyError::io(&snap))?;
    let file_name = source.file_name().expect("a file has a name");
    if engine.copy_dir {
        copy_dir(source.parent().expect("a file has a directory"), &snap)?;
    } else {
        let to = snap.join(file_name);
        std::fs::copy(&source, &to).map_err(TourneyError::io(&to))?;
    }
    engine.sha256 = Some(sha256_file(&source)?);
    engine.program = rel.join(file_name);
    engine.source = Some(source);
    if let Some(wd) = &engine.working_dir {
        let wd = base.join(wd);
        let wd = std::path::absolute(&wd).map_err(TourneyError::io(&wd))?;
        if !wd.is_dir() {
            return Err(TourneyError::Spec(format!("{} isn't a directory", wd.display())));
        }
        engine.working_dir = Some(wd);
    }
    Ok(())
}

/// An absolute path, without `canonicalize`'s `\\?\` prefix on Windows
/// (programs and working directories are handed to engines).
fn absolute(path: &Path) -> Result<PathBuf, TourneyError> {
    std::path::absolute(path).map_err(TourneyError::io(path))
}

/// A seed for an SPSA run that didn't give one.
fn new_seed() -> u32 {
    let t = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap_or_default();
    (t.as_nanos() as u32) ^ std::process::id().rotate_left(16)
}

/// A name made safe for a file name.
pub(crate) fn slug(name: &str) -> String {
    let s: String =
        name.chars().map(|c| if c.is_ascii_alphanumeric() || "-_.".contains(c) { c } else { '_' }).collect();
    if s.trim_matches('.').is_empty() { "engine".into() } else { s }
}

fn sha256_file(path: &Path) -> Result<String, TourneyError> {
    let bytes = std::fs::read(path).map_err(TourneyError::io(path))?;
    Ok(Sha256::digest(&bytes).iter().map(|b| format!("{b:02x}")).collect())
}

fn copy_dir(from: &Path, to: &Path) -> Result<(), TourneyError> {
    for item in std::fs::read_dir(from).map_err(TourneyError::io(from))? {
        let item = item.map_err(TourneyError::io(from))?;
        let src = item.path();
        let dest = to.join(item.file_name());
        if src.is_dir() {
            std::fs::create_dir_all(&dest).map_err(TourneyError::io(&dest))?;
            copy_dir(&src, &dest)?;
        } else {
            std::fs::copy(&src, &dest).map_err(TourneyError::io(&dest))?;
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn spec(dev: &str, base: &str) -> TestSpec {
        TestSpec::parse(&format!(
            "name = \"t\"\nmax_pairs = 4\n\
             [[engines]]\nname = \"dev build\"\nprogram = \"{dev}\"\ncopy_dir = true\n\
             [[engines]]\nname = \"base\"\nprogram = \"{base}\"\nargs = [\"aei\"]\n"
        ))
        .unwrap()
    }

    #[test]
    fn snapshots_engines_and_reopens() {
        let src = tempfile::tempdir().unwrap();
        std::fs::create_dir(src.path().join("dev")).unwrap();
        std::fs::write(src.path().join("dev/bot"), b"dev program").unwrap();
        std::fs::write(src.path().join("dev/weights.bin"), b"weights").unwrap();
        std::fs::write(src.path().join("base-bot"), b"abc").unwrap();
        let out = tempfile::tempdir().unwrap();
        let dir = out.path().join("test");

        let t = TestDir::create(spec("dev/bot", "base-bot"), src.path(), &dir).unwrap();
        let dev = &t.spec.engines[0];
        assert_eq!(dev.program, Path::new("engines/dev_build/bot"));
        assert!(t.path.join("engines/dev_build/weights.bin").exists());
        let base = &t.spec.engines[1];
        assert_eq!(base.program, Path::new("engines/base/base-bot"));
        assert!(!t.path.join("engines/base/dev").exists());
        assert_eq!(
            base.sha256.as_deref(),
            Some("ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad")
        );
        assert_eq!(
            base.source.as_deref(),
            Some(std::path::absolute(src.path().join("base-bot")).unwrap()).as_deref()
        );

        // Changing the original doesn't change the test.
        std::fs::write(src.path().join("base-bot"), b"rebuilt").unwrap();
        let again = TestDir::open(&dir).unwrap();
        assert_eq!(again.spec, t.spec);
        let config = again.engine_config(1);
        assert_eq!(std::fs::read(&config.program).unwrap(), b"abc");
        assert_eq!(config.args, ["aei"]);
        assert_eq!(config.working_dir.as_deref(), Some(again.path.join("engines/base").as_path()));

        let err = TestDir::create(spec("dev/bot", "base-bot"), src.path(), &dir).unwrap_err();
        assert!(err.to_string().contains("isn't empty"));
    }
}
