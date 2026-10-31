use std::path::PathBuf;

/// Errors that callers can turn into actionable UI copy.
#[derive(Debug, thiserror::Error)]
pub enum EntwurfError {
    #[cfg(feature = "native")]
    #[error("database error: {0}")]
    Database(#[from] rusqlite::Error),

    #[error("unsupported source type: {0}")]
    UnsupportedSource(String),

    #[error("could not read {path}: {source}")]
    Read {
        path: PathBuf,
        #[source]
        source: std::io::Error,
    },

    #[error("could not write {path}: {source}")]
    Write {
        path: PathBuf,
        #[source]
        source: std::io::Error,
    },

    #[error("invalid contract: {0}")]
    Contract(String),

    #[error("knowledge IR is not compilable: {0}")]
    InvalidIr(String),

    #[error("project not found: {0}")]
    ProjectNotFound(String),

    #[error("task not found: {0}")]
    TaskNotFound(String),

    #[error("layout failed: {0}")]
    Layout(String),

    #[error("render failed: {0}")]
    Render(String),

    #[error("export failed: {0}")]
    Export(String),

    #[error("validation has {blocking} blocking finding(s)")]
    ValidationBlocked { blocking: usize },

    #[error("OCR is unavailable; install Tesseract (chi_sim+eng) or import text")]
    OcrUnavailable,

    #[error("state transition {from} -> {to} is not allowed")]
    InvalidTransition { from: String, to: String },

    #[error("invalid input: {0}")]
    InvalidInput(String),

    #[error("unsupported operation: {0}")]
    Unsupported(String),

    #[error("stored data is corrupt at {path}: {reason}")]
    CorruptState { path: PathBuf, reason: String },
}

pub type Result<T> = std::result::Result<T, EntwurfError>;

impl EntwurfError {
    pub fn read(path: impl Into<PathBuf>, source: std::io::Error) -> Self {
        Self::Read {
            path: path.into(),
            source,
        }
    }

    pub fn write(path: impl Into<PathBuf>, source: std::io::Error) -> Self {
        Self::Write {
            path: path.into(),
            source,
        }
    }
}
