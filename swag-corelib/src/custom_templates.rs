//! Custom templates (`templates.json` in the swag home).
//!
//! Custom templates *derive* from one of the three hardcoded builtins:
//! `base` picks the personalization rules and Rust validation, while
//! `source` points at the actual content (a git repo or a local directory).

use std::collections::BTreeMap;
use std::fs;
use std::path::PathBuf;

use serde::{Deserialize, Serialize};

use crate::error::{Error, Result};
use crate::home;
use crate::template::Template;

/// Where a custom template's content comes from.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(tag = "type", rename_all = "lowercase")]
pub enum TemplateSource {
    /// Clone `url` (optionally `ref`, a branch or tag) with `git clone --depth 1`.
    Git {
        /// HTTPS or SSH clone URL.
        url: String,
        /// Branch or tag; defaults to the remote default branch.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        #[serde(rename = "ref")]
        git_ref: Option<String>,
    },
    /// Copy a local directory at scaffold time.
    Path {
        /// Local directory holding the template content.
        path: PathBuf,
    },
}

impl TemplateSource {
    /// Human-readable id used in listings (URL or path).
    #[must_use]
    pub fn describe(&self) -> String {
        match self {
            TemplateSource::Git { url, git_ref } => match git_ref {
                Some(git_ref) => format!("{url}@{git_ref}"),
                None => url.clone(),
            },
            TemplateSource::Path { path } => path.display().to_string(),
        }
    }
}

/// A user-registered template deriving from a builtin.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct CustomTemplate {
    /// Registry key (must not collide with builtin ids/aliases).
    pub name: String,
    /// Builtin this template derives from (personalization + Rust rules).
    pub base: Template,
    /// One-line description shown by listings.
    #[serde(default)]
    pub description: String,
    /// Where the content is fetched from.
    pub source: TemplateSource,
    /// RFC 3339 creation timestamp.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub created_at: Option<String>,
}

impl CustomTemplate {
    /// Build a template with validated name/source; stamps `created_at`.
    pub fn new(
        name: &str,
        base: Template,
        description: &str,
        source: TemplateSource,
    ) -> Result<Self> {
        validate_name(name)?;
        validate_source(&source)?;
        Ok(Self {
            name: name.to_string(),
            base,
            description: description.to_string(),
            source,
            created_at: Some(crate::projects::now_utc()),
        })
    }
}

/// The on-disk registry: name → template (missing file = empty).
#[derive(Debug, Default, Clone, Serialize, Deserialize)]
pub struct Registry(pub BTreeMap<String, CustomTemplate>);

/// Load the registry; missing file → empty.
pub fn load() -> Result<Registry> {
    let path = home::templates_path()?;
    if !path.exists() {
        return Ok(Registry::default());
    }
    let bytes = fs::read(&path).map_err(|e| Error::Io {
        context: format!("could not read '{}'", path.display()),
        source: e,
    })?;
    serde_json::from_slice(&bytes).map_err(|e| Error::RegistryCorrupt {
        path: path.display().to_string(),
        reason: e.to_string(),
    })
}

fn save(registry: &Registry) -> Result<()> {
    home::write_json_atomic(&home::templates_path()?, registry)
}

/// Ensure `name` is usable and does not shadow a builtin id or alias.
pub fn validate_name(name: &str) -> Result<()> {
    if name.is_empty() {
        return Err(Error::InvalidTemplateName(
            name.to_string(),
            "name must not be empty".to_string(),
        ));
    }
    if name.len() > 64 {
        return Err(Error::InvalidTemplateName(
            name.to_string(),
            "name must be at most 64 characters".to_string(),
        ));
    }
    if !name
        .chars()
        .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-' || c == '_')
    {
        return Err(Error::InvalidTemplateName(
            name.to_string(),
            "name may only contain lowercase letters, digits, '-' and '_'".to_string(),
        ));
    }
    let first = name.chars().next().expect("non-empty checked above");
    if !first.is_ascii_lowercase() && !first.is_ascii_digit() {
        return Err(Error::InvalidTemplateName(
            name.to_string(),
            "name must start with a letter or digit".to_string(),
        ));
    }
    if Template::parse_id(name).is_ok() {
        return Err(Error::InvalidTemplateName(
            name.to_string(),
            "name collides with a built-in template id or alias".to_string(),
        ));
    }
    Ok(())
}

/// Ensure a git URL looks plausible (HTTPS or SSH).
fn validate_git_url(url: &str) -> Result<()> {
    if url.starts_with("https://") && url.len() > "https://".len() {
        return Ok(());
    }
    if url.starts_with("git@") && url.contains(':') {
        return Ok(());
    }
    Err(Error::InvalidSource(
        url.to_string(),
        "git URL must start with https:// or look like git@host:owner/repo.git".to_string(),
    ))
}

/// Validate a source; local paths must exist and be directories.
fn validate_source(source: &TemplateSource) -> Result<()> {
    match source {
        TemplateSource::Git { url, .. } => validate_git_url(url),
        TemplateSource::Path { path } => {
            if !path.exists() {
                return Err(Error::InvalidSource(
                    path.display().to_string(),
                    "path does not exist".to_string(),
                ));
            }
            if !path.is_dir() {
                return Err(Error::InvalidSource(
                    path.display().to_string(),
                    "path is not a directory".to_string(),
                ));
            }
            Ok(())
        }
    }
}

/// Register a template; with `force` an existing entry is overwritten.
pub fn add(template: CustomTemplate, force: bool) -> Result<()> {
    validate_name(&template.name)?;
    validate_source(&template.source)?;
    let mut registry = load()?;
    if registry.0.contains_key(&template.name) && !force {
        return Err(Error::TemplateExists(template.name));
    }
    registry.0.insert(template.name.clone(), template);
    save(&registry)
}

/// Look up one template by name.
pub fn get(name: &str) -> Result<CustomTemplate> {
    let registry = load()?;
    registry
        .0
        .get(name)
        .cloned()
        .ok_or_else(|| Error::UnknownTemplate(name.to_string(), known_list(&registry)))
}

/// All templates, sorted by name.
pub fn list() -> Result<Vec<CustomTemplate>> {
    Ok(load()?.0.into_values().collect())
}

/// Remove a template; errors when the name is unknown.
pub fn remove(name: &str) -> Result<CustomTemplate> {
    let mut registry = load()?;
    let entry = registry
        .0
        .remove(name)
        .ok_or_else(|| Error::UnknownTemplate(name.to_string(), known_list(&registry)))?;
    save(&registry)?;
    Ok(entry)
}

/// Combined catalogue string (builtins + customs) for error messages.
pub(crate) fn known_list(registry: &Registry) -> String {
    let mut ids: Vec<String> = Template::ALL
        .iter()
        .map(|t| format!("{} ({})", t.id(), t.short_alias()))
        .collect();
    ids.extend(registry.0.keys().cloned());
    ids.join(", ")
}

/// A template id resolved against builtins first, then customs.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ResolvedTemplate {
    /// One of the three hardcoded templates.
    Builtin(Template),
    /// A user-registered template.
    Custom(CustomTemplate),
}

impl ResolvedTemplate {
    /// Display id: builtin id or custom name.
    #[must_use]
    pub fn id(&self) -> String {
        match self {
            ResolvedTemplate::Builtin(t) => t.id().to_string(),
            ResolvedTemplate::Custom(c) => c.name.clone(),
        }
    }

    /// Whether the scaffolded project ships Rust code.
    #[must_use]
    pub fn needs_rust(&self) -> bool {
        match self {
            ResolvedTemplate::Builtin(t) => t.needs_rust(),
            ResolvedTemplate::Custom(c) => c.base.needs_rust(),
        }
    }
}

/// Resolve `id`: builtins (ids + short aliases) win over customs.
pub fn resolve_id(id: &str) -> Result<ResolvedTemplate> {
    if let Ok(builtin) = Template::parse_id(id) {
        return Ok(ResolvedTemplate::Builtin(builtin));
    }
    let registry = load()?;
    let normalized = id.trim().to_lowercase();
    if let Some(custom) = registry.0.get(&normalized).cloned() {
        return Ok(ResolvedTemplate::Custom(custom));
    }
    Err(Error::UnknownTemplate(
        id.to_string(),
        known_list(&registry),
    ))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_env_lock::ENV_LOCK;

    fn isolated_home(tag: &str) {
        let dir = std::env::temp_dir().join(format!(
            "swag-ctpl-test-{tag}-{}",
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .expect("time")
                .as_nanos()
        ));
        unsafe { std::env::set_var("SWAG_HOME", &dir) };
    }

    fn restore(old: Option<String>) {
        unsafe {
            match old {
                Some(v) => std::env::set_var("SWAG_HOME", v),
                None => std::env::remove_var("SWAG_HOME"),
            }
        }
    }

    #[test]
    fn rejects_builtin_collisions() {
        assert!(validate_name("blog-acme").is_ok());
        assert!(validate_name("clean").is_err());
        assert!(validate_name("svelte-clean-template").is_err());
        assert!(validate_name("Has-Upper").is_err());
        assert!(validate_name("").is_err());
    }

    #[test]
    fn add_get_remove_round_trip() {
        let _guard = ENV_LOCK.lock().unwrap();
        let old = std::env::var("SWAG_HOME").ok();
        isolated_home("crud");
        let tpl = CustomTemplate {
            name: "blog-acme".to_string(),
            base: Template::SvelteClean,
            description: "ACME fork".to_string(),
            source: TemplateSource::Git {
                url: "https://github.com/acme/blog-base.git".to_string(),
                git_ref: None,
            },
            created_at: None,
        };
        add(tpl.clone(), false).unwrap();
        assert!(add(tpl.clone(), false).is_err(), "duplicate without force");
        add(tpl.clone(), true).expect("force overwrite");
        let got = get("blog-acme").unwrap();
        assert_eq!(got.base, Template::SvelteClean);
        assert!(matches!(
            resolve_id("blog-acme"),
            Ok(ResolvedTemplate::Custom(_))
        ));
        // Builtin still wins over custom names that somehow match: parse first.
        assert!(matches!(
            resolve_id("clean"),
            Ok(ResolvedTemplate::Builtin(_))
        ));
        remove("blog-acme").unwrap();
        assert!(resolve_id("blog-acme").is_err());
        restore(old);
    }

    #[test]
    fn validates_sources() {
        let _guard = ENV_LOCK.lock().unwrap();
        let old = std::env::var("SWAG_HOME").ok();
        isolated_home("src");
        assert!(validate_git_url("https://github.com/a/b.git").is_ok());
        assert!(validate_git_url("git@github.com:a/b.git").is_ok());
        assert!(validate_git_url("ftp://nope").is_err());
        let missing = CustomTemplate {
            name: "x".to_string(),
            base: Template::SvelteClean,
            description: String::new(),
            source: TemplateSource::Path {
                path: PathBuf::from("/tmp/opencode/swag-definitely-missing-xyz"),
            },
            created_at: None,
        };
        assert!(add(missing, false).is_err());
        restore(old);
    }
}
