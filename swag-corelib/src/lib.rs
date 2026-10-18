//! swag-corelib: all scaffolding and project-management logic.
//!
//! The CLI crate (`swag-cli`) is a thin clap-derive layer over this library.
//! Commands are template-aware: frontend projects shell out to `bun`, while
//! fullstack and desktop projects drive both `bun` and `cargo` through the
//! task planner in [`tasks`].

pub mod bun;
pub mod cargo;
pub mod custom_templates;
pub mod doctor;
pub mod error;
pub mod home;
pub mod layout;
pub mod project;
pub mod projects;
pub mod provenance;
pub mod tasks;
pub mod template;
#[cfg(test)]
pub(crate) mod test_env_lock;
pub mod update;
pub mod vercel;

pub use bun::Bun;
pub use cargo::Cargo;
pub use custom_templates::{CustomTemplate, ResolvedTemplate, TemplateSource};
pub use error::{Error, Result};
pub use layout::{Component, ProjectLayout};
pub use project::{Project, ScaffoldOptions};
pub use projects::ProjectEntry;
pub use tasks::{Action, Mode, Step, TaskPlan};
pub use template::{Template, TemplateType};
pub use vercel::Vercel;
