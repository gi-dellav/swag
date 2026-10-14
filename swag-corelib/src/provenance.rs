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
        }
    }
}

/// Load provenance from `dir/.swag.json`.
pub fn load(dir: &Path) -> Result<Provenance> {
    let path = dir.join(PROVENANCE_FILE);
    let bytes = fs::read(&path).map_err(|_| {
        Error::NotAProject(format!(
            "no {} in '{}' (legacy project: re-scaffold or keep a registry entry)",
            PROVENANCE_FILE,
            dir.display()
        ))
    })?;
    serde_json::from_slice(&bytes).map_err(|e| {
        Error::NotAProject(format!(
            "{} in '{}' is corrupt: {e}",
            PROVENANCE_FILE,
            dir.display()
        ))
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
}
