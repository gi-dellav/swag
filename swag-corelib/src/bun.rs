//! `bun` integration.
//!
//! Bun is the canonical tool for managing generated projects
//! (mirroring `~/svelte-clean-template/README.md`):
//! ```bash
//! bun install
//! bun run dev       # start the dev server
//! bun run check     # svelte-check type/diagnostics
//! bun run test      # unit tests with bun test
//! bun run build     # production build into dist/
//! ```
//! Every wrapper below shells out to the `bun` binary found on PATH.

use std::path::Path;
use std::process::Command;

use crate::error::{Error, Result};

/// Minimum supported bun major version (templates require bun >= 1.1).
const MIN_MAJOR: u64 = 1;
/// Minimum supported bun minor version.
const MIN_MINOR: u64 = 1;

/// Thin wrapper around the `bun` binary on PATH.
#[derive(Debug, Clone, Default)]
pub struct Bun {
    program: String,
}

impl Bun {
    /// Resolve `bun` from PATH and verify it meets the minimum version.
    pub fn new() -> Result<Self> {
        let bun = Self {
            program: "bun".to_string(),
        };
        bun.ensure_available()?;
        Ok(bun)
    }

    /// Resolve a custom binary path (useful for tests).
    #[cfg(test)]
    pub(crate) fn with_program(program: &str) -> Self {
        Self {
            program: program.to_string(),
        }
    }

    /// Fail with [`Error::BunNotFound`] if `bun --version` does not work
    /// or reports a version older than 1.1.
    pub fn ensure_available(&self) -> Result<()> {
        let output = Command::new(&self.program)
            .arg("--version")
            .output()
            .map_err(|e| Error::BunNotFound(e.to_string()))?;
        if !output.status.success() {
            return Err(Error::BunNotFound(format!(
                "`{} --version` exited with {}",
                self.program, output.status
            )));
        }
        let version = String::from_utf8_lossy(&output.stdout).trim().to_string();
        let (major, minor) = parse_version(&version)
            .ok_or_else(|| Error::BunNotFound(format!("could not parse version '{version}'")))?;
        if major < MIN_MAJOR || (major == MIN_MAJOR && minor < MIN_MINOR) {
            return Err(Error::BunNotFound(format!(
                "found {version}, requires bun >= {MIN_MAJOR}.{MIN_MINOR}"
            )));
        }
        Ok(())
    }

    /// Return the `bun --version` string.
    pub fn version(&self) -> Result<String> {
        let output = Command::new(&self.program)
            .arg("--version")
            .output()
            .map_err(|e| Error::CommandFailed(format!("bun --version: {e}")))?;
        if !output.status.success() {
            return Err(Error::CommandFailed(format!(
                "bun --version exited with {}",
                output.status
            )));
        }
        Ok(String::from_utf8_lossy(&output.stdout).trim().to_string())
    }

    /// `bun install` in `dir`.
    pub fn install(&self, dir: &Path) -> Result<()> {
        self.run(dir, &["install"])
    }

    /// `bun run dev` in `dir` (long-running, inherits stdio).
    pub fn dev(&self, dir: &Path, extra: &[String]) -> Result<()> {
        self.script(dir, "dev", extra)
    }

    /// `bun run build` in `dir`.
    pub fn build(&self, dir: &Path, extra: &[String]) -> Result<()> {
        self.script(dir, "build", extra)
    }

    /// `bun run check` (svelte-check) in `dir`.
    pub fn check(&self, dir: &Path, extra: &[String]) -> Result<()> {
        self.script(dir, "check", extra)
    }

    /// `bun run test` (`bun test`) in `dir`.
    pub fn test(&self, dir: &Path, extra: &[String]) -> Result<()> {
        self.script(dir, "test", extra)
    }

    /// `bun run preview` in `dir`.
    pub fn preview(&self, dir: &Path, extra: &[String]) -> Result<()> {
        self.script(dir, "preview", extra)
    }

    /// Run an arbitrary package script: `bun run <script> [-- extra...]`.
    pub fn script(&self, dir: &Path, script: &str, extra: &[String]) -> Result<()> {
        let mut args = vec!["run".to_string(), script.to_string()];
        if !extra.is_empty() {
            args.push("--".to_string());
            args.extend(extra.iter().cloned());
        }
        let arg_refs: Vec<&str> = args.iter().map(String::as_str).collect();
        self.run(dir, &arg_refs)
    }

    fn run(&self, dir: &Path, args: &[&str]) -> Result<()> {
        let status = Command::new(&self.program)
            .args(args)
            .current_dir(dir)
            .status()
            .map_err(|e| {
                Error::CommandFailed(format!(
                    "failed to spawn `{} {}` in '{}': {e}",
                    self.program,
                    args.join(" "),
                    dir.display()
                ))
            })?;
        if status.success() {
            Ok(())
        } else {
            Err(Error::CommandFailed(format!(
                "`{} {}` in '{}' exited with {status}",
                self.program,
                args.join(" "),
                dir.display()
            )))
        }
    }
}

fn parse_version(raw: &str) -> Option<(u64, u64)> {
    let mut parts = raw.split('.');
    let major = parts.next()?.parse().ok()?;
    let minor = parts.next().unwrap_or("0").parse().ok()?;
    Some((major, minor))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_bun_versions() {
        assert_eq!(parse_version("1.3.14"), Some((1, 3)));
        assert_eq!(parse_version("1.1"), Some((1, 1)));
        assert_eq!(parse_version("bogus"), None);
    }

    #[test]
    fn missing_binary_reports_bun_not_found() {
        let bun = Bun::with_program("swag-definitely-not-a-binary-xyz");
        let err = bun.ensure_available().unwrap_err();
        assert!(matches!(err, Error::BunNotFound(_)), "{err:?}");
    }
}
