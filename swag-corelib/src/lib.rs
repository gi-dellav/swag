//! swag-corelib: all scaffolding and project-management logic.
//!
//! The CLI crate (`swag-cli`) is a thin clap-derive layer over this library.
//! `bun` is the canonical tool for managing generated projects:
//! install, dev, build, check, test and preview all shell out to `bun`.

pub mod bun;
pub mod error;
pub mod project;
pub mod template;

pub use bun::Bun;
pub use error::{Error, Result};
pub use project::{Project, ScaffoldOptions};
pub use template::Template;
