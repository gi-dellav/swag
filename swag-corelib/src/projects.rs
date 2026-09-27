//! Project registry (`projects.json` in the swag home).
//!
//! A map of project name → [`ProjectEntry`]. `swag new` auto-registers;
//! `swag projects` manages entries explicitly.

use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::error::{Error, Result};
use crate::home;

/// A registered project.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ProjectEntry {
    /// Registry key; same rules as scaffold project names.
    pub name: String,
    /// Absolute path of the project root.
    pub path: PathBuf,
    /// Template id it was created from (builtin id or custom name).
    pub template: String,
    /// GitHub owner used at scaffold time, if any.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub owner: Option<String>,
    /// RFC 3339 creation timestamp (seconds precision, UTC).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub created_at: Option<String>,
    /// RFC 3339 last-use timestamp (bumped on `new`, `add`, `cd`).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub last_used: Option<String>,
}

/// The on-disk registry: a name → entry map (empty file / missing = empty).
#[derive(Debug, Default, Clone, Serialize, Deserialize)]
pub struct Registry(pub BTreeMap<String, ProjectEntry>);

/// Current UTC time as `YYYY-MM-DDTHH:MM:SSZ` (no extra deps).
pub(crate) fn now_utc() -> String {
    // Use /proc-free approach: derive from SystemTime via chrono-less formatting.
    // Format manually from UNIX seconds using a days-based civil-date algorithm.
    let secs = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0);
    format_unix_secs(secs)
}

fn format_unix_secs(secs: u64) -> String {
    let (h, m, s) = ((secs / 3600) % 24, (secs / 60) % 60, secs % 60);
    let mut days = (secs / 86400) as i64;
    // Howard Hinnant's civil_from_days, shifted to 1970-01-01.
    days += 719468;
    let era = if days >= 0 { days } else { days - 146096 } / 146097;
    let doe = (days - era * 146097) as u64; // [0, 146096]
    let yoe = (doe - doe / 1460 + doe / 36524 - doe / 146096) / 365; // [0, 399]
    let mut y = yoe as i64 + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100); // [0, 365]
    let mp = (5 * doy + 2) / 153; // [0, 11]
    let d = doy - (153 * mp + 2) / 5 + 1; // [1, 31]
    let mth = if mp < 10 { mp + 3 } else { mp - 9 }; // [1, 12]
    y += i64::from(mth <= 2);
    format!("{y:04}-{:02}-{:02}T{h:02}:{m:02}:{s:02}Z", mth, d)
}

/// Load the registry; missing file → empty.
pub fn load() -> Result<Registry> {
    let path = home::projects_path()?;
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
    home::write_json_atomic(&home::projects_path()?, registry)
}

/// Resolve `raw` to an absolute path (canonicalize when it exists).
pub fn resolve_path(raw: &Path) -> Result<PathBuf> {
    if let Ok(canonical) = raw.canonicalize() {
        return Ok(canonical);
    }
    if raw.is_absolute() {
        return Ok(raw.to_path_buf());
    }
    let cwd = std::env::current_dir().map_err(|e| Error::Io {
        context: "could not determine current directory".to_string(),
        source: e,
    })?;
    Ok(cwd.join(raw))
}

/// Insert or replace an entry; returns the previous entry, if any.
pub fn upsert(
    name: &str,
    path: &Path,
    template: &str,
    owner: Option<&str>,
) -> Result<Option<ProjectEntry>> {
    let mut registry = load()?;
    let now = now_utc();
    let previous = registry.0.get(name).cloned();
    let created_at = previous
        .as_ref()
        .and_then(|p| p.created_at.clone())
        .or(Some(now.clone()));
    registry.0.insert(
        name.to_string(),
        ProjectEntry {
            name: name.to_string(),
            path: resolve_path(path)?,
            template: template.to_string(),
            owner: owner.map(str::to_string),
            created_at,
            last_used: Some(now),
        },
    );
    save(&registry)?;
    Ok(previous)
}

/// Look up one entry by name; bumps nothing.
pub fn get(name: &str) -> Result<ProjectEntry> {
    let registry = load()?;
    registry
        .0
        .get(name)
        .cloned()
        .ok_or_else(|| Error::ProjectNotFound(name.to_string()))
}

/// All entries, sorted by name.
pub fn list() -> Result<Vec<ProjectEntry>> {
    Ok(load()?.0.into_values().collect())
}

/// Remove an entry; errors when the name is unknown.
pub fn remove(name: &str) -> Result<ProjectEntry> {
    let mut registry = load()?;
    let entry = registry
        .0
        .remove(name)
        .ok_or_else(|| Error::ProjectNotFound(name.to_string()))?;
    save(&registry)?;
    Ok(entry)
}

/// Bump `last_used` for an entry; errors when unknown.
pub fn touch(name: &str) -> Result<ProjectEntry> {
    let mut registry = load()?;
    let entry = registry
        .0
        .get_mut(name)
        .ok_or_else(|| Error::ProjectNotFound(name.to_string()))?;
    entry.last_used = Some(now_utc());
    let entry = entry.clone();
    save(&registry)?;
    Ok(entry)
}

/// Drop entries whose path no longer exists; returns the pruned entries.
pub fn prune() -> Result<Vec<ProjectEntry>> {
    let mut registry = load()?;
    let mut pruned = Vec::new();
    registry.0.retain(|_, entry| {
        if entry.path.exists() {
            true
        } else {
            pruned.push(entry.clone());
            false
        }
    });
    pruned.sort_by(|a, b| a.name.cmp(&b.name));
    if !pruned.is_empty() {
        save(&registry)?;
    }
    Ok(pruned)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_env_lock::ENV_LOCK;

    fn isolated_home(tag: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!(
            "swag-projects-test-{tag}-{}",
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .expect("time")
                .as_nanos()
        ));
        unsafe { std::env::set_var("SWAG_HOME", &dir) };
        dir
    }

    #[test]
    fn crud_round_trip() {
        let _guard = ENV_LOCK.lock().unwrap();
        let old = std::env::var("SWAG_HOME").ok();
        let dir = isolated_home("crud");
        let target = std::env::temp_dir();
        upsert("demo", &target, "clean", None).unwrap();
        let entry = get("demo").unwrap();
        assert_eq!(entry.template, "clean");
        assert!(entry.path.is_absolute());
        assert_eq!(list().unwrap().len(), 1);
        remove("demo").unwrap();
        assert!(get("demo").is_err());
        assert!(list().unwrap().is_empty());
        drop(dir);
        unsafe {
            match old {
                Some(v) => std::env::set_var("SWAG_HOME", v),
                None => std::env::remove_var("SWAG_HOME"),
            }
        }
    }

    #[test]
    fn formats_timestamps_sanely() {
        assert_eq!(format_unix_secs(0), "1970-01-01T00:00:00Z");
        let now = now_utc();
        assert!(now.starts_with("202"), "{now}");
        assert!(now.ends_with('Z'), "{now}");
    }
}
