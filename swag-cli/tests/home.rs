//! Projects / templates / cd integration tests: no network, no bun required.
//!
//! Each test gets an isolated `SWAG_HOME` so registries never leak between
//! runs or into the developer's real `~/.swag`.

use std::path::PathBuf;
use std::process::Command;

fn unique_home(tag: &str) -> PathBuf {
    std::env::temp_dir().join(format!(
        "swag-home-test-{tag}-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .expect("time")
            .as_nanos()
    ))
}

fn swag(home: &std::path::Path, args: &[&str]) -> (String, String, i32) {
    let output = Command::new(env!("CARGO_BIN_EXE_swag"))
        .env("SWAG_HOME", home)
        .args(args)
        .output()
        .expect("could not spawn swag binary");
    (
        String::from_utf8_lossy(&output.stdout).into_owned(),
        String::from_utf8_lossy(&output.stderr).into_owned(),
        output.status.code().unwrap_or(-1),
    )
}

struct Home {
    dir: PathBuf,
}

impl Home {
    fn fresh(tag: &str) -> Self {
        Self {
            dir: unique_home(tag),
        }
    }

    fn run(&self, args: &[&str]) -> (String, String, i32) {
        swag(&self.dir, args)
    }
}

impl Drop for Home {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.dir);
    }
}

#[test]
fn projects_add_list_show_remove() {
    let home = Home::fresh("projects");
    let target = std::env::temp_dir();
    let target = target.to_str().expect("utf8 temp dir");

    let (out, _, code) = home.run(&["projects"]);
    assert_eq!(code, 0, "{out}");
    assert!(out.contains("no projects"), "{out}");

    let (out, _, code) = home.run(&["projects", "add", "demo", "--path", target]);
    assert_eq!(code, 0, "{out}");
    assert!(out.contains("demo"), "{out}");

    let (out, _, code) = home.run(&["projects", "list"]);
    assert_eq!(code, 0, "{out}");
    assert!(out.contains("demo"), "{out}");

    let (out, _, code) = home.run(&["projects", "show", "demo", "--json"]);
    assert_eq!(code, 0, "{out}");
    let parsed: serde_json::Value = serde_json::from_str(&out).expect("valid JSON");
    assert_eq!(parsed["name"], "demo");

    let (out, _, code) = home.run(&["projects", "--json"]);
    assert_eq!(code, 0, "{out}");
    let parsed: serde_json::Value = serde_json::from_str(&out).expect("valid JSON");
    assert_eq!(parsed.as_array().expect("array").len(), 1);

    let (_, err, code) = home.run(&["projects", "show", "missing"]);
    assert_ne!(code, 0);
    assert!(err.contains("missing"), "{err}");

    let (out, _, code) = home.run(&["projects", "remove", "demo"]);
    assert_eq!(code, 0, "{out}");
    let (out, _, code) = home.run(&["projects", "list"]);
    assert_eq!(code, 0, "{out}");
    assert!(!out.contains("demo"), "{out}");
}

#[test]
fn projects_prune_drops_missing_dirs() {
    let home = Home::fresh("prune");
    let gone = home.dir.join("gone");
    let (out, _, code) = home.run(&[
        "projects",
        "add",
        "gone",
        "--path",
        gone.to_str().expect("utf8"),
    ]);
    assert_eq!(code, 0, "{out}");

    let (out, _, code) = home.run(&["projects", "prune", "--dry-run"]);
    assert_eq!(code, 0, "{out}");
    assert!(out.contains("would prune gone"), "{out}");

    let (out, _, code) = home.run(&["projects", "prune"]);
    assert_eq!(code, 0, "{out}");
    assert!(out.contains("pruned gone"), "{out}");

    let (out, _, code) = home.run(&["projects", "list"]);
    assert_eq!(code, 0, "{out}");
    assert!(!out.contains("gone"), "{out}");
}

#[test]
fn cd_print_resolves_registry_path() {
    let home = Home::fresh("cd");
    let target = std::env::temp_dir();
    let canonical = target.canonicalize().unwrap();
    home.run(&[
        "projects",
        "add",
        "demo",
        "--path",
        target.to_str().expect("utf8"),
    ]);
    let (out, _, code) = home.run(&["cd", "demo", "--print"]);
    assert_eq!(code, 0, "{out}");
    assert_eq!(out.trim(), canonical.to_str().expect("utf8"));

    let (_, err, code) = home.run(&["cd", "missing", "--print"]);
    assert_ne!(code, 0);
    assert!(err.contains("missing"), "{err}");
}

#[test]
fn templates_add_list_show_remove() {
    let home = Home::fresh("templates");
    let src = std::env::temp_dir();
    let src = src.to_str().expect("utf8 temp dir");

    let (out, _, code) = home.run(&[
        "templates",
        "add",
        "blog-acme",
        "--base",
        "clean",
        "--path",
        src,
        "--description",
        "ACME fork",
    ]);
    assert_eq!(code, 0, "{out}");

    let (out, _, code) = home.run(&["templates"]);
    assert_eq!(code, 0, "{out}");
    assert!(out.contains("blog-acme"), "{out}");
    assert!(out.contains("svelte-clean-template"), "{out}");

    let (out, _, code) = home.run(&["templates", "show", "blog-acme"]);
    assert_eq!(code, 0, "{out}");
    assert!(out.contains("base: svelte-clean-template"), "{out}");

    let (out, _, code) = home.run(&["list", "--json"]);
    assert_eq!(code, 0, "{out}");
    let parsed: serde_json::Value = serde_json::from_str(&out).expect("valid JSON");
    let ids: Vec<&str> = parsed
        .as_array()
        .expect("array")
        .iter()
        .map(|t| t["id"].as_str().expect("id"))
        .collect();
    assert!(ids.contains(&"blog-acme"), "{ids:?}");

    // Built-ins cannot be removed; unknown names error helpfully.
    let (_, err, code) = home.run(&["templates", "remove", "clean"]);
    assert_ne!(code, 0);
    assert!(err.contains("built-in"), "{err}");

    let (out, _, code) = home.run(&["templates", "remove", "blog-acme"]);
    assert_eq!(code, 0, "{out}");
}

#[test]
fn templates_reject_bad_names_and_sources() {
    let home = Home::fresh("templates-bad");

    let (_, err, code) = home.run(&[
        "templates",
        "add",
        "clean",
        "--base",
        "clean",
        "--git",
        "https://github.com/acme/base.git",
    ]);
    assert_ne!(code, 0);
    assert!(err.contains("clean"), "{err}");

    let (_, err, code) = home.run(&[
        "templates",
        "add",
        "blog-acme",
        "--base",
        "clean",
        "--git",
        "ftp://not-a-git-url",
    ]);
    assert_ne!(code, 0);
    assert!(err.contains("git URL"), "{err}");

    let (_, err, code) = home.run(&[
        "templates",
        "add",
        "blog-acme",
        "--base",
        "clean",
        "--path",
        "/tmp/opencode/swag-definitely-missing-xyz",
    ]);
    assert_ne!(code, 0);
    assert!(err.contains("does not exist"), "{err}");

    let (_, err, code) = home.run(&["templates", "add", "blog-acme", "--base", "clean"]);
    assert_ne!(code, 0);
    assert!(err.contains("--git"), "{err}");
}

#[test]
fn new_from_local_template_auto_registers() {
    let home = Home::fresh("new-local");
    // Minimal local template tree containing the base id + .git (skipped).
    let src = home.dir.join("src-tpl");
    std::fs::create_dir_all(src.join("src")).unwrap();
    std::fs::create_dir_all(src.join(".git")).unwrap();
    std::fs::write(
        src.join("package.json"),
        r#"{"name": "svelte-clean-template"}"#,
    )
    .unwrap();
    let (out, _, code) = home.run(&[
        "templates",
        "add",
        "local-base",
        "--base",
        "clean",
        "--path",
        src.to_str().expect("utf8"),
    ]);
    assert_eq!(code, 0, "{out}");

    let dest = home.dir.join("out").join("demo");
    let (out, _, code) = home.run(&[
        "new",
        "--template",
        "local-base",
        "demo",
        "--output",
        dest.to_str().expect("utf8"),
        "--no-install",
        "--no-git-init",
    ]);
    assert_eq!(code, 0, "{out}");
    assert!(out.contains("registered"), "{out}");
    assert!(
        std::fs::read_to_string(dest.join("package.json"))
            .expect("scaffolded file")
            .contains("demo"),
        "personalization must rewrite base id"
    );

    let (out, _, code) = home.run(&["cd", "demo", "--print"]);
    assert_eq!(code, 0, "{out}");
    assert!(!out.trim().is_empty());
}
