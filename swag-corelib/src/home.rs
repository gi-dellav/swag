//! `~/.swag` home directory.
//!
//! Resolution order:
//! 1. `$SWAG_HOME` when set and non-empty (tests / power users);
//! 2. Unix: `$HOME/.swag`;
//! 3. Windows: `%USERPROFILE%\.swag`, falling back to `%APPDATA%\swag`.
//!
//! The home holds `projects.json` and `templates.json`, written atomically
//! (write `<file>.tmp` + `rename`) so a crash never leaves half-written JSON.

use std::fs;
use std::io::Write;
use std::path::PathBuf;

use crate::error::{Error, Result};

/// Name of the projects registry file inside the home directory.
pub const PROJECTS_FILE: &str = "projects.json";
/// Name of the custom templates registry file inside the home directory.
pub const TEMPLATES_FILE: &str = "templates.json";

/// Resolve the swag home directory (see module docs).
pub fn swag_home() -> Result<PathBuf> {
    if let Ok(dir) = std::env::var("SWAG_HOME")
        && !dir.trim().is_empty()
    {
        return Ok(PathBuf::from(dir));
    }
    #[cfg(windows)]
    {
        if let Ok(profile) = std::env::var("USERPROFILE")
            && !profile.trim().is_empty()
        {
            return Ok(PathBuf::from(profile).join(".swag"));
        }
        if let Ok(appdata) = std::env::var("APPDATA")
            && !appdata.trim().is_empty()
        {
            return Ok(PathBuf::from(appdata).join("swag"));
        }
        return Err(Error::HomeNotFound(
            "set the SWAG_HOME environment variable".to_string(),
        ));
    }
    #[cfg(not(windows))]
    {
        if let Ok(home) = std::env::var("HOME")
            && !home.trim().is_empty()
        {
            return Ok(PathBuf::from(home).join(".swag"));
        }
        Err(Error::HomeNotFound(
            "set the SWAG_HOME environment variable".to_string(),
        ))
    }
}

/// Path of the projects registry file.
pub fn projects_path() -> Result<PathBuf> {
    Ok(swag_home()?.join(PROJECTS_FILE))
}

/// Path of the custom templates registry file.
pub fn templates_path() -> Result<PathBuf> {
    Ok(swag_home()?.join(TEMPLATES_FILE))
}

/// Create the home directory if it does not exist.
pub fn ensure_home() -> Result<PathBuf> {
    let home = swag_home()?;
    fs::create_dir_all(&home).map_err(|e| Error::Io {
        context: format!("could not create swag home '{}'", home.display()),
        source: e,
    })?;
    Ok(home)
}

/// Serialize `value` as pretty JSON and write it atomically to `path`.
pub fn write_json_atomic<T: serde::Serialize>(path: &std::path::Path, value: &T) -> Result<()> {
    let parent = path.parent().unwrap_or_else(|| std::path::Path::new("."));
    fs::create_dir_all(parent).map_err(|e| Error::Io {
        context: format!("could not create '{}'", parent.display()),
        source: e,
    })?;
    let rendered = serde_json::to_string_pretty(value)
        .map_err(|e| Error::Internal(format!("could not render JSON: {e}")))?;
    // `<file>.tmp` beside the target (same filesystem → rename is atomic).
    let tmp = path.with_file_name(format!(
        "{}.tmp",
        path.file_name()
            .map(|s| s.to_string_lossy().into_owned())
            .unwrap_or_else(|| "swag".to_string())
    ));
    let mut file = fs::File::create(&tmp).map_err(|e| Error::Io {
        context: format!("could not write '{}'", tmp.display()),
        source: e,
    })?;
    file.write_all(rendered.as_bytes()).map_err(|e| Error::Io {
        context: format!("could not write '{}'", tmp.display()),
        source: e,
    })?;
    // Flush the data before renaming, then flush the directory entry so the
    // rename itself survives a crash (atomic vs torn writes is not enough).
    file.sync_all().map_err(|e| Error::Io {
        context: format!("could not flush '{}'", tmp.display()),
        source: e,
    })?;
    drop(file);
    fs::rename(&tmp, path).map_err(|e| Error::Io {
        context: format!("could not write '{}'", path.display()),
        source: e,
    })?;
    #[cfg(unix)]
    if let Ok(dir) = fs::File::open(parent) {
        let _ = dir.sync_all();
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_env_lock::ENV_LOCK;

    #[test]
    fn swag_home_env_override_wins() {
        let _guard = ENV_LOCK.lock().unwrap();
        let old = std::env::var("SWAG_HOME").ok();
        let target = std::env::temp_dir().join("swag-home-test");
        unsafe { std::env::set_var("SWAG_HOME", &target) };
        assert_eq!(swag_home().unwrap(), target);
        unsafe {
            match old {
                Some(v) => std::env::set_var("SWAG_HOME", v),
                None => std::env::remove_var("SWAG_HOME"),
            }
        }
    }
}
