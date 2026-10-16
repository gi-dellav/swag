//! `swag vc`: deploy frontend/backend projects to Vercel.
//!
//! Thin wrapper over the `vercel` CLI (mirroring [`crate::bun::Bun`]:
//! resolve from PATH, verify with `--version`, shell out with the project
//! directory as CWD). Remote builds only: `vercel link` when the project is
//! not linked yet, then `vercel deploy` from the project root, so the
//! template's own `vercel.json` (fullstack) or framework auto-detection
//! (frontend) drives the build. Desktop (Tauri) projects are refused.
//!
//! Auth follows the Vercel CLI: an existing login, `VERCEL_TOKEN` in the
//! environment (preferred for CI/agents), or `--token`. The token value is
//! never printed; see [`display_args`].

use std::fs;
use std::path::Path;
use std::process::Command;

use crate::custom_templates::ResolvedTemplate;
use crate::error::{Error, Result};
use crate::template::TemplateType;

/// Which Vercel deployment shape a project uses.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DeployKind {
    /// Static webapp at the project root (Vercel auto-detects Vite).
    Frontend,
    /// Fullstack: root `vercel.json` services (static frontend + container backend).
    Backend,
}

/// Options for `vercel link`.
#[derive(Debug, Clone, Default)]
pub struct LinkOptions {
    /// Answer prompts with defaults (`--yes`).
    pub yes: bool,
    /// Vercel project name or id (`--project`).
    pub project: Option<String>,
    /// Auth token (`--token`); prefer `VERCEL_TOKEN` in the environment for CI.
    pub token: Option<String>,
    /// Scope slug (`--scope`).
    pub scope: Option<String>,
    /// Team slug or id (`--team`).
    pub team: Option<String>,
}

/// Options for `vercel deploy` (remote build).
#[derive(Debug, Clone, Default)]
pub struct DeployOptions {
    /// Production deploy (`--prod`); default is a preview deployment.
    pub prod: bool,
    /// Skip prompts (`--yes`).
    pub yes: bool,
    /// Vercel project name or id (`--project`).
    pub project: Option<String>,
    /// Auth token (`--token`); prefer `VERCEL_TOKEN` in the environment for CI.
    pub token: Option<String>,
    /// Scope slug (`--scope`).
    pub scope: Option<String>,
    /// Team slug or id (`--team`).
    pub team: Option<String>,
    /// Runtime env vars (`--env KEY=val`, repeatable).
    pub env: Vec<String>,
    /// Build-time env vars (`--build-env KEY=val`, repeatable).
    pub build_env: Vec<String>,
    /// Return without waiting for the deployment (`--no-wait`).
    pub no_wait: bool,
    /// Ignore the build cache (`--force`).
    pub force: bool,
    /// Extra args forwarded verbatim to `vercel deploy`.
    pub extra: Vec<String>,
}

/// Thin wrapper around the `vercel` binary on PATH.
#[derive(Debug, Clone, Default)]
pub struct Vercel {
    program: String,
}

impl Vercel {
    /// Resolve `vercel` from PATH and verify it runs.
    pub fn new() -> Result<Self> {
        let vercel = Self {
            program: "vercel".to_string(),
        };
        vercel.ensure_available()?;
        Ok(vercel)
    }

    /// Resolve a custom binary path (useful for tests).
    #[cfg(test)]
    pub(crate) fn with_program(program: &str) -> Self {
        Self {
            program: program.to_string(),
        }
    }

    /// Fail with [`Error::VercelNotFound`] if `vercel --version` does not work.
    pub fn ensure_available(&self) -> Result<()> {
        let output = Command::new(&self.program)
            .arg("--version")
            .output()
            .map_err(|e| Error::VercelNotFound(e.to_string()))?;
        if !output.status.success() {
            return Err(Error::VercelNotFound(format!(
                "`{} --version` exited with {}",
                self.program, output.status
            )));
        }
        Ok(())
    }

    /// Return the `vercel --version` string.
    pub fn version(&self) -> Result<String> {
        let output = Command::new(&self.program)
            .arg("--version")
            .output()
            .map_err(|e| Error::CommandFailed(format!("vercel --version: {e}")))?;
        if !output.status.success() {
            return Err(Error::CommandFailed(format!(
                "vercel --version exited with {}",
                output.status
            )));
        }
        Ok(String::from_utf8_lossy(&output.stdout).trim().to_string())
    }

    /// True when `dir` is already linked (`.vercel/project.json` exists).
    #[must_use]
    pub fn is_linked(dir: &Path) -> bool {
        dir.join(".vercel").join("project.json").is_file()
    }

    /// `vercel link [...]` in `dir` (long-running / interactive: inherits stdio).
    pub fn link(&self, dir: &Path, opts: &LinkOptions) -> Result<()> {
        let args = link_args(opts)?;
        let status = Command::new(&self.program)
            .args(&args)
            .current_dir(dir)
            .status()
            .map_err(|e| {
                Error::CommandFailed(format!(
                    "failed to spawn `{} {}` in '{}': {e}",
                    self.program,
                    display_args(&args),
                    dir.display()
                ))
            })?;
        if status.success() {
            Ok(())
        } else {
            Err(Error::CommandFailed(format!(
                "`{} {}` in '{}' exited with {status}",
                self.program,
                display_args(&args),
                dir.display()
            )))
        }
    }

    /// `vercel deploy [...]` in `dir`; returns the deployment URL from stdout.
    ///
    /// Build progress (stderr) is forwarded as-is; the URL (stdout) is
    /// returned so the CLI can print it bare for scripting (`swag vc > url.txt`).
    pub fn deploy(&self, dir: &Path, opts: &DeployOptions) -> Result<String> {
        let args = deploy_args(opts)?;
        let output = Command::new(&self.program)
            .args(&args)
            .current_dir(dir)
            .output()
            .map_err(|e| {
                Error::CommandFailed(format!(
                    "failed to spawn `{} {}` in '{}': {e}",
                    self.program,
                    display_args(&args),
                    dir.display()
                ))
            })?;
        let stderr = String::from_utf8_lossy(&output.stderr);
        if !stderr.is_empty() {
            eprint!("{stderr}");
            if !stderr.ends_with('\n') {
                eprintln!();
            }
        }
        if !output.status.success() {
            return Err(Error::CommandFailed(format!(
                "`{} {}` in '{}' exited with {}",
                self.program,
                display_args(&args),
                dir.display(),
                output.status
            )));
        }
        deployment_url(&String::from_utf8_lossy(&output.stdout))
    }
}

/// Determine whether `dir` deploys as a static frontend or a fullstack backend.
///
/// Reads `.swag.json` provenance (falling back to `.template_type`, so legacy
/// and custom-base projects work), and refuses desktop (Tauri) projects:
/// Vercel cannot run a desktop app.
pub fn detect_kind(dir: &Path) -> Result<DeployKind> {
    match template_type_for_dir(dir)? {
        TemplateType::Frontend => Ok(DeployKind::Frontend),
        TemplateType::Backend => Ok(DeployKind::Backend),
        TemplateType::Desktop => Err(Error::CommandFailed(format!(
            "swag vc does not support desktop (Tauri) projects: '{}' is a desktop app, which Vercel cannot run (ship it with cargo/tauri instead)",
            dir.display()
        ))),
    }
}

/// Check the project layout for a Vercel deploy; returns advisory notes.
///
/// Hard errors (missing `package.json`, `vercel.json`, service roots) fail;
/// dashboard-only state (env vars) is returned as notes the CLI prints.
pub fn preflight(dir: &Path, kind: &DeployKind) -> Result<Vec<String>> {
    let mut notes = Vec::new();
    match kind {
        DeployKind::Frontend => {
            if !dir.join("package.json").is_file() {
                return Err(Error::NotAProject(format!(
                    "no package.json in '{}' (not a frontend project; see `swag doctor`)",
                    dir.display()
                )));
            }
        }
        DeployKind::Backend => {
            let raw = fs::read_to_string(dir.join("vercel.json")).map_err(|_| {
                Error::CommandFailed(format!(
                    "backend projects need vercel.json at the project root ('{}' has none; see knowledge/vercel-deploy.md in the template)",
                    dir.display()
                ))
            })?;
            let parsed: serde_json::Value = serde_json::from_str(&raw).map_err(|e| {
                Error::CommandFailed(format!(
                    "vercel.json in '{}' is not valid JSON: {e}",
                    dir.display()
                ))
            })?;
            let services = parsed.get("services");
            let has_frontend = services.and_then(|s| s.get("frontend")).is_some();
            let has_backend = services.and_then(|s| s.get("backend")).is_some();
            if !(has_frontend && has_backend) {
                return Err(Error::CommandFailed(format!(
                    "vercel.json in '{}' must define frontend + backend services (see knowledge/vercel-deploy.md in the template)",
                    dir.display()
                )));
            }
            for sub in ["frontend", "backend"] {
                if !dir.join(sub).is_dir() {
                    return Err(Error::CommandFailed(format!(
                        "'{}' is missing the '{sub}/' service root declared in vercel.json",
                        dir.display()
                    )));
                }
            }
            if !dir.join("backend").join("Dockerfile.vercel").is_file() {
                return Err(Error::CommandFailed(format!(
                    "'{}' is missing backend/Dockerfile.vercel (the container Function entrypoint; see knowledge/vercel-deploy.md in the template)",
                    dir.display()
                )));
            }
            notes.push(
                "set dashboard env (Production + Preview): SITE_URL, ALLOWED_ORIGINS, DATABASE_URL (or `none`), RUST_LOG — see knowledge/vercel-deploy.md"
                    .to_string(),
            );
        }
    }
    Ok(notes)
}

/// Resolve the [`TemplateType`] of `dir`: provenance first (resolved template
/// wins so custom templates inherit their base), then `.template_type`.
fn template_type_for_dir(dir: &Path) -> Result<TemplateType> {
    if let Ok(prov) = crate::provenance::load(dir) {
        let resolved_type = crate::custom_templates::resolve_id(&prov.template)
            .ok()
            .map(|resolved| match resolved {
                ResolvedTemplate::Builtin(t) => t.template_type(),
                ResolvedTemplate::Custom(c) => c.base.template_type(),
            });
        if let Some(kind) = resolved_type.or(prov.template_type) {
            return Ok(kind);
        }
    }
    if let Some(kind) = crate::template::read_template_type(dir) {
        return Ok(kind);
    }
    Err(Error::NotAProject(format!(
        "cannot determine project type for '{}' (no .swag.json or .template_type)",
        dir.display()
    )))
}

/// Build the `vercel link` argument list (validated).
pub(crate) fn link_args(opts: &LinkOptions) -> Result<Vec<String>> {
    let mut args = vec!["link".to_string()];
    if opts.yes {
        args.push("--yes".to_string());
    }
    push_flag(&mut args, "--project", opts.project.as_deref())?;
    push_flag(&mut args, "--token", opts.token.as_deref())?;
    push_flag(&mut args, "--scope", opts.scope.as_deref())?;
    push_flag(&mut args, "--team", opts.team.as_deref())?;
    Ok(args)
}

/// Build the `vercel deploy` argument list (validated).
pub(crate) fn deploy_args(opts: &DeployOptions) -> Result<Vec<String>> {
    let mut args = vec!["deploy".to_string()];
    if opts.prod {
        args.push("--prod".to_string());
    }
    if opts.yes {
        args.push("--yes".to_string());
    }
    push_flag(&mut args, "--project", opts.project.as_deref())?;
    push_flag(&mut args, "--token", opts.token.as_deref())?;
    push_flag(&mut args, "--scope", opts.scope.as_deref())?;
    push_flag(&mut args, "--team", opts.team.as_deref())?;
    for pair in &opts.env {
        validate_env_pair("--env", pair)?;
        args.push("--env".to_string());
        args.push(pair.clone());
    }
    for pair in &opts.build_env {
        validate_env_pair("--build-env", pair)?;
        args.push("--build-env".to_string());
        args.push(pair.clone());
    }
    if opts.no_wait {
        args.push("--no-wait".to_string());
    }
    if opts.force {
        args.push("--force".to_string());
    }
    args.extend(opts.extra.iter().cloned());
    Ok(args)
}

/// Push `--flag value` (validated); `None` pushes nothing.
fn push_flag(args: &mut Vec<String>, flag: &str, value: Option<&str>) -> Result<()> {
    if let Some(value) = value {
        validate_flag_value(flag, value)?;
        args.push(flag.to_string());
        args.push(value.to_string());
    }
    Ok(())
}

/// Reject empty values and values that would parse as CLI options
/// (argument injection, same guard style as template git URLs/refs).
fn validate_flag_value(flag: &str, value: &str) -> Result<()> {
    if value.is_empty() {
        return Err(Error::CommandFailed(format!("{flag} must not be empty")));
    }
    if value.starts_with('-') {
        return Err(Error::CommandFailed(format!(
            "{flag} value {value:?} must not start with '-'"
        )));
    }
    Ok(())
}

/// `--env` / `--build-env` must be `KEY=value` with a non-empty key.
fn validate_env_pair(flag: &str, pair: &str) -> Result<()> {
    let valid = pair
        .split_once('=')
        .is_some_and(|(key, _)| !key.is_empty() && !key.contains(' '));
    if valid {
        Ok(())
    } else {
        Err(Error::CommandFailed(format!(
            "{flag} must be KEY=value, got {pair:?}"
        )))
    }
}

/// Render args for messages with the `--token` value redacted.
pub(crate) fn display_args(args: &[String]) -> String {
    let mut out = Vec::with_capacity(args.len());
    let mut redact_next = false;
    for arg in args {
        if redact_next {
            out.push("<redacted>".to_string());
            redact_next = false;
        } else if arg == "--token" {
            out.push(arg.clone());
            redact_next = true;
        } else if arg.starts_with("--token=") {
            out.push("--token=<redacted>".to_string());
        } else {
            out.push(arg.clone());
        }
    }
    out.join(" ")
}

/// Vercel prints the deployment URL to stdout; take the last non-empty line.
pub(crate) fn deployment_url(stdout: &str) -> Result<String> {
    stdout
        .lines()
        .map(str::trim)
        .rfind(|line| !line.is_empty())
        .map(str::to_string)
        .ok_or_else(|| {
            Error::CommandFailed("`vercel deploy` printed no deployment URL to stdout".to_string())
        })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn stamp(tag: &str) -> std::path::PathBuf {
        std::env::temp_dir().join(format!(
            "swag-vercel-test-{tag}-{}",
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .expect("time")
                .as_nanos()
        ))
    }

    fn write_provenance(dir: &Path, template: &str, kind: TemplateType) {
        std::fs::create_dir_all(dir).unwrap();
        let prov =
            crate::provenance::Provenance::new("demo", template, None, Some(21), Some(kind), None);
        crate::provenance::save(dir, &prov).unwrap();
    }

    #[test]
    fn missing_binary_reports_vercel_not_found() {
        let vercel = Vercel::with_program("swag-definitely-not-a-binary-xyz");
        let err = vercel.ensure_available().unwrap_err();
        assert!(matches!(err, Error::VercelNotFound(_)), "{err:?}");
    }

    #[test]
    fn detects_frontend_backend_and_refuses_desktop() {
        let frontend = stamp("frontend");
        write_provenance(&frontend, "svelte-clean-template", TemplateType::Frontend);
        assert_eq!(detect_kind(&frontend).unwrap(), DeployKind::Frontend);

        let backend = stamp("backend");
        write_provenance(&backend, "svelte-fullstack-template", TemplateType::Backend);
        assert_eq!(detect_kind(&backend).unwrap(), DeployKind::Backend);

        let desktop = stamp("desktop");
        write_provenance(&desktop, "svelte-tauri-template", TemplateType::Desktop);
        let err = detect_kind(&desktop).unwrap_err();
        assert!(err.to_string().contains("desktop"), "{err}");

        for dir in [&frontend, &backend, &desktop] {
            std::fs::remove_dir_all(dir).unwrap();
        }
    }

    #[test]
    fn falls_back_to_type_file_without_provenance() {
        let dir = stamp("typefile");
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join(".template_type"), "backend\n").unwrap();
        assert_eq!(detect_kind(&dir).unwrap(), DeployKind::Backend);
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn unknown_directory_is_not_a_project() {
        let err = detect_kind(&stamp("missing-xyz")).unwrap_err();
        assert!(matches!(err, Error::NotAProject(_)), "{err:?}");
    }

    #[test]
    fn frontend_preflight_needs_package_json() {
        let dir = stamp("front-pre");
        std::fs::create_dir_all(&dir).unwrap();
        let err = preflight(&dir, &DeployKind::Frontend).unwrap_err();
        assert!(err.to_string().contains("package.json"), "{err}");
        std::fs::write(dir.join("package.json"), "{}").unwrap();
        assert!(preflight(&dir, &DeployKind::Frontend).unwrap().is_empty());
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn backend_preflight_checks_vercel_json_and_layout() {
        let dir = stamp("back-pre");
        std::fs::create_dir_all(&dir).unwrap();
        let err = preflight(&dir, &DeployKind::Backend).unwrap_err();
        assert!(err.to_string().contains("vercel.json"), "{err}");

        std::fs::write(dir.join("vercel.json"), "not json").unwrap();
        let err = preflight(&dir, &DeployKind::Backend).unwrap_err();
        assert!(err.to_string().contains("not valid JSON"), "{err}");

        std::fs::write(dir.join("vercel.json"), r#"{"services": {"frontend": {}}}"#).unwrap();
        let err = preflight(&dir, &DeployKind::Backend).unwrap_err();
        assert!(err.to_string().contains("frontend + backend"), "{err}");

        std::fs::write(
            dir.join("vercel.json"),
            r#"{"services": {"frontend": {}, "backend": {}}}"#,
        )
        .unwrap();
        let err = preflight(&dir, &DeployKind::Backend).unwrap_err();
        assert!(err.to_string().contains("frontend/'"), "{err}");

        std::fs::create_dir_all(dir.join("frontend")).unwrap();
        std::fs::create_dir_all(dir.join("backend")).unwrap();
        let err = preflight(&dir, &DeployKind::Backend).unwrap_err();
        assert!(err.to_string().contains("Dockerfile.vercel"), "{err}");

        std::fs::write(
            dir.join("backend").join("Dockerfile.vercel"),
            "FROM scratch",
        )
        .unwrap();
        let notes = preflight(&dir, &DeployKind::Backend).unwrap();
        assert!(notes.iter().any(|n| n.contains("SITE_URL")), "{notes:?}");
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn deploy_args_shape_and_validation() {
        let opts = DeployOptions {
            prod: true,
            yes: true,
            project: Some("demo".to_string()),
            token: Some("tok".to_string()),
            scope: None,
            team: None,
            env: vec!["FOO=bar".to_string()],
            build_env: vec!["SITE_URL=https://x/".to_string()],
            no_wait: true,
            force: false,
            extra: vec!["--regions".to_string(), "sfo1".to_string()],
        };
        assert_eq!(
            deploy_args(&opts).unwrap(),
            vec![
                "deploy",
                "--prod",
                "--yes",
                "--project",
                "demo",
                "--token",
                "tok",
                "--env",
                "FOO=bar",
                "--build-env",
                "SITE_URL=https://x/",
                "--no-wait",
                "--regions",
                "sfo1",
            ]
        );

        let bad_project = DeployOptions {
            project: Some("--evil".to_string()),
            ..Default::default()
        };
        assert!(deploy_args(&bad_project).is_err());

        let bad_env = DeployOptions {
            env: vec!["NOEQUALS".to_string()],
            ..Default::default()
        };
        assert!(deploy_args(&bad_env).is_err());

        let bad_link = LinkOptions {
            scope: Some("-x".to_string()),
            ..Default::default()
        };
        assert!(link_args(&bad_link).is_err());

        assert_eq!(link_args(&LinkOptions::default()).unwrap(), vec!["link"]);
    }

    #[test]
    fn token_never_printed() {
        let args = deploy_args(&DeployOptions {
            token: Some("secret-123".to_string()),
            extra: vec!["--token=also-secret".to_string()],
            ..Default::default()
        })
        .unwrap();
        let shown = display_args(&args);
        assert!(!shown.contains("secret"), "{shown}");
        assert!(shown.contains("<redacted>"), "{shown}");
    }

    #[test]
    fn deployment_url_takes_last_line() {
        assert_eq!(
            deployment_url("building...\nhttps://demo-abc.vercel.app\n").unwrap(),
            "https://demo-abc.vercel.app"
        );
        assert!(deployment_url("  \n").is_err());
    }

    #[test]
    fn link_detection_reads_project_json() {
        let dir = stamp("linked");
        std::fs::create_dir_all(&dir).unwrap();
        assert!(!Vercel::is_linked(&dir));
        std::fs::create_dir_all(dir.join(".vercel")).unwrap();
        std::fs::write(dir.join(".vercel").join("project.json"), "{}").unwrap();
        assert!(Vercel::is_linked(&dir));
        std::fs::remove_dir_all(&dir).unwrap();
    }
}
