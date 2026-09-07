use std::path::PathBuf;

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("cannot open \"{0}\": {1}")]
    Io(PathBuf, #[source] std::io::Error),

    #[error("invalid EPUB: {0}")]
    InvalidEpub(String),

    #[error("unsupported content: {0}")]
    UnsupportedContent(String),

    #[error("{0}")]
    Message(String),
}

impl Error {
    pub fn io_path<P: Into<PathBuf>>(path: P, err: std::io::Error) -> Self {
        Self::Io(path.into(), err)
    }
}
