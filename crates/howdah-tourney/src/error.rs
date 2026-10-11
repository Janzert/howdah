use std::path::PathBuf;

#[derive(Debug, thiserror::Error)]
pub enum TourneyError {
    #[error("{path}: {source}")]
    Io {
        path: PathBuf,
        #[source]
        source: std::io::Error,
    },
    #[error("invalid test spec: {0}")]
    Spec(String),
    #[error("{path}: {message}")]
    Parse { path: PathBuf, message: String },
    #[error("no test {0:?}")]
    NoTest(String),
    /// A request the daemon won't carry out, and why.
    #[error("{0}")]
    Refused(String),
}

impl TourneyError {
    pub(crate) fn io(path: impl Into<PathBuf>) -> impl FnOnce(std::io::Error) -> TourneyError {
        let path = path.into();
        move |source| TourneyError::Io { path, source }
    }
}
