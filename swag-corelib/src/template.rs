//! Template catalogue.
//!
//! The three built-in templates live at `github.com/gi-dellav/<repo>` and
//! are fetched with `git clone --depth 1` (see [`crate::project::scaffold`]).

use std::fmt;
use std::str::FromStr;

use crate::error::{Error, Result};

/// A scaffoldable project template.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
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
        matches!(self, Template::SvelteTauri | Template::SvelteFullstack)
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

    fn short_alias(self) -> &'static str {
        match self {
            Template::SvelteClean => "clean",
            Template::SvelteTauri => "tauri",
            Template::SvelteFullstack => "fullstack",
        }
    }
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
    fn clone_urls_point_at_github() {
        assert_eq!(
            Template::SvelteClean.clone_url(),
            "https://github.com/gi-dellav/svelte-clean-template.git"
        );
    }
}
