//! The list of configured engines, saved as JSON in the app config dir.

use std::path::{Path, PathBuf};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use howdah_aei::{Engine, EngineConfig};

use crate::dto::{ApiError, EngineIdentity, EngineSpec};

pub struct EngineRegistry {
    path: PathBuf,
    engines: Vec<EngineSpec>,
}

impl EngineRegistry {
    /// Loads the list from `path`. A missing or unreadable file starts with
    /// the defaults (the bundled test engine, when present).
    pub fn load(path: PathBuf) -> EngineRegistry {
        let engines = std::fs::read_to_string(&path)
            .ok()
            .and_then(|text| serde_json::from_str(&text).ok())
            .unwrap_or_else(default_engines);
        EngineRegistry { path, engines }
    }

    pub fn list(&self) -> Vec<EngineSpec> {
        self.engines.clone()
    }

    pub fn get(&self, id: &str) -> Option<EngineSpec> {
        self.engines.iter().find(|e| e.id == id).cloned()
    }

    /// Adds or updates an engine; a new one (empty id) gets a fresh id.
    pub fn save(&mut self, mut spec: EngineSpec) -> Result<EngineSpec, ApiError> {
        for o in &mut spec.options {
            o.name = o.name.trim().to_string();
            o.value = o.value.trim().to_string();
        }
        validate(&spec)?;
        if spec.id.is_empty() {
            spec.id = new_id(&spec.name);
        }
        match self.engines.iter_mut().find(|e| e.id == spec.id) {
            Some(existing) => *existing = spec.clone(),
            None => self.engines.push(spec.clone()),
        }
        self.write()?;
        Ok(spec)
    }

    pub fn delete(&mut self, id: &str) -> Result<(), ApiError> {
        self.engines.retain(|e| e.id != id);
        self.write()
    }

    fn write(&self) -> Result<(), ApiError> {
        let io = |e: std::io::Error| ApiError::state(format!("couldn't save the engine list: {e}"));
        if let Some(dir) = self.path.parent() {
            std::fs::create_dir_all(dir).map_err(io)?;
        }
        let json = serde_json::to_string_pretty(&self.engines).expect("engine specs serialize");
        std::fs::write(&self.path, json).map_err(io)
    }
}

fn validate(spec: &EngineSpec) -> Result<(), ApiError> {
    if spec.name.trim().is_empty() {
        return Err(ApiError::illegal("the engine needs a name"));
    }
    if spec.program.trim().is_empty() {
        return Err(ApiError::illegal("the engine needs a program to run"));
    }
    if let Some(o) = spec.options.iter().find(|o| o.name.is_empty() || o.name.contains(char::is_whitespace)) {
        return Err(ApiError::illegal(format!("option name {:?} needs to be one word", o.name)));
    }
    Ok(())
}

fn new_id(name: &str) -> String {
    let slug: String =
        name.to_lowercase().chars().map(|c| if c.is_ascii_alphanumeric() { c } else { '-' }).collect();
    let stamp = SystemTime::now().duration_since(UNIX_EPOCH).map_or(0, |d| d.as_millis());
    format!("{}-{stamp}", slug.trim_matches('-'))
}

/// The bundled random-move test engine, if it's next to the app binary
/// (it is in development builds after `cargo build -p howdah-aei`).
fn default_engines() -> Vec<EngineSpec> {
    let exe = std::env::current_exe().ok();
    let dir = exe.as_deref().and_then(Path::parent);
    let name = if cfg!(windows) { "aei-test-engine.exe" } else { "aei-test-engine" };
    dir.map(|d| d.join(name))
        .filter(|p| p.exists())
        .map(|p| EngineSpec {
            id: "random-test-engine".into(),
            name: "Random mover (test engine)".into(),
            program: p.display().to_string(),
            args: Vec::new(),
            working_dir: None,
            options: Vec::new(),
        })
        .into_iter()
        .collect()
}

pub fn engine_config(spec: &EngineSpec) -> EngineConfig {
    let mut config = EngineConfig::new(&spec.program).args(spec.args.iter().cloned());
    if let Some(dir) = spec.working_dir.as_deref().filter(|d| !d.trim().is_empty()) {
        config = config.working_dir(dir);
    }
    config
}

/// Starts the engine, reads its identity from the handshake, and quits it.
pub async fn probe(spec: &EngineSpec) -> Result<EngineIdentity, ApiError> {
    validate(spec)?;
    let engine = Engine::start(&engine_config(spec)).await.map_err(ApiError::illegal)?;
    let id = engine.id().clone();
    engine.quit(Duration::from_secs(3)).await.ok();
    Ok(EngineIdentity {
        name: id.name,
        author: id.author,
        version: id.version,
        protocol_version: id.protocol_version,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn spec(name: &str) -> EngineSpec {
        EngineSpec {
            id: String::new(),
            name: name.into(),
            program: "/bin/true".into(),
            args: vec![],
            working_dir: None,
            options: Vec::new(),
        }
    }

    #[test]
    fn save_update_delete_persist() {
        let dir = std::env::temp_dir().join(format!("arimaa-engines-test-{}", std::process::id()));
        let path = dir.join("engines.json");
        let _ = std::fs::remove_file(&path);
        let mut reg = EngineRegistry { path: path.clone(), engines: Vec::new() };
        let a = reg.save(spec("Bot A")).unwrap();
        assert!(a.id.starts_with("bot-a-"));
        let mut a2 = a.clone();
        a2.args = vec!["aei".into()];
        reg.save(a2.clone()).unwrap();
        assert_eq!(reg.list(), vec![a2.clone()]);
        assert!(reg.save(spec(" ")).is_err());

        let reloaded = EngineRegistry::load(path.clone());
        assert_eq!(reloaded.get(&a.id), Some(a2));
        reg.delete(&a.id).unwrap();
        assert!(EngineRegistry::load(path).list().is_empty());
        let _ = std::fs::remove_dir_all(dir);
    }
}
