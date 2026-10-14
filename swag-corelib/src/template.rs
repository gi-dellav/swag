//! Template catalogue.
//!
//! The three built-in templates live at `github.com/gi-dellav/<repo>` and
//! are fetched with `git clone --depth 1` (see [`crate::project::scaffold`]).

use std::fmt;
use std::path::Path;
use std::str::FromStr;

use serde::{Deserialize, Serialize};

use crate::error::{Error, Result};

/// A scaffoldable project template.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum Template {
    /// Static PWA: Svelte 5 + Vite + TailwindCSS 4, GitHub Pages deploy.
    SvelteClean,
    /// Desktop app: Svelte 5 + Tauri 2, Rust core in `src-tauri/`.
    SvelteTauri,
    /// Fullstack: Rust + Axum backend, TypeScript + Svelte frontend.
    SvelteFullstack,
}

impl Template {
    /// All built-in templates, in display order.
    pub const ALL: [Template; 3] = [
        Template::SvelteClean,
        Template::SvelteTauri,
        Template::SvelteFullstack,
    ];

    /// CLI-facing id, e.g. `svelte-clean-template`.
    #[must_use]
    pub fn id(self) -> &'static str {
        match self {
            Template::SvelteClean => "svelte-clean-template",
            Template::SvelteTauri => "svelte-tauri-template",
            Template::SvelteFullstack => "svelte-fullstack-template",
        }
    }

    /// GitHub repo slug (`gi-dellav/<repo>`).
    #[must_use]
    pub fn repo(self) -> String {
        format!("gi-dellav/{}", self.id())
    }

    /// HTTPS clone URL.
    #[must_use]
    pub fn clone_url(self) -> String {
        format!("https://github.com/{}.git", self.repo())
    }

    /// One-line description shown by `swag list-templates`.
    #[must_use]
    pub fn description(self) -> &'static str {
        match self {
            Template::SvelteClean => {
                "Static PWA (Svelte 5 + Vite + TailwindCSS 4), bun-managed, GitHub Pages deploy"
            }
            Template::SvelteTauri => {
                "Desktop app (Svelte 5 + Tauri 2 + TailwindCSS 4), bun-managed, Rust core in src-tauri/"
            }
            Template::SvelteFullstack => {
                "Fullstack app (Rust + Axum backend, TypeScript + Svelte frontend), bun-managed"
            }
        }
    }

    /// Whether the template ships a Rust workspace that needs `cargo` tooling.
    #[must_use]
    pub fn needs_rust(self) -> bool {
        self.template_type().needs_rust()
    }

    /// Logic family of this built-in (mirrors its `.template_type` file).
    #[must_use]
    pub fn template_type(self) -> TemplateType {
        match self {
            Template::SvelteClean => TemplateType::Frontend,
            Template::SvelteTauri => TemplateType::Desktop,
            Template::SvelteFullstack => TemplateType::Backend,
        }
    }

    /// Parse a user-supplied id, accepting short aliases too
    /// (`clean`, `tauri`, `fullstack`).
    pub fn parse_id(id: &str) -> Result<Template> {
        let normalized = id.trim().to_lowercase();
        Template::ALL
            .iter()
            .find(|t| t.id() == normalized || t.short_alias() == normalized)
            .copied()
            .ok_or_else(|| {
                let known = Template::ALL
                    .iter()
                    .map(|t| format!("{} ({})", t.id(), t.short_alias()))
                    .collect::<Vec<_>>()
                    .join(", ");
                Error::UnknownTemplate(id.to_string(), known)
            })
    }

    pub(crate) fn short_alias(self) -> &'static str {
        match self {
            Template::SvelteClean => "clean",
            Template::SvelteTauri => "tauri",
            Template::SvelteFullstack => "fullstack",
        }
    }
}

/// Name of the file inside templates and scaffolded projects holding the
/// template version (a single number, e.g. `21`).
pub const TEMPLATE_VERSION_FILE: &str = ".template_version";
/// Name of the file holding the template type (`frontend`|`backend`|`desktop`).
pub const TEMPLATE_TYPE_FILE: &str = ".template_type";

/// Logic family of a template, read from `.template_type`.
///
/// New templates ship this file; it drives validation and `doctor` checks
/// deterministically (no agent involved):
/// - `frontend`: static webapp (bun only, dots allowed in names);
/// - `backend`: Rust backend present (Cargo names forbid `.`);
/// - `desktop`: Tauri desktop app (Cargo names forbid `.`, Tauri config expected).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum TemplateType {
    Frontend,
    Backend,
    Desktop,
}

impl TemplateType {
    /// Parse a `.template_type` payload (case-insensitive, trimmed).
    pub fn parse(raw: &str) -> Option<Self> {
        match raw.trim().to_ascii_lowercase().as_str() {
            "frontend" => Some(Self::Frontend),
            "backend" => Some(Self::Backend),
            "desktop" => Some(Self::Desktop),
            _ => None,
        }
    }

    /// CLI-facing string (`frontend`, `backend` or `desktop`).
    #[must_use]
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Frontend => "frontend",
            Self::Backend => "backend",
            Self::Desktop => "desktop",
        }
    }

    /// Whether projects of this type ship Rust code (Cargo naming rules apply).
    #[must_use]
    pub fn needs_rust(self) -> bool {
        !matches!(self, Self::Frontend)
    }
}

impl fmt::Display for TemplateType {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

/// Parse a `.template_version` payload (single number, whitespace tolerated).
#[must_use]
pub fn parse_template_version(raw: &str) -> Option<u64> {
    raw.trim().parse::<u64>().ok()
}

/// Read `.template_version` from `dir`; `None` when missing or unparsable
/// (legacy templates predate the file).
#[must_use]
pub fn read_template_version(dir: &Path) -> Option<u64> {
    let raw = std::fs::read_to_string(dir.join(TEMPLATE_VERSION_FILE)).ok()?;
    parse_template_version(&raw)
}

/// Read `.template_type` from `dir`; `None` when missing or unknown
/// (legacy templates predate the file).
#[must_use]
pub fn read_template_type(dir: &Path) -> Option<TemplateType> {
    let raw = std::fs::read_to_string(dir.join(TEMPLATE_TYPE_FILE)).ok()?;
    TemplateType::parse(&raw)
}

impl fmt::Display for Template {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.id())
    }
}

impl FromStr for Template {
    type Err = Error;

    fn from_str(s: &str) -> Result<Self> {
        Template::parse_id(s)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_full_ids_and_aliases() {
        assert_eq!(
            Template::parse_id("svelte-clean-template").unwrap(),
            Template::SvelteClean
        );
        assert_eq!(Template::parse_id("tauri").unwrap(), Template::SvelteTauri);
        assert_eq!(
            Template::parse_id("FULLSTACK").unwrap(),
            Template::SvelteFullstack
        );
    }

    #[test]
    fn rejects_unknown_template_with_helpful_message() {
        let err = Template::parse_id("svelte-native").unwrap_err();
        let msg = err.to_string();
        assert!(msg.contains("svelte-native"), "{msg}");
        assert!(msg.contains("svelte-clean-template"), "{msg}");
    }

    #[test]
    fn template_types_parse_and_map() {
        assert_eq!(
            TemplateType::parse("frontend"),
            Some(TemplateType::Frontend)
        );
        assert_eq!(
            TemplateType::parse(" Backend "),
            Some(TemplateType::Backend)
        );
        assert_eq!(TemplateType::parse("DESKTOP"), Some(TemplateType::Desktop));
        assert_eq!(TemplateType::parse("mobile"), None);
        assert_eq!(
            Template::SvelteClean.template_type(),
            TemplateType::Frontend
        );
        assert_eq!(Template::SvelteTauri.template_type(), TemplateType::Desktop);
        assert_eq!(
            Template::SvelteFullstack.template_type(),
            TemplateType::Backend
        );
        assert_eq!(parse_template_version("21\n"), Some(21));
        assert_eq!(parse_template_version("bogus"), None);
    }

    #[test]
    fn clone_urls_point_at_github() {
        assert_eq!(
            Template::SvelteClean.clone_url(),
            "https://github.com/gi-dellav/svelte-clean-template.git"
        );
    }
}
