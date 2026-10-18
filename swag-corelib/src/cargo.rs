//! `cargo` integration.
//!
//! Rust-bearing templates (fullstack `backend/`, desktop `src-tauri/`) are
//! driven through the `cargo` binary on PATH. The task planner builds the
//! concrete argument vectors; this wrapper only owns toolchain resolution and
//! a few direct helpers used by the scaffolder and tests.

use std::path::Path;
use std::process::Command;

use crate::error::{Error, Result};

/// Thin wrapper around the `cargo` binary on PATH.
#[derive(Debug, Clone, Default)]
pub struct Cargo {
    program: String,
}

impl Cargo {
    /// Resolve `cargo` from PATH and verify it runs.
    pub fn new() -> Result<Self> {
        let cargo = Self {
            program: "cargo".to_string(),
        };
        cargo.ensure_available()?;
        Ok(cargo)
    }

    /// Resolve a custom binary path (useful for tests).
    #[cfg(test)]
    pub(crate) fn with_program(program: &str) -> Self {
        Self {
            program: program.to_string(),
        }
    }

    /// Fail with [`Error::CargoNotFound`] when `cargo --version` does not run.
    pub fn ensure_available(&self) -> Result<()> {
        let output = Command::new(&self.program)
            .arg("--version")
            .output()
            .map_err(|e| Error::CargoNotFound(e.to_string()))?;
        if !output.status.success() {
            return Err(Error::CargoNotFound(format!(
                "`{} --version` exited with {}",
                self.program, output.status
            )));
        }
        Ok(())
    }

    /// Return the `cargo --version` string.
    pub fn version(&self) -> Result<String> {
        let output = Command::new(&self.program)
            .arg("--version")
            .output()
            .map_err(|e| Error::CommandFailed(format!("cargo --version: {e}")))?;
        if !output.status.success() {
            return Err(Error::CommandFailed(format!(
                "cargo --version exited with {}",
                output.status
            )));
        }
        Ok(String::from_utf8_lossy(&output.stdout).trim().to_string())
    }

    /// `cargo fetch` in `dir` (`--locked` when a lockfile is present).
    pub fn fetch(&self, dir: &Path) -> Result<()> {
        let mut args = vec!["fetch"];
        if dir.join("Cargo.lock").is_file() {
            args.push("--locked");
        }
        self.run(dir, &args)
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn missing_binary_reports_cargo_not_found() {
        let cargo = Cargo::with_program("swag-definitely-not-a-cargo-binary-xyz");
        let err = cargo.ensure_available().unwrap_err();
        assert!(matches!(err, Error::CargoNotFound(_)), "{err:?}");
    }
}
