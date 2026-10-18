//! Template-aware command routing tests: no network, no real Bun/Cargo.
//!
//! Stub `bun` / `cargo` executables on PATH record the argv and working
//! directory, so these tests pin the fullstack/desktop command plans.

use std::path::PathBuf;
use std::process::Command;

fn unique_home(tag: &str) -> PathBuf {
    std::env::temp_dir().join(format!(
        "swag-cmd-test-{tag}-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .expect("time")
            .as_nanos()
    ))
}

struct Sandbox {
    root: PathBuf,
    home: PathBuf,
    log: PathBuf,
}

impl Sandbox {
    fn new(tag: &str) -> Self {
        let root = unique_home(tag);
        let home = root.join("home");
        let log = root.join("calls.log");
        std::fs::create_dir_all(&home).unwrap();
        std::fs::write(&log, "").unwrap();
        Self { root, home, log }
    }

    fn tool_dir(&self) -> PathBuf {
        let bin = self.root.join("bin");
        std::fs::create_dir_all(&bin).unwrap();
        for tool in ["bun", "cargo"] {
            let path = bin.join(tool);
            let version = if tool == "bun" {
                "1.3.0"
            } else {
                "cargo 1.80.0"
            };
            std::fs::write(
                &path,
                format!(
                    "#!/bin/sh\n\
                     if [ \"$1\" = \"--version\" ]; then echo \"{version}\"; exit 0; fi\n\
                     printf '%s|%s|%s\\n' \"{tool}\" \"$PWD\" \"$*\" >> \"$SWAG_TEST_LOG\"\n\
                     exit 0\n"
                ),
            )
            .unwrap();
            #[cfg(unix)]
            {
                use std::os::unix::fs::PermissionsExt;
                std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o755)).unwrap();
            }
        }
        bin
    }

    #[cfg(unix)]
    fn write(&self, relative: &str, content: &str) -> PathBuf {
        let path = self.root.join(relative);
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent).unwrap();
        }
        std::fs::write(&path, content).unwrap();
        path
    }

    #[cfg(unix)]
    fn fullstack(&self) -> PathBuf {
        self.write(".template_type", "backend\n");
        self.write(
            "frontend/package.json",
            r#"{"scripts":{"dev":"vite","build":"vite build","check":"svelte-check","test":"bun test","preview":"vite preview"}}"#,
        );
        self.write("backend/Cargo.toml", "[package]\nname = \"backend\"\n");
        self.write("backend/Cargo.lock", "");
        self.root.clone()
    }

    #[cfg(unix)]
    fn desktop(&self) -> PathBuf {
        self.write(".template_type", "desktop\n");
        self.write(
            "package.json",
            r#"{"scripts":{"dev":"vite","build":"vite build","check":"svelte-check","test":"bun test","preview":"vite preview","desktop:dev":"tauri dev","desktop:build":"tauri build"}}"#,
        );
        self.write("src-tauri/Cargo.toml", "[package]\nname = \"desktop\"\n");
        self.write("src-tauri/Cargo.lock", "");
        self.root.clone()
    }

    fn run(&self, args: &[&str]) -> (String, String, i32) {
        let path = self.tool_dir();
        let output = Command::new(env!("CARGO_BIN_EXE_swag"))
            .env("SWAG_HOME", &self.home)
            .env("SWAG_TEST_LOG", &self.log)
            .env("PATH", &path)
            .args(args)
            .output()
            .expect("could not spawn swag binary");
        (
            String::from_utf8_lossy(&output.stdout).into_owned(),
            String::from_utf8_lossy(&output.stderr).into_owned(),
            output.status.code().unwrap_or(-1),
        )
    }

    #[cfg(unix)]
    fn calls(&self) -> Vec<String> {
        std::fs::read_to_string(&self.log)
            .unwrap_or_default()
            .lines()
            .map(str::to_string)
            .collect()
    }
}

impl Drop for Sandbox {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.root);
    }
}

#[cfg(unix)]
#[test]
fn fullstack_install_runs_bun_and_cargo_fetch() {
    let sandbox = Sandbox::new("fullstack-install");
    let project = sandbox.fullstack();
    let (out, err, code) = sandbox.run(&["install", project.to_str().expect("utf8")]);
    assert_eq!(code, 0, "{out}\n{err}");
    let calls = sandbox.calls();
    assert_eq!(calls.len(), 2, "{calls:?}");
    assert!(calls[0].contains("bun|"), "{calls:?}");
    assert!(calls[0].ends_with("install --ignore-scripts"), "{calls:?}");
    assert!(calls[0].contains("/frontend|"), "{calls:?}");
    assert!(calls[1].contains("cargo|"), "{calls:?}");
    assert!(calls[1].ends_with("fetch --locked"), "{calls:?}");
    assert!(calls[1].contains("/backend|"), "{calls:?}");
}

#[cfg(unix)]
#[test]
fn fullstack_dev_starts_both_components() {
    let sandbox = Sandbox::new("fullstack-dev");
    let project = sandbox.fullstack();
    let (out, err, code) = sandbox.run(&["dev", project.to_str().expect("utf8")]);
    assert_eq!(code, 0, "{out}\n{err}");
    let calls = sandbox.calls();
    assert_eq!(calls.len(), 2, "{calls:?}");
    assert!(
        calls
            .iter()
            .any(|call| call.contains("/frontend|") && call.ends_with("run dev")),
        "{calls:?}"
    );
    assert!(
        calls
            .iter()
            .any(|call| call.contains("/backend|") && call.ends_with("|run")),
        "{calls:?}"
    );
}

#[cfg(unix)]
#[test]
fn fullstack_check_runs_only_selected_component() {
    let sandbox = Sandbox::new("fullstack-rust-check");
    let project = sandbox.fullstack();
    let (out, err, code) = sandbox.run(&["check", project.to_str().expect("utf8"), "--rust"]);
    assert_eq!(code, 0, "{out}\n{err}");
    let calls = sandbox.calls();
    assert_eq!(calls.len(), 2, "{calls:?}");
    assert!(
        calls.iter().all(|call| call.starts_with("cargo|")),
        "{calls:?}"
    );
    assert!(calls[0].ends_with("fmt -- --check"), "{calls:?}");
    assert!(
        calls[1].ends_with("clippy --all-targets -- -D warnings"),
        "{calls:?}"
    );
}

#[cfg(unix)]
#[test]
fn desktop_build_prefers_tauri_script() {
    let sandbox = Sandbox::new("desktop-build");
    let project = sandbox.desktop();
    let (out, err, code) = sandbox.run(&["build", project.to_str().expect("utf8")]);
    assert_eq!(code, 0, "{out}\n{err}");
    let calls = sandbox.calls();
    assert_eq!(calls.len(), 1, "{calls:?}");
    assert!(calls[0].contains("bun|"), "{calls:?}");
    assert!(calls[0].ends_with("run desktop:build"), "{calls:?}");
}

#[cfg(unix)]
#[test]
fn desktop_check_runs_web_and_rust() {
    let sandbox = Sandbox::new("desktop-check");
    let project = sandbox.desktop();
    let (out, err, code) = sandbox.run(&["check", project.to_str().expect("utf8")]);
    assert_eq!(code, 0, "{out}\n{err}");
    let calls = sandbox.calls();
    assert_eq!(calls.len(), 3, "{calls:?}");
    assert!(
        calls[0].contains("bun|") && calls[0].ends_with("run check"),
        "{calls:?}"
    );
    assert!(
        calls[1].contains("cargo|") && calls[1].ends_with("fmt -- --check"),
        "{calls:?}"
    );
    assert!(
        calls[2].contains("cargo|") && calls[2].ends_with("clippy --all-targets -- -D warnings"),
        "{calls:?}"
    );
}

#[cfg(unix)]
#[test]
fn desktop_run_requires_a_build() {
    let sandbox = Sandbox::new("desktop-run");
    let project = sandbox.desktop();
    let (_, err, code) = sandbox.run(&["run", project.to_str().expect("utf8")]);
    assert_ne!(code, 0);
    assert!(err.contains("run `swag build` first"), "{err}");
    assert!(
        sandbox.calls().is_empty(),
        "nothing should run before preflight"
    );
}

#[cfg(unix)]
#[test]
fn desktop_dev_rust_only_is_refused() {
    let sandbox = Sandbox::new("desktop-dev-rust");
    let project = sandbox.desktop();
    let (_, err, code) = sandbox.run(&["dev", project.to_str().expect("utf8"), "--rust"]);
    assert_ne!(code, 0);
    assert!(err.contains("--rust"), "{err}");
    assert!(err.contains("desktop"), "{err}");
}

#[cfg(unix)]
#[test]
fn scaffolded_fullstack_installs_both_components() {
    let sandbox = Sandbox::new("scaffold-fullstack");
    sandbox.write("template/.template_type", "backend\n");
    sandbox.write(
        "template/frontend/package.json",
        r#"{"name":"svelte-fullstack-template","scripts":{}}"#,
    );
    sandbox.write(
        "template/backend/Cargo.toml",
        "[package]\nname = \"backend\"\n",
    );
    sandbox.write("template/backend/Cargo.lock", "");
    let template = sandbox.root.join("template");
    let dest = sandbox.root.join("app");

    let (out, err, code) = sandbox.run(&[
        "templates",
        "add",
        "my-stack",
        "--base",
        "fullstack",
        "--path",
        template.to_str().expect("utf8"),
    ]);
    assert_eq!(code, 0, "{out}\n{err}");

    let (out, err, code) = sandbox.run(&[
        "new",
        "--template",
        "my-stack",
        "demo",
        "--output",
        dest.to_str().expect("utf8"),
        "--no-git-init",
    ]);
    assert_eq!(code, 0, "{out}\n{err}");
    let calls = sandbox.calls();
    assert_eq!(calls.len(), 2, "{calls:?}");
    assert!(calls[0].contains("/app/frontend|"), "{calls:?}");
    assert!(calls[1].contains("/app/backend|"), "{calls:?}");
}

#[test]
fn command_help_mentions_selectors_and_run() {
    let sandbox = Sandbox::new("help");
    for command in ["install", "dev", "build", "check", "test", "run"] {
        let (out, _, code) = sandbox.run(&[command, "--help"]);
        assert_eq!(code, 0, "{command}: {out}");
        assert!(out.contains("--web"), "{command} missing --web:\n{out}");
        assert!(out.contains("--rust"), "{command} missing --rust:\n{out}");
    }
}
