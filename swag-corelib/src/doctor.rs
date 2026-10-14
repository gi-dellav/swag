//! `swag doctor`: read-only environment and project health checks.
//!
//! No agents, no network clones, no mutations (unless `--fix`, which only
//! creates `~/.swag` and prunes stale registry entries). Each check returns
//! a structured [`Check`] with a remediation hint; the CLI maps any `Fail`
//! to a non-zero exit code.

use std::path::Path;
use std::process::Command;

use serde::{Deserialize, Serialize};

/// Severity of a [`Check`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Status {
    Ok,
    Warn,
    Fail,
}

/// One health check result.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Check {
    /// Short id, e.g. `bun`, `home`, `project-type`.
    pub name: String,
    /// Severity.
    pub status: Status,
    /// Human-readable detail.
    pub message: String,
    /// What to do when not `Ok`.
    pub hint: String,
}

impl Check {
    fn ok(name: &str, message: impl Into<String>) -> Self {
        Self {
            name: name.to_string(),
            status: Status::Ok,
            message: message.into(),
            hint: String::new(),
        }
    }

    fn warn(name: &str, message: impl Into<String>, hint: impl Into<String>) -> Self {
        Self {
            name: name.to_string(),
            status: Status::Warn,
            message: message.into(),
            hint: hint.into(),
        }
    }

    fn fail(name: &str, message: impl Into<String>, hint: impl Into<String>) -> Self {
        Self {
            name: name.to_string(),
            status: Status::Fail,
            message: message.into(),
            hint: hint.into(),
        }
    }
}

/// Full doctor outcome.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DoctorReport {
    /// Individual checks, in display order.
    pub checks: Vec<Check>,
}

impl DoctorReport {
    /// True when no check failed (warnings are tolerated).
    #[must_use]
    pub fn healthy(&self) -> bool {
        !self.checks.iter().any(|c| c.status == Status::Fail)
    }
}

/// Run all checks; `project_dir` adds project-scoped checks when given.
#[must_use]
pub fn doctor(project_dir: Option<&Path>) -> DoctorReport {
    let mut checks = vec![check_bun(), check_git(), check_cargo(), check_home()];
    match project_dir {
        Some(dir) => checks.extend(check_project(dir)),
        None => checks.push(Check::ok(
            "project",
            "no project given (pass a directory for project checks)",
        )),
    }
    DoctorReport { checks }
}

/// Safe repairs: create `~/.swag` and prune stale registry entries.
/// Returns human-readable lines describing what was done.
pub fn fix() -> crate::error::Result<Vec<String>> {
    let mut done = Vec::new();
    let home = crate::home::ensure_home()?;
    done.push(format!("home OK: {}", home.display()));
    let pruned = crate::projects::prune()?;
    if pruned.is_empty() {
        done.push("nothing to prune".to_string());
    } else {
        for entry in pruned {
            done.push(format!("pruned {} ({})", entry.name, entry.path.display()));
        }
    }
    Ok(done)
}

fn check_bun() -> Check {
    match crate::Bun::new() {
        Ok(bun) => match bun.version() {
            Ok(version) => Check::ok("bun", format!("bun {version}")),
            Err(e) => Check::fail("bun", format!("{e}"), "install bun >= 1.1 (https://bun.sh)"),
        },
        Err(e) => Check::fail("bun", format!("{e}"), "install bun >= 1.1 (https://bun.sh)"),
    }
}

fn check_git() -> Check {
    match Command::new("git").arg("--version").output() {
        Ok(output) if output.status.success() => Check::ok(
            "git",
            String::from_utf8_lossy(&output.stdout).trim().to_string(),
        ),
        _ => Check::fail(
            "git",
            "git not found on PATH",
            "install git (needed to fetch templates)",
        ),
    }
}

fn check_cargo() -> Check {
    match Command::new("cargo").arg("--version").output() {
        Ok(output) if output.status.success() => Check::ok(
            "cargo",
            String::from_utf8_lossy(&output.stdout).trim().to_string(),
        ),
        _ => Check::warn(
            "cargo",
            "cargo not found on PATH",
            "install Rust via rustup (needed for backend/desktop templates only)",
        ),
    }
}

fn check_home() -> Check {
    match crate::home::swag_home() {
        Ok(home) => {
            if home.exists() && !home.is_dir() {
                return Check::fail(
                    "home",
                    format!("{} exists but is not a directory", home.display()),
                    "move it aside or set $SWAG_HOME elsewhere",
                );
            }
            match crate::projects::list() {
                Ok(_) => match crate::custom_templates::list() {
                    Ok(_) => Check::ok("home", format!("{}", home.display())),
                    Err(e) => Check::fail(
                        "home",
                        format!("templates.json unreadable: {e}"),
                        "fix the JSON or delete it (see `swag templates`)",
                    ),
                },
                Err(e) => Check::fail(
                    "home",
                    format!("projects.json unreadable: {e}"),
                    "fix the JSON or delete it (see `swag projects`)",
                ),
            }
        }
        Err(e) => Check::fail(
            "home",
            format!("{e}"),
            "set $SWAG_HOME to a writable directory",
        ),
    }
}

fn check_project(dir: &Path) -> Vec<Check> {
    let mut out = Vec::new();
    if !dir.exists() {
        return vec![Check::fail(
            "project-dir",
            format!("'{}' does not exist", dir.display()),
            "pass an existing project directory or registered name",
        )];
    }
    out.push(Check::ok("project-dir", format!("{}", dir.display())));

    match crate::provenance::load(dir) {
        Ok(prov) => out.push(Check::ok(
            "project-provenance",
            format!("{} from {}", prov.name, prov.template),
        )),
        Err(_) => out.push(Check::warn(
            "project-provenance",
            "no .swag.json (legacy project)",
            "use `swag update --dry-run` with a registry entry, or re-scaffold",
        )),
    }

    version_check(dir, &mut out);
    type_check(dir, &mut out);
    layout_checks(dir, &mut out);
    out
}

fn version_check(dir: &Path, out: &mut Vec<Check>) {
    let raw = std::fs::read_to_string(dir.join(crate::template::TEMPLATE_VERSION_FILE));
    match raw {
        Err(_) => out.push(Check::warn(
            "project-version",
            "no .template_version (legacy template)",
            "update the template to ship a numeric .template_version",
        )),
        Ok(content) => match crate::template::parse_template_version(&content) {
            Some(version) => out.push(Check::ok("project-version", format!("template v{version}"))),
            None => out.push(Check::fail(
                "project-version",
                format!(
                    "{} is not a number: {:?}",
                    crate::template::TEMPLATE_VERSION_FILE,
                    content.trim()
                ),
                "template must write a single number (e.g. 21) into .template_version",
            )),
        },
    }
}

fn type_check(dir: &Path, out: &mut Vec<Check>) {
    let raw = std::fs::read_to_string(dir.join(crate::template::TEMPLATE_TYPE_FILE));
    match raw {
        Err(_) => out.push(Check::warn(
            "project-type",
            "no .template_type (legacy template)",
            "template should ship frontend|backend|desktop in .template_type",
        )),
        Ok(content) => match crate::template::TemplateType::parse(&content) {
            Some(kind) => {
                out.push(Check::ok("project-type", kind.as_str().to_string()));
                if kind.needs_rust()
                    && !dir.join("Cargo.toml").exists()
                    && !dir.join("src-tauri").exists()
                    && !dir.join("backend").exists()
                {
                    out.push(Check::warn(
                        "project-rust",
                        format!("type {kind} but no Rust layout found"),
                        "expect Cargo.toml, src-tauri/ or backend/ for backend/desktop projects",
                    ));
                }
                if kind == crate::template::TemplateType::Desktop
                    && !dir.join("src-tauri").join("tauri.conf.json").exists()
                    && !dir.join("src-tauri").exists()
                {
                    out.push(Check::warn(
                        "project-tauri",
                        "desktop project without src-tauri/",
                        "check the template ships src-tauri/tauri.conf.json",
                    ));
                }
            }
            None => out.push(Check::fail(
                "project-type",
                format!("unknown type: {:?}", content.trim()),
                "template must write frontend, backend or desktop into .template_type",
            )),
        },
    }
    if !dir.join("package.json").exists() {
        out.push(Check::fail(
            "project-package",
            "no package.json",
            "not a bun-managed project (run `swag new` or restore the file)",
        ));
    }
    if !dir.join("node_modules").exists() {
        out.push(Check::warn(
            "project-deps",
            "no node_modules/",
            "run `bun install` in the project directory",
        ));
    }
}

fn layout_checks(dir: &Path, out: &mut Vec<Check>) {
    let git = Command::new("git")
        .args(["status", "--porcelain"])
        .current_dir(dir)
        .output();
    if let Ok(git) = git
        && git.status.success()
        && !String::from_utf8_lossy(&git.stdout).trim().is_empty()
    {
        out.push(Check::warn(
            "project-git",
            "uncommitted changes",
            "commit or stash before `swag update` (or use --force)",
        ));
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    fn stamp(tag: &str) -> PathBuf {
        std::env::temp_dir().join(format!(
            "swag-doctor-test-{tag}-{}",
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .expect("time")
                .as_nanos()
        ))
    }

    #[test]
    fn healthy_project_passes() {
        let dir = stamp("ok");
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join("package.json"), "{}").unwrap();
        std::fs::write(dir.join(".template_version"), "21\n").unwrap();
        std::fs::write(dir.join(".template_type"), "frontend\n").unwrap();
        let report = doctor(Some(&dir));
        assert!(
            report
                .checks
                .iter()
                .any(|c| c.name == "project-version" && c.status == Status::Ok)
        );
        assert!(
            report
                .checks
                .iter()
                .any(|c| c.name == "project-type" && c.status == Status::Ok)
        );
        assert!(
            !report
                .checks
                .iter()
                .any(|c| c.name == "project-package" && c.status == Status::Fail)
        );
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn bad_version_and_type_fail() {
        let dir = stamp("bad");
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join("package.json"), "{}").unwrap();
        std::fs::write(dir.join(".template_version"), "bogus\n").unwrap();
        std::fs::write(dir.join(".template_type"), "mobile\n").unwrap();
        let report = doctor(Some(&dir));
        assert!(
            report
                .checks
                .iter()
                .any(|c| c.name == "project-version" && c.status == Status::Fail)
        );
        assert!(
            report
                .checks
                .iter()
                .any(|c| c.name == "project-type" && c.status == Status::Fail)
        );
        assert!(!report.healthy());
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn legacy_project_warns_not_fails() {
        let dir = stamp("legacy");
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join("package.json"), "{}").unwrap();
        let report = doctor(Some(&dir));
        assert!(
            report
                .checks
                .iter()
                .any(|c| c.name == "project-version" && c.status == Status::Warn)
        );
        assert!(
            report
                .checks
                .iter()
                .any(|c| c.name == "project-type" && c.status == Status::Warn)
        );
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn missing_dir_fails() {
        let missing = std::env::temp_dir().join("swag-definitely-missing-xyz");
        let report = doctor(Some(&missing));
        assert!(!report.healthy());
    }

    #[test]
    fn env_checks_present() {
        let report = doctor(None);
        for name in ["bun", "git", "cargo", "home"] {
            assert!(report.checks.iter().any(|c| c.name == name), "{name}");
        }
    }
}
