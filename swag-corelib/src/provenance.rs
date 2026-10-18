//! Project provenance (`.swag.json` at the project root).
//!
//! Written by [`crate::project::scaffold`] so that `swag update` and
//! `swag doctor` work deterministically from the project directory alone:
//! no registry lookup, no agent, no network needed to know what a project is.
//! Legacy projects (scaffolded before provenance existed) are inferred from
//! `.template_version` / `.template_type` plus the project registry.

use std::fs;
use std::path::Path;

use serde::{Deserialize, Serialize};

use crate::error::{Error, Result};
use crate::template::TemplateType;

/// Provenance file name at the project root.
pub const PROVENANCE_FILE: &str = ".swag.json";

/// What a project was scaffolded from.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct Provenance {
    /// Project (package / directory) name at scaffold time.
    pub name: String,
    /// Template id or custom template name.
    pub template: String,
    /// Builtin base id for custom templates.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub base: Option<String>,
    /// `.template_version` captured at scaffold time.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub version: Option<u64>,
    /// `.template_type` captured at scaffold time.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub template_type: Option<TemplateType>,
    /// GitHub owner used at scaffold time, if any.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub owner: Option<String>,
    /// Exact upstream git revision captured at scaffold time (git templates
    /// only; set by [`crate::project::scaffold`] from `git rev-parse HEAD`).
    /// Makes an otherwise unpinned clone reproducible after the fact.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub commit: Option<String>,
    /// Relative paths of the template-managed files at the last scaffold or
    /// update. `swag update --prune` only deletes files listed here that
    /// disappeared upstream, so user-created files are never pruned.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub template_files: Option<Vec<String>>,
    /// Relative paths that already existed when `swag init` ran and were
    /// deliberately kept instead of replaced by the template (currently
    /// `README.md` and `LICENSE`). `swag update` never overwrites or prunes
    /// these files.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub preserved_files: Option<Vec<String>>,
}

impl Provenance {
    /// Build a provenance record.
    pub fn new(
        name: &str,
        template: &str,
        base: Option<&str>,
        version: Option<u64>,
        template_type: Option<TemplateType>,
        owner: Option<&str>,
    ) -> Self {
        Self {
            name: name.to_string(),
            template: template.to_string(),
            base: base.map(str::to_string),
            version,
            template_type,
            owner: owner.map(str::to_string),
            commit: None,
            template_files: None,
            preserved_files: None,
        }
    }
}

/// Load provenance from `dir/.swag.json`.
pub fn load(dir: &Path) -> Result<Provenance> {
    let path = dir.join(PROVENANCE_FILE);
    let bytes = match fs::read(&path) {
        Ok(bytes) => bytes,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
            return Err(Error::NotAProject(format!(
                "no {} in '{}' (legacy project: re-scaffold or keep a registry entry)",
                PROVENANCE_FILE,
                dir.display()
            )));
        }
        Err(e) => {
            return Err(Error::Io {
                context: format!("could not read '{}'", path.display()),
                source: e,
            });
        }
    };
    serde_json::from_slice(&bytes).map_err(|e| Error::ProvenanceCorrupt {
        path: path.display().to_string(),
        reason: e.to_string(),
    })
}

/// Save provenance to `dir/.swag.json` (pretty JSON, no atomic dance needed:
// scaffold owns the directory at this point).
pub fn save(dir: &Path, provenance: &Provenance) -> Result<()> {
    let rendered = serde_json::to_string_pretty(provenance)
        .map_err(|e| Error::CommandFailed(format!("could not render provenance JSON: {e}")))?;
    fs::write(dir.join(PROVENANCE_FILE), rendered).map_err(|e| Error::Io {
        context: format!("could not write provenance in '{}'", dir.display()),
        source: e,
    })
}

/// Resolve the [`TemplateType`] of a project directory.
///
/// Resolution order:
/// 1. `.swag.json` provenance, with custom templates inheriting their base
///    builtin's type;
/// 2. the provenance's recorded `template_type` (when the template id is no
///    longer resolvable);
/// 3. the `.template_type` file shipped by the template;
/// 4. layout markers, for hand-created or legacy projects.
pub fn template_type_for_dir(dir: &Path) -> Result<TemplateType> {
    if let Ok(provenance) = load(dir) {
        if let Ok(resolved) = crate::custom_templates::resolve_id(&provenance.template) {
            return Ok(match resolved {
                crate::custom_templates::ResolvedTemplate::Builtin(template) => {
                    template.template_type()
                }
                crate::custom_templates::ResolvedTemplate::Custom(custom) => {
                    custom.base.template_type()
                }
            });
        }
        if let Some(kind) = provenance.template_type {
            return Ok(kind);
        }
    }
    if let Some(kind) = crate::template::read_template_type(dir) {
        return Ok(kind);
    }
    if let Some(kind) = infer_template_type(dir) {
        return Ok(kind);
    }
    Err(Error::NotAProject(format!(
        "cannot determine project type for '{}' (no .swag.json or .template_type)",
        dir.display()
    )))
}

/// Best-effort type inference from the directory layout, used only when a
/// project ships neither provenance nor a `.template_type` file.
fn infer_template_type(dir: &Path) -> Option<TemplateType> {
    let package = dir.join("package.json").is_file();
    let tauri = dir.join("src-tauri").join("Cargo.toml").is_file();
    let fullstack = dir.join("frontend").join("package.json").is_file()
        && dir.join("backend").join("Cargo.toml").is_file();
    if tauri && package {
        Some(TemplateType::Desktop)
    } else if fullstack {
        Some(TemplateType::Backend)
    } else if package {
        Some(TemplateType::Frontend)
    } else {
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn round_trip() {
        let dir = std::env::temp_dir().join(format!(
            "swag-prov-test-{}",
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .expect("time")
                .as_nanos()
        ));
        fs::create_dir_all(&dir).unwrap();
        let prov = Provenance::new(
            "demo",
            "svelte-clean-template",
            None,
            Some(21),
            Some(TemplateType::Frontend),
            None,
        );
        save(&dir, &prov).unwrap();
        assert_eq!(load(&dir).unwrap(), prov);
        fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn missing_file_is_not_a_project() {
        let dir = std::env::temp_dir().join(format!(
            "swag-prov-missing-{}",
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .expect("time")
                .as_nanos()
        ));
        fs::create_dir_all(&dir).unwrap();
        assert!(matches!(load(&dir), Err(Error::NotAProject(_))));
        fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn corrupt_file_is_reported_as_corrupt() {
        let dir = std::env::temp_dir().join(format!(
            "swag-prov-corrupt-{}",
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .expect("time")
                .as_nanos()
        ));
        fs::create_dir_all(&dir).unwrap();
        fs::write(dir.join(PROVENANCE_FILE), "not json").unwrap();
        assert!(matches!(load(&dir), Err(Error::ProvenanceCorrupt { .. })));
        fs::remove_dir_all(&dir).unwrap();
    }
}
