//! Template-aware command planning and execution.
//!
//! The CLI delegates `install`, `dev`, `build`, `check`, `test`, `run` and
//! `preview` here. A [`TaskPlan`] is either sequential (setup, checks, builds)
//! or parallel (development servers and production runtimes). Plans are pure
//! data so tests can assert the exact commands without spawning anything.

use std::path::{Path, PathBuf};
use std::process::{Command, ExitStatus, Stdio};
use std::thread::sleep;
use std::time::Duration;

use crate::bun::Bun;
use crate::cargo::Cargo;
use crate::error::{Error, Result};
use crate::layout::ProjectLayout;

/// Project action requested by the CLI.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Action {
    /// Install Bun and Cargo dependencies.
    Install,
    /// Development servers (hot reload).
    Dev,
    /// Production build.
    Build,
    /// Type checks plus Rust formatting/lints.
    Check,
    /// Unit tests.
    Test,
    /// Production runtime (requires a prior build).
    Run,
    /// Web-only production preview.
    Preview,
}

impl Action {
    fn as_str(self) -> &'static str {
        match self {
            Action::Install => "install",
            Action::Dev => "dev",
            Action::Build => "build",
            Action::Check => "check",
            Action::Test => "test",
            Action::Run => "run",
            Action::Preview => "preview",
        }
    }
}

/// How the steps of a [`TaskPlan`] run.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Mode {
    /// Run steps in order, stopping at the first failure.
    Sequential,
    /// Run all steps concurrently, stopping together when one fails.
    Parallel,
}

/// One subprocess invocation in a plan.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Step {
    /// Human-readable component label (e.g. `web: bun run check`).
    pub label: String,
    /// Binary to execute.
    pub program: String,
    /// Arguments passed verbatim.
    pub args: Vec<String>,
    /// Working directory.
    pub dir: PathBuf,
}

impl Step {
    fn new(label: impl Into<String>, program: &str, args: &[&str], dir: &Path) -> Self {
        Self {
            label: label.into(),
            program: program.to_string(),
            args: args.iter().map(|arg| (*arg).to_string()).collect(),
            dir: dir.to_path_buf(),
        }
    }

    /// Shell-like rendering used in errors and progress messages.
    #[must_use]
    pub fn display(&self) -> String {
        if self.args.is_empty() {
            self.program.clone()
        } else {
            format!("{} {}", self.program, self.args.join(" "))
        }
    }

    fn command(&self) -> Command {
        let mut command = Command::new(&self.program);
        command
            .args(&self.args)
            .current_dir(&self.dir)
            .stdin(Stdio::inherit())
            .stdout(Stdio::inherit())
            .stderr(Stdio::inherit());
        command
    }
}

/// A planned set of subprocesses.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TaskPlan {
    /// Execution mode.
    pub mode: Mode,
    /// Steps in display/start order.
    pub steps: Vec<Step>,
}

/// Plan a project action.
///
/// `web` / `rust` are the CLI selection flags; passing neither selects every
/// component the template provides. `extra` is forwarded only when the plan
/// resolves to a single command.
pub fn plan(
    layout: &ProjectLayout,
    action: Action,
    web: bool,
    rust: bool,
    extra: &[String],
) -> Result<TaskPlan> {
    validate_selection(layout, web, rust)?;
    let mut task = match action {
        Action::Install => install_plan(layout, web, rust)?,
        Action::Dev => dev_plan(layout, web, rust)?,
        Action::Build => build_plan(layout, web, rust)?,
        Action::Check => check_plan(layout, web, rust)?,
        Action::Test => test_plan(layout, web, rust)?,
        Action::Run => run_plan(layout, web, rust)?,
        Action::Preview => preview_plan(layout, web, rust)?,
    };
    if !extra.is_empty() {
        if task.steps.len() != 1 {
            return Err(Error::CommandFailed(format!(
                "extra arguments after `--` need a single component (pass --web or --rust to `swag {}`)",
                action.as_str()
            )));
        }
        task.steps[0].args.push("--".to_string());
        task.steps[0].args.extend(extra.iter().cloned());
    }
    Ok(task)
}

/// Execute a plan, resolving the required toolchains first.
pub fn execute(task: &TaskPlan) -> Result<()> {
    verify_tools(&task.steps)?;
    match task.mode {
        Mode::Sequential => execute_sequential(&task.steps),
        Mode::Parallel => execute_parallel(&task.steps),
    }
}

fn validate_selection(layout: &ProjectLayout, web: bool, rust: bool) -> Result<()> {
    if web && !layout.has_web() {
        return Err(Error::CommandFailed(format!(
            "no web component in '{}' (expected package.json; run `swag doctor`)",
            layout.root.display()
        )));
    }
    if rust && !layout.has_rust() {
        return Err(Error::CommandFailed(format!(
            "no Rust component in '{}' (expected backend/ or src-tauri/; run `swag doctor`)",
            layout.root.display()
        )));
    }
    Ok(())
}

fn install_plan(layout: &ProjectLayout, web: bool, rust: bool) -> Result<TaskPlan> {
    let (web, rust) = requested(layout, web, rust);
    let mut steps = Vec::new();
    if web {
        let dir = layout.require_web()?;
        steps.push(Step::new(
            format!("web: bun install ({})", dir.display()),
            "bun",
            &["install", "--ignore-scripts"],
            dir,
        ));
    }
    if rust {
        let dir = layout.require_rust()?;
        let mut args = vec!["fetch"];
        if dir.join("Cargo.lock").is_file() {
            args.push("--locked");
        }
        steps.push(Step::new(
            format!("rust: cargo fetch ({})", dir.display()),
            "cargo",
            &args,
            dir,
        ));
    }
    sequence(steps)
}

fn dev_plan(layout: &ProjectLayout, web: bool, rust: bool) -> Result<TaskPlan> {
    if layout.is_desktop() {
        if rust && !web {
            return Err(Error::CommandFailed(
                "desktop `swag dev --rust` is not supported: Tauri dev runs the web and Rust sides together (use the default, or --web for a browser-only server)".to_string(),
            ));
        }
        let root = layout.require_web()?;
        if web && !rust {
            return Ok(single(Step::new(
                format!("web: bun run dev ({})", root.display()),
                "bun",
                &["run", "dev"],
                root,
            )));
        }
        let script = desktop_script(root, "desktop:dev", "tauri dev");
        return Ok(single(Step::new(
            format!("desktop: bun run {script} ({})", root.display()),
            "bun",
            &["run", &script],
            root,
        )));
    }
    let (web, rust) = requested(layout, web, rust);
    let mut steps = Vec::new();
    if web {
        let dir = layout.require_web()?;
        steps.push(Step::new(
            format!("web: bun run dev ({})", dir.display()),
            "bun",
            &["run", "dev"],
            dir,
        ));
    }
    if rust {
        let dir = layout.require_rust()?;
        steps.push(Step::new(
            format!("rust: cargo run ({})", dir.display()),
            "cargo",
            &["run"],
            dir,
        ));
    }
    parallel(steps)
}

fn build_plan(layout: &ProjectLayout, web: bool, rust: bool) -> Result<TaskPlan> {
    if layout.is_desktop() && (web == rust) {
        let root = layout.require_web()?;
        let script = desktop_script(root, "desktop:build", "tauri build");
        return Ok(single(Step::new(
            format!("desktop: bun run {script} ({})", root.display()),
            "bun",
            &["run", &script],
            root,
        )));
    }
    let (web, rust) = requested(layout, web, rust);
    let mut steps = Vec::new();
    if web {
        let dir = layout.require_web()?;
        steps.push(Step::new(
            format!("web: bun run build ({})", dir.display()),
            "bun",
            &["run", "build"],
            dir,
        ));
    }
    if rust {
        let dir = layout.require_rust()?;
        steps.push(Step::new(
            format!("rust: cargo build --release ({})", dir.display()),
            "cargo",
            &["build", "--release"],
            dir,
        ));
    }
    sequence(steps)
}

fn check_plan(layout: &ProjectLayout, web: bool, rust: bool) -> Result<TaskPlan> {
    let (web, rust) = requested(layout, web, rust);
    let mut steps = Vec::new();
    if web {
        let dir = layout.require_web()?;
        steps.push(Step::new(
            format!("web: bun run check ({})", dir.display()),
            "bun",
            &["run", "check"],
            dir,
        ));
    }
    if rust {
        let dir = layout.require_rust()?;
        steps.push(Step::new(
            format!("rust: cargo fmt -- --check ({})", dir.display()),
            "cargo",
            &["fmt", "--", "--check"],
            dir,
        ));
        steps.push(Step::new(
            format!("rust: cargo clippy ({})", dir.display()),
            "cargo",
            &["clippy", "--all-targets", "--", "-D", "warnings"],
            dir,
        ));
    }
    sequence(steps)
}

fn test_plan(layout: &ProjectLayout, web: bool, rust: bool) -> Result<TaskPlan> {
    let (web, rust) = requested(layout, web, rust);
    let mut steps = Vec::new();
    if web {
        let dir = layout.require_web()?;
        steps.push(Step::new(
            format!("web: bun run test ({})", dir.display()),
            "bun",
            &["run", "test"],
            dir,
        ));
    }
    if rust {
        let dir = layout.require_rust()?;
        steps.push(Step::new(
            format!("rust: cargo test ({})", dir.display()),
            "cargo",
            &["test"],
            dir,
        ));
    }
    sequence(steps)
}

fn run_plan(layout: &ProjectLayout, web: bool, rust: bool) -> Result<TaskPlan> {
    if layout.is_desktop() {
        if !web {
            let dir = layout.require_rust()?;
            require_build(layout.root.join("dist"))?;
            return Ok(single(Step::new(
                format!("desktop: cargo run --release ({})", dir.display()),
                "cargo",
                &["run", "--release"],
                dir,
            )));
        }
        let mut steps = Vec::new();
        let web_dir = layout.require_web()?;
        require_build(web_dir.join("dist"))?;
        steps.push(Step::new(
            format!("web: bun run preview ({})", web_dir.display()),
            "bun",
            &["run", "preview"],
            web_dir,
        ));
        if rust {
            let dir = layout.require_rust()?;
            steps.push(Step::new(
                format!("desktop: cargo run --release ({})", dir.display()),
                "cargo",
                &["run", "--release"],
                dir,
            ));
        }
        return parallel(steps);
    }
    let (web, rust) = requested(layout, web, rust);
    let mut steps = Vec::new();
    if web {
        let dir = layout.require_web()?;
        require_build(dir.join("dist"))?;
        steps.push(Step::new(
            format!("web: bun run preview ({})", dir.display()),
            "bun",
            &["run", "preview"],
            dir,
        ));
    }
    if rust {
        let dir = layout.require_rust()?;
        steps.push(Step::new(
            format!("rust: cargo run --release ({})", dir.display()),
            "cargo",
            &["run", "--release"],
            dir,
        ));
    }
    parallel(steps)
}

fn preview_plan(layout: &ProjectLayout, web: bool, rust: bool) -> Result<TaskPlan> {
    if rust {
        return Err(Error::CommandFailed(
            "`swag preview` only supports the web component (use `swag run --rust` for the production backend)".to_string(),
        ));
    }
    let _ = web;
    let dir = layout.require_web()?;
    Ok(single(Step::new(
        format!("web: bun run preview ({})", dir.display()),
        "bun",
        &["run", "preview"],
        dir,
    )))
}

/// Resolve the CLI flags to the components a non-desktop plan should drive.
fn requested(layout: &ProjectLayout, web: bool, rust: bool) -> (bool, bool) {
    if web || rust {
        return (web, rust);
    }
    (layout.has_web(), layout.has_rust())
}

fn sequence(steps: Vec<Step>) -> Result<TaskPlan> {
    if steps.is_empty() {
        return Err(no_components());
    }
    Ok(TaskPlan {
        mode: Mode::Sequential,
        steps,
    })
}

fn parallel(steps: Vec<Step>) -> Result<TaskPlan> {
    if steps.is_empty() {
        return Err(no_components());
    }
    let mode = if steps.len() > 1 {
        Mode::Parallel
    } else {
        Mode::Sequential
    };
    Ok(TaskPlan { mode, steps })
}

fn single(step: Step) -> TaskPlan {
    TaskPlan {
        mode: Mode::Sequential,
        steps: vec![step],
    }
}

fn no_components() -> Error {
    Error::CommandFailed("project has no runnable components (run `swag doctor`)".to_string())
}

fn require_build(dist: PathBuf) -> Result<()> {
    if dist.is_dir() {
        Ok(())
    } else {
        Err(Error::CommandFailed(format!(
            "no production build in '{}' (run `swag build` first)",
            dist.display()
        )))
    }
}

fn desktop_script(root: &Path, preferred: &str, fallback: &str) -> String {
    let package = std::fs::read_to_string(root.join("package.json"))
        .ok()
        .and_then(|raw| serde_json::from_str::<serde_json::Value>(&raw).ok());
    let has_preferred = package
        .as_ref()
        .and_then(|value| value.get("scripts"))
        .and_then(|scripts| scripts.get(preferred))
        .is_some_and(serde_json::Value::is_string);
    if has_preferred {
        preferred.to_string()
    } else {
        fallback.to_string()
    }
}

fn verify_tools(steps: &[Step]) -> Result<()> {
    if steps.iter().any(|step| step.program == "bun") {
        Bun::new()?;
    }
    if steps.iter().any(|step| step.program == "cargo") {
        Cargo::new()?;
    }
    Ok(())
}

fn execute_sequential(steps: &[Step]) -> Result<()> {
    for step in steps {
        eprintln!("swag: running {} [{}]", step.display(), step.label);
        let status = step.command().status().map_err(|e| spawn_error(step, e))?;
        if !status.success() {
            return Err(step_failed(step, status));
        }
    }
    Ok(())
}

fn execute_parallel(steps: &[Step]) -> Result<()> {
    let mut children: Vec<(&Step, std::process::Child)> = Vec::new();
    for step in steps {
        eprintln!("swag: starting {} [{}]", step.display(), step.label);
        match step.command().spawn() {
            Ok(child) => children.push((step, child)),
            Err(error) => {
                for (_, child) in children.iter_mut() {
                    let _ = child.kill();
                    let _ = child.wait();
                }
                return Err(spawn_error(step, error));
            }
        }
    }

    let mut live = vec![true; children.len()];
    let mut running = children.len();
    let mut failure: Option<String> = None;

    while running > 0 {
        for (index, (step, child)) in children.iter_mut().enumerate() {
            if !live[index] {
                continue;
            }
            match child.try_wait() {
                Ok(Some(status)) => {
                    live[index] = false;
                    running -= 1;
                    if !status.success() && failure.is_none() {
                        failure = Some(format!("{} exited with {status}", step.label));
                    }
                }
                Ok(None) => {}
                Err(e) => {
                    live[index] = false;
                    running -= 1;
                    if failure.is_none() {
                        failure = Some(format!("{}: {e}", step.label));
                    }
                }
            }
        }
        if failure.is_some() {
            break;
        }
        if running > 0 {
            sleep(Duration::from_millis(100));
        }
    }

    if let Some(message) = failure {
        for (index, (_, child)) in children.iter_mut().enumerate() {
            if live[index] {
                let _ = child.kill();
                let _ = child.wait();
            }
        }
        return Err(Error::CommandFailed(format!(
            "{message}; stopped the other component"
        )));
    }
    Ok(())
}

fn spawn_error(step: &Step, error: std::io::Error) -> Error {
    Error::CommandFailed(format!(
        "failed to spawn `{}` in '{}': {error}",
        step.display(),
        step.dir.display()
    ))
}

fn step_failed(step: &Step, status: ExitStatus) -> Error {
    Error::CommandFailed(format!(
        "`{}` in '{}' exited with {status}",
        step.display(),
        step.dir.display()
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn stamp(tag: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!(
            "swag-tasks-test-{tag}-{}-{}",
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

    fn frontend(tag: &str) -> PathBuf {
        let dir = stamp(tag);
        write(&dir.join(".template_type"), "frontend\n");
        write(&dir.join("package.json"), r#"{"scripts":{"dev":"vite"}}"#);
        dir
    }

    fn fullstack(tag: &str) -> PathBuf {
        let dir = stamp(tag);
        write(&dir.join(".template_type"), "backend\n");
        write(
            &dir.join("frontend/package.json"),
            r#"{"scripts":{"dev":"vite","build":"vite build","check":"svelte-check","test":"bun test","preview":"vite preview"}}"#,
        );
        write(
            &dir.join("backend/Cargo.toml"),
            "[package]\nname = \"backend\"\n",
        );
        write(&dir.join("backend/Cargo.lock"), "");
        dir
    }

    fn desktop_fixture(tag: &str) -> PathBuf {
        let dir = stamp(tag);
        write(&dir.join(".template_type"), "desktop\n");
        write(
            &dir.join("package.json"),
            r#"{"scripts":{"dev":"vite","build":"vite build","check":"svelte-check","test":"bun test","preview":"vite preview","desktop:dev":"tauri dev","desktop:build":"tauri build"}}"#,
        );
        write(
            &dir.join("src-tauri/Cargo.toml"),
            "[package]\nname = \"desktop\"\n",
        );
        write(&dir.join("src-tauri/Cargo.lock"), "");
        dir
    }

    fn layout(dir: &Path) -> ProjectLayout {
        ProjectLayout::detect(dir).unwrap()
    }

    fn strip_dirs(steps: &[Step]) -> Vec<(String, Vec<String>)> {
        steps
            .iter()
            .map(|step| (step.program.clone(), step.args.clone()))
            .collect()
    }

    #[test]
    fn frontend_actions_use_root_bun_scripts() {
        let dir = frontend("frontend-actions");
        let layout = layout(&dir);
        let install = plan(&layout, Action::Install, false, false, &[]).unwrap();
        assert_eq!(
            strip_dirs(&install.steps),
            [(
                "bun".to_string(),
                vec!["install".to_string(), "--ignore-scripts".to_string()]
            )]
        );
        let dev = plan(&layout, Action::Dev, false, false, &[]).unwrap();
        assert_eq!(
            strip_dirs(&dev.steps),
            [(
                "bun".to_string(),
                vec!["run".to_string(), "dev".to_string()]
            )]
        );
        write(&dir.join("dist/index.html"), "built");
        let run = plan(&layout, Action::Run, false, false, &[]).unwrap();
        assert_eq!(
            strip_dirs(&run.steps),
            [(
                "bun".to_string(),
                vec!["run".to_string(), "preview".to_string()]
            )]
        );
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn fullstack_actions_split_web_and_rust() {
        let dir = fullstack("fullstack-actions");
        let layout = layout(&dir);
        let install = plan(&layout, Action::Install, false, false, &[]).unwrap();
        assert_eq!(
            strip_dirs(&install.steps),
            [
                (
                    "bun".to_string(),
                    vec!["install".to_string(), "--ignore-scripts".to_string()]
                ),
                (
                    "cargo".to_string(),
                    vec!["fetch".to_string(), "--locked".to_string()]
                ),
            ]
        );
        let check = plan(&layout, Action::Check, false, false, &[]).unwrap();
        let commands = strip_dirs(&check.steps);
        assert_eq!(commands[0].0, "bun");
        assert_eq!(commands[1].1, vec!["fmt", "--", "--check"]);
        assert_eq!(
            commands[2].1,
            vec!["clippy", "--all-targets", "--", "-D", "warnings"]
        );
        let dev = plan(&layout, Action::Dev, false, false, &[]).unwrap();
        assert_eq!(dev.mode, Mode::Parallel);
        assert_eq!(dev.steps.len(), 2);
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn desktop_uses_tauri_scripts_for_dev_and_build() {
        let dir = desktop_fixture("desktop-actions");
        let layout = layout(&dir);
        let dev = plan(&layout, Action::Dev, false, false, &[]).unwrap();
        assert_eq!(
            strip_dirs(&dev.steps),
            [(
                "bun".to_string(),
                vec!["run".to_string(), "desktop:dev".to_string()]
            )]
        );
        let web_dev = plan(&layout, Action::Dev, true, false, &[]).unwrap();
        assert_eq!(
            strip_dirs(&web_dev.steps),
            [(
                "bun".to_string(),
                vec!["run".to_string(), "dev".to_string()]
            )]
        );
        let build = plan(&layout, Action::Build, false, false, &[]).unwrap();
        assert_eq!(
            strip_dirs(&build.steps),
            [(
                "bun".to_string(),
                vec!["run".to_string(), "desktop:build".to_string()]
            )]
        );
        let rust_build = plan(&layout, Action::Build, false, true, &[]).unwrap();
        assert_eq!(
            strip_dirs(&rust_build.steps),
            [(
                "cargo".to_string(),
                vec!["build".to_string(), "--release".to_string()]
            )]
        );
        let check = plan(&layout, Action::Check, false, false, &[]).unwrap();
        assert_eq!(check.steps.len(), 3);
        assert!(check.steps[1].label.contains("cargo fmt"));
        write(&dir.join("dist/index.html"), "built");
        let run = plan(&layout, Action::Run, false, false, &[]).unwrap();
        assert_eq!(
            strip_dirs(&run.steps),
            [(
                "cargo".to_string(),
                vec!["run".to_string(), "--release".to_string()]
            )]
        );
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn selections_are_validated() {
        let dir = frontend("selection");
        let frontend_layout = layout(&dir);
        assert!(plan(&frontend_layout, Action::Check, false, true, &[]).is_err());
        let desktop_dir = desktop_fixture("selection-desktop");
        let desktop_layout = layout(&desktop_dir);
        assert!(plan(&desktop_layout, Action::Dev, false, true, &[]).is_err());
        std::fs::remove_dir_all(&dir).unwrap();
        std::fs::remove_dir_all(&desktop_dir).unwrap();
    }

    #[test]
    fn extra_args_require_a_single_step() {
        let dir = fullstack("extra");
        let layout = layout(&dir);
        let extra = vec!["--host".to_string()];
        assert!(plan(&layout, Action::Check, false, false, &extra).is_err());
        let dev = plan(&layout, Action::Dev, true, false, &extra).unwrap();
        assert_eq!(dev.steps[0].args, vec!["run", "dev", "--", "--host"]);
        std::fs::remove_dir_all(&dir).unwrap();
    }
}
