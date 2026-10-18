//! Template-aware project layout.
//!
//! Built-in templates have three shapes:
//! - frontend: one Bun/Vite package at the project root;
//! - backend (fullstack): `frontend/` Bun package + `backend/` Cargo crate;
//! - desktop: root Bun package + `src-tauri/` Cargo crate.
//!
//! Commands use this module to find the components they must drive instead of
//! assuming every project is a single Bun package at the root.

use std::path::{Path, PathBuf};

use crate::error::{Error, Result};
use crate::provenance;
use crate::template::TemplateType;

/// A runnable part of a project.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Component {
    /// Bun/Vite package (`package.json`).
    Web,
    /// Cargo crate (`Cargo.toml`), including Tauri's `src-tauri/`.
    Rust,
}

impl Component {
    /// Human-readable label used in command output.
    #[must_use]
    pub fn as_str(self) -> &'static str {
        match self {
            Component::Web => "web",
            Component::Rust => "rust",
        }
    }
}

/// Resolved component roots for a project directory.
#[derive(Debug, Clone)]
pub struct ProjectLayout {
    /// Logic family from provenance / `.template_type` / layout markers.
    pub kind: TemplateType,
    /// Project root.
    pub root: PathBuf,
    /// Bun/Vite package root, when the project has one.
    pub web_root: Option<PathBuf>,
    /// Cargo crate root (`backend/` or `src-tauri/`), when present.
    pub rust_root: Option<PathBuf>,
}

impl ProjectLayout {
    /// Detect the layout of `root`.
    pub fn detect(root: &Path) -> Result<Self> {
        if !root.is_dir() {
            return Err(Error::NotAProject(format!(
                "'{}' is not a project directory",
                root.display()
            )));
        }
        let kind = provenance::template_type_for_dir(root)?;
        let web_root = find_web_root(root, kind);
        let rust_root = find_rust_root(root, kind);
        Ok(Self {
            kind,
            root: root.to_path_buf(),
            web_root,
            rust_root,
        })
    }

    /// Whether this project has a Bun/Vite component.
    #[must_use]
    pub fn has_web(&self) -> bool {
        self.web_root.is_some()
    }

    /// Whether this project has a Cargo component.
    #[must_use]
    pub fn has_rust(&self) -> bool {
        self.rust_root.is_some()
    }

    /// The web root, or an actionable error when the component is missing.
    pub fn require_web(&self) -> Result<&Path> {
        self.web_root.as_deref().ok_or_else(|| {
            Error::CommandFailed(format!(
                "no web component in '{}' (expected package.json; run `swag doctor`)",
                self.root.display()
            ))
        })
    }

    /// The Rust root, or an actionable error when the component is missing.
    pub fn require_rust(&self) -> Result<&Path> {
        self.rust_root.as_deref().ok_or_else(|| {
            Error::CommandFailed(format!(
                "no Rust component in '{}' (expected backend/ or src-tauri/; run `swag doctor`)",
                self.root.display()
            ))
        })
    }

    /// Whether this is a Tauri desktop project.
    #[must_use]
    pub fn is_desktop(&self) -> bool {
        self.kind == TemplateType::Desktop
    }
}

fn find_web_root(root: &Path, kind: TemplateType) -> Option<PathBuf> {
    let root_package = root.join("package.json").is_file();
    let frontend_package = root.join("frontend").join("package.json").is_file();
    match kind {
        TemplateType::Backend => {
            if frontend_package {
                Some(root.join("frontend"))
            } else if root_package {
                Some(root.to_path_buf())
            } else {
                None
            }
        }
        TemplateType::Desktop | TemplateType::Frontend => {
            if root_package {
                Some(root.to_path_buf())
            } else if frontend_package {
                Some(root.join("frontend"))
            } else {
                None
            }
        }
    }
}

fn find_rust_root(root: &Path, kind: TemplateType) -> Option<PathBuf> {
    let root_manifest = root.join("Cargo.toml").is_file();
    match kind {
        TemplateType::Backend => {
            if root.join("backend").join("Cargo.toml").is_file() {
                Some(root.join("backend"))
            } else if root_manifest {
                Some(root.to_path_buf())
            } else {
                None
            }
        }
        TemplateType::Desktop => {
            if root.join("src-tauri").join("Cargo.toml").is_file() {
                Some(root.join("src-tauri"))
            } else if root_manifest {
                Some(root.to_path_buf())
            } else {
                None
            }
        }
        TemplateType::Frontend => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn stamp(tag: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!(
            "swag-layout-test-{tag}-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .expect("time")
                .as_nanos()
        ));
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    fn write(path: &Path, content: &str) {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent).unwrap();
        }
        std::fs::write(path, content).unwrap();
    }

    #[test]
    fn detects_frontend_layout() {
        let dir = stamp("frontend");
        write(&dir.join(".template_type"), "frontend\n");
        write(&dir.join("package.json"), "{}");
        let layout = ProjectLayout::detect(&dir).unwrap();
        assert_eq!(layout.kind, TemplateType::Frontend);
        assert_eq!(layout.web_root.as_deref(), Some(dir.as_path()));
        assert_eq!(layout.rust_root, None);
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn detects_fullstack_layout() {
        let dir = stamp("fullstack");
        write(&dir.join(".template_type"), "backend\n");
        write(&dir.join("frontend/package.json"), "{}");
        write(
            &dir.join("backend/Cargo.toml"),
            "[package]\nname = \"backend\"\n",
        );
        let layout = ProjectLayout::detect(&dir).unwrap();
        assert_eq!(layout.kind, TemplateType::Backend);
        assert_eq!(
            layout.web_root.as_deref(),
            Some(dir.join("frontend").as_path())
        );
        assert_eq!(
            layout.rust_root.as_deref(),
            Some(dir.join("backend").as_path())
        );
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn detects_desktop_layout() {
        let dir = stamp("desktop");
        write(&dir.join(".template_type"), "desktop\n");
        write(&dir.join("package.json"), "{}");
        write(
            &dir.join("src-tauri/Cargo.toml"),
            "[package]\nname = \"desktop\"\n",
        );
        let layout = ProjectLayout::detect(&dir).unwrap();
        assert_eq!(layout.kind, TemplateType::Desktop);
        assert_eq!(layout.web_root.as_deref(), Some(dir.as_path()));
        assert_eq!(
            layout.rust_root.as_deref(),
            Some(dir.join("src-tauri").as_path())
        );
        assert!(layout.is_desktop());
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn infers_layout_without_metadata() {
        let dir = stamp("infer");
        write(&dir.join("frontend/package.json"), "{}");
        write(
            &dir.join("backend/Cargo.toml"),
            "[package]\nname = \"backend\"\n",
        );
        let layout = ProjectLayout::detect(&dir).unwrap();
        assert_eq!(layout.kind, TemplateType::Backend);
        assert!(layout.has_web());
        assert!(layout.has_rust());
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn missing_components_error_with_hints() {
        let dir = stamp("missing");
        write(&dir.join(".template_type"), "frontend\n");
        let layout = ProjectLayout::detect(&dir).unwrap();
        assert!(
            layout
                .require_web()
                .unwrap_err()
                .to_string()
                .contains("no web")
        );
        assert!(
            layout
                .require_rust()
                .unwrap_err()
                .to_string()
                .contains("no Rust")
        );
        std::fs::remove_dir_all(&dir).unwrap();
    }
}
