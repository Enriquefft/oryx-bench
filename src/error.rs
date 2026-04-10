//! Error types for oryx-bench.
//!
//! Application code uses [`anyhow::Result`] for propagation; this module
//! provides structured error types for the pieces that benefit from them
//! (GraphQL pull, project discovery, config load).

use std::path::PathBuf;

use thiserror::Error;

/// A single issue detected by `oryx-bench lint`.
pub use crate::lint::Issue;

/// Standard non-zero exit codes, mirroring `command-reference.md`.
#[derive(Debug, Clone, Copy)]
pub enum ExitKind {
    /// Success.
    Ok = 0,
    /// Lint errors / build failure / explicit failure.
    Failure = 1,
    /// Lint warnings (only with `--strict`).
    WarningStrict = 2,
    /// Usage error (bad CLI arguments).
    Usage = 64,
    /// Data error (corrupt `revision.json` or `layout.toml`, unknown keycode).
    Data = 65,
    /// Service unavailable (Oryx GraphQL down).
    ServiceUnavailable = 69,
    /// IO error (couldn't write file, network failure).
    Io = 74,
}

#[derive(Debug, Error)]
pub enum ProjectError {
    #[error("no oryx-bench project found: searched for kb.toml upward from {0}")]
    NotFound(PathBuf),
    // The source is interpolated manually here (instead of via
    // `{source}` + `#[source]`) because anyhow's chain-aware
    // `:#` formatter would otherwise print the underlying toml
    // error twice — once from the outer format string, once
    // from walking the #[source] chain. Attaching `#[source]`
    // alone is the minimal-information form; we want the full
    // toml error inline so the user sees the line+column
    // pointer without digging.
    #[error("kb.toml at {path} is invalid: {source}")]
    InvalidConfig {
        path: PathBuf,
        source: toml::de::Error,
    },
    #[error("project at {path} is in Oryx mode but pulled/revision.json is missing — run `oryx-bench pull`")]
    MissingPulled { path: PathBuf },
    #[error("project at {path} is in local mode but {file} is missing")]
    MissingLocalLayout { path: PathBuf, file: String },
    #[error("{0}")]
    Other(String),
}

#[derive(Debug, Error)]
pub enum PullError {
    #[error("Oryx GraphQL returned HTTP {status}: {body}")]
    HttpStatus { status: u16, body: String },
    #[error("Oryx GraphQL returned errors: {0}")]
    GraphQl(String),
    #[error("network error talking to Oryx: {0}")]
    Network(#[from] reqwest::Error),
    #[error("layout '{hash_id}' not found on Oryx")]
    LayoutNotFound { hash_id: String },
    #[error("could not parse Oryx response: {0}")]
    Parse(#[from] serde_json::Error),
    #[error(
        "Oryx response exceeds the {limit}-byte cap — refusing to buffer; this is almost certainly a server bug"
    )]
    ResponseTooLarge { limit: usize },
}
