//! swag-corelib: all scaffolding and project-management logic.
//!
//! The CLI crate (`swag-cli`) is a thin clap-derive layer over this library.
//! `bun` is the canonical tool for managing generated projects:
//! install, dev, build, check, test and preview all shell out to `bun`.

pub mod bun;
pub mod custom_templates;
pub mod error;
pub mod home;
pub mod project;
pub mod projects;
pub mod template;
#[cfg(test)]
pub(crate) mod test_env_lock;

pub use bun::Bun;
pub use custom_templates::{CustomTemplate, ResolvedTemplate, TemplateSource};
pub use error::{Error, Result};
pub use project::{Project, ScaffoldOptions};
pub use projects::ProjectEntry;
pub use template::Template;
