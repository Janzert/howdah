use thiserror::Error;

#[derive(Debug, Error)]
pub enum AeiError {
    #[error("couldn't start engine {program}: {source}")]
    Spawn { program: String, source: std::io::Error },
    #[error("working directory {0} doesn't exist")]
    WorkingDir(String),
    #[error("engine I/O error: {0}")]
    Io(std::io::Error),
    #[error("engine exited")]
    Exited,
    #[error("timed out waiting for {0}")]
    Timeout(&'static str),
    #[error("expected {expected}, got {got}")]
    Unexpected { expected: &'static str, got: String },
}
