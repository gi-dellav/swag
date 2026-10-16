//! Unified error type for swag-corelib.

/// All errors produced by swag-corelib.
#[derive(Debug, thiserror::Error)]
pub enum Error {
    /// A `bun` (or `git`) subprocess failed.
    #[error("command failed: {0}")]
    CommandFailed(String),

    /// An internal invariant failed (JSON rendering, tree walking, …):
    /// never caused by user input, so it signals a bug rather than a fix.
    #[error("internal error: {0}")]
    Internal(String),

    /// The `bun` binary could not be found on PATH.
    #[error("bun not found on PATH (requires bun >= 1.1): {0}")]
    BunNotFound(String),

    /// The `vercel` binary could not be found on PATH.
    #[error(
        "vercel not found on PATH (install with `npm i -g vercel` or `bun add -g vercel`, see https://vercel.com/docs/cli): {0}"
    )]
    VercelNotFound(String),

    /// No home directory could be determined (`$SWAG_HOME` unset and no home found).
    #[error("could not determine swag home directory: {0}")]
    HomeNotFound(String),

    /// A registry file exists but is not valid JSON / has an unexpected shape.
    #[error("registry '{path}' is corrupt: {reason} (delete it or fix the JSON)")]
    RegistryCorrupt {
        /// Path of the corrupt registry file.
        path: String,
        /// Human-readable reason.
        reason: String,
    },

    /// A project name is not in the registry.
    #[error("unknown project '{0}' (see `swag projects list`)")]
    ProjectNotFound(String),

    /// A custom template name is already registered.
    #[error("template '{0}' already exists (use --force to overwrite)")]
    TemplateExists(String),

    /// A custom template name is not usable.
    #[error("invalid template name '{0}': {1}")]
    InvalidTemplateName(String, String),

    /// A custom template source (git URL / path) is not usable.
    #[error("invalid template source '{0}': {1}")]
    InvalidSource(String, String),

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

    /// The destination directory does not exist and `swag init` needs one.
    #[error(
        "destination '{0}' is not a pre-existing directory (swag init scaffolds into an existing directory; use `swag new` to create one)"
    )]
    DestinationMissing(String),

    /// A directory is not a swag project (missing provenance or unknown template).
    #[error("not a swag project: {0}")]
    NotAProject(String),

    /// The project has uncommitted changes; re-run with `--force` to overwrite.
    #[error("project has uncommitted changes: {0}")]
    DirtyTree(String),

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
