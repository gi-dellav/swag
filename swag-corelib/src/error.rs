//! Unified error type for swag-corelib.

/// All errors produced by swag-corelib.
#[derive(Debug, thiserror::Error)]
pub enum Error {
    /// A `bun` (or `git`) subprocess failed.
    #[error("command failed: {0}")]
    CommandFailed(String),

    /// The `bun` binary could not be found on PATH.
    #[error("bun not found on PATH (requires bun >= 1.1): {0}")]
    BunNotFound(String),

    /// A template id is unknown (not one of the three built-ins).
    #[error("unknown template '{0}' (expected one of: {1})")]
    UnknownTemplate(String, String),

    /// Template fetch (git clone) failed.
    #[error("failed to fetch template '{template}': {reason}")]
    FetchFailed {
        /// Template that failed to fetch.
        template: String,
        /// Human-readable reason.
        reason: String,
    },

    /// The destination directory already exists and `--force` was not given.
    #[error("destination '{0}' already exists (use --force to overwrite)")]
    DestinationExists(String),

    /// A project name is not usable as a directory / package name.
    #[error("invalid project name '{0}': {1}")]
    InvalidName(String, String),

    /// A GitHub owner is not usable in repo slugs / bundle identifiers.
    #[error("invalid GitHub owner '{0}': {1}")]
    InvalidOwner(String, String),

    /// Any filesystem I/O error, with context about what was attempted.
    #[error("{context}: {source}")]
    Io {
        /// What swag was trying to do.
        context: String,
        /// The underlying I/O error.
        #[source]
        source: std::io::Error,
    },
}

/// Convenience alias used across the crate.
pub type Result<T> = std::result::Result<T, Error>;
