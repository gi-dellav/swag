//! `update` / `doctor` integration tests: no network, no bun required.
//!
//! Local `--path` templates stand in for git clones; `new` always passes
//! `--no-install --no-git-init` so no toolchain is needed.

use std::path::PathBuf;
use std::process::Command;

fn unique_home(tag: &str) -> PathBuf {
    std::env::temp_dir().join(format!(
        "swag-upd-test-{tag}-{}-{}",
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

fn swag_in_dir(
    home: &std::path::Path,
    cwd: &std::path::Path,
    args: &[&str],
) -> (String, String, i32) {
    let output = Command::new(env!("CARGO_BIN_EXE_swag"))
        .env("SWAG_HOME", home)
        .current_dir(cwd)
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

/// Minimal template tree: personalized name + version/type files.
fn write_template(dir: &std::path::Path, version: &str, body: &str) {
    std::fs::create_dir_all(dir).unwrap();
    std::fs::write(
        dir.join("package.json"),
        r#"{"name": "svelte-clean-template"}"#,
    )
    .unwrap();
    std::fs::write(dir.join(".template_version"), version).unwrap();
    std::fs::write(dir.join(".template_type"), "frontend\n").unwrap();
    std::fs::write(dir.join("app.txt"), body).unwrap();
}

fn scaffold_from(home: &Home, src: &std::path::Path, dest: &std::path::Path) {
    let (out, _, code) = home.run(&[
        "templates",
        "add",
        "local-upd",
        "--base",
        "clean",
        "--path",
        src.to_str().expect("utf8"),
    ]);
    assert_eq!(code, 0, "{out}");
    let (out, _, code) = home.run(&[
        "new",
        "--template",
        "local-upd",
        "demo",
        "--output",
        dest.to_str().expect("utf8"),
        "--no-install",
        "--no-git-init",
    ]);
    assert_eq!(code, 0, "{out}");
    assert!(
        dest.join(".swag.json").exists(),
        "scaffold writes provenance"
    );
    assert!(
        dest.join(".template_version").exists(),
        "version file survives"
    );
}

#[test]
fn update_check_apply_cycle() {
    let home = Home::fresh("cycle");
    let src = home.dir.join("tpl");
    write_template(&src, "21\n", "v1");
    let dest = home.dir.join("out").join("demo");
    scaffold_from(&home, &src, &dest);
    let dest_str = dest.to_str().expect("utf8");

    let (out, _, code) = home.run(&["update", dest_str, "--check"]);
    assert_eq!(code, 0, "{out}");
    assert!(out.contains("up to date"), "{out}");

    std::fs::write(src.join("app.txt"), "v2").unwrap();
    std::fs::write(src.join(".template_version"), "22\n").unwrap();

    let (out, _, code) = home.run(&["update", dest_str, "--check"]);
    assert_eq!(code, 1, "check exits 1 when behind: {out}");

    let (out, _, code) = home.run(&["update", dest_str, "--dry-run"]);
    assert_eq!(code, 0, "{out}");
    assert!(out.contains("app.txt"), "{out}");
    assert_eq!(
        std::fs::read_to_string(dest.join("app.txt")).unwrap(),
        "v1",
        "dry-run must not write"
    );

    let (out, _, code) = home.run(&["update", dest_str]);
    assert_eq!(code, 0, "{out}");
    assert_eq!(std::fs::read_to_string(dest.join("app.txt")).unwrap(), "v2");
    assert_eq!(
        std::fs::read_to_string(dest.join(".template_version")).unwrap(),
        "22\n"
    );

    let (out, _, code) = home.run(&["update", dest_str, "--check"]);
    assert_eq!(code, 0, "{out}");
    assert!(out.contains("up to date"), "{out}");
}

#[test]
fn update_json_report_shape() {
    let home = Home::fresh("json");
    let src = home.dir.join("tpl");
    write_template(&src, "21\n", "v1");
    let dest = home.dir.join("demo");
    scaffold_from(&home, &src, &dest);

    let (out, _, code) = home.run(&["update", dest.to_str().expect("utf8"), "--check", "--json"]);
    assert_eq!(code, 0, "{out}");
    let parsed: serde_json::Value = serde_json::from_str(&out).expect("valid JSON");
    assert_eq!(parsed["up_to_date"], true);
    assert_eq!(parsed["local_version"], 21);
    assert_eq!(parsed["upstream_version"], 21);
    assert_eq!(parsed["applied"], false);
}

#[test]
fn update_resolves_registry_name_and_rejects_unknown() {
    let home = Home::fresh("registry");
    let src = home.dir.join("tpl");
    write_template(&src, "21\n", "v1");
    let dest = home.dir.join("demo");
    scaffold_from(&home, &src, &dest);

    let (out, _, code) = home.run(&["update", "demo", "--check"]);
    assert_eq!(code, 0, "{out}");

    let (_, err, code) = home.run(&["update", "missing", "--check"]);
    assert_ne!(code, 0);
    assert!(!err.is_empty(), "error must go to stderr");
}

#[test]
fn doctor_env_and_project_reports() {
    let home = Home::fresh("doctor");
    let (out, _, _) = home.run(&["doctor", "--json"]);
    let parsed: serde_json::Value = serde_json::from_str(&out).expect("valid JSON");
    let checks = parsed["checks"].as_array().expect("checks array");
    for name in ["bun", "git", "cargo", "home"] {
        assert!(checks.iter().any(|c| c["name"] == name), "{out}");
    }

    let src = home.dir.join("tpl");
    write_template(&src, "bogus\n", "v1");
    std::fs::write(src.join(".template_type"), "mobile\n").unwrap();
    let dest = home.dir.join("demo");
    std::fs::create_dir_all(&dest).unwrap();
    for entry in std::fs::read_dir(&src).unwrap() {
        let entry = entry.unwrap();
        std::fs::copy(entry.path(), dest.join(entry.file_name())).unwrap();
    }

    let (_, _, code) = home.run(&["doctor", dest.to_str().expect("utf8")]);
    assert_ne!(code, 0, "bad version/type must fail doctor");

    let (out, _, _) = home.run(&["doctor", dest.to_str().expect("utf8"), "--json"]);
    let parsed: serde_json::Value = serde_json::from_str(&out).expect("valid JSON");
    let checks = parsed["checks"].as_array().expect("checks array");
    assert!(
        checks
            .iter()
            .any(|c| c["name"] == "project-version" && c["status"] == "fail")
    );
    assert!(
        checks
            .iter()
            .any(|c| c["name"] == "project-type" && c["status"] == "fail")
    );
}

#[test]
fn doctor_fix_prunes_stale_entries() {
    let home = Home::fresh("fix");
    let gone = home.dir.join("gone");
    let (out, _, code) = home.run(&[
        "projects",
        "add",
        "gone",
        "--path",
        gone.to_str().expect("utf8"),
    ]);
    assert_eq!(code, 0, "{out}");

    let (out, _, code) = home.run(&["doctor", "--fix"]);
    assert_eq!(code, 0, "{out}");
    assert!(out.contains("pruned gone") || out.contains("fix:"), "{out}");

    let (out, _, code) = home.run(&["projects", "list"]);
    assert_eq!(code, 0, "{out}");
    assert!(!out.contains("gone"), "{out}");
}

#[test]
fn update_prune_preserves_user_files() {
    let home = Home::fresh("prune-user");
    let src = home.dir.join("tpl");
    write_template(&src, "21\n", "v1");
    std::fs::write(src.join("old.txt"), "old").unwrap();
    let dest = home.dir.join("demo");
    scaffold_from(&home, &src, &dest);

    std::fs::write(dest.join("notes.txt"), "user data").unwrap();
    std::fs::remove_file(src.join("old.txt")).unwrap();
    std::fs::write(src.join("new.txt"), "new").unwrap();

    let (out, err, code) = home.run(&["update", dest.to_str().expect("utf8"), "--prune"]);
    assert_eq!(code, 0, "{out} {err}");
    assert!(dest.join("notes.txt").is_file(), "user file must survive");
    assert!(!dest.join("old.txt").exists(), "template file is pruned");
    assert!(dest.join("new.txt").is_file(), "new template file is added");
    assert!(out.contains("D old.txt (deleted)"), "{out}");
    assert!(!out.contains("old.txt (kept"), "{out}");
    assert!(out.contains("updated (now"), "{out}");

    let (out, _, code) = home.run(&["update", dest.to_str().expect("utf8"), "--check"]);
    assert_eq!(code, 0, "{out}");
    assert!(out.contains("up to date"), "{out}");
}

#[test]
fn update_restores_missing_version_files() {
    let home = Home::fresh("version-sync");
    let src = home.dir.join("tpl");
    write_template(&src, "21\n", "v1");
    let dest = home.dir.join("demo");
    scaffold_from(&home, &src, &dest);

    std::fs::remove_file(dest.join(".template_version")).unwrap();
    std::fs::remove_file(dest.join(".template_type")).unwrap();

    let (out, _, code) = home.run(&["update", dest.to_str().expect("utf8"), "--check"]);
    assert_eq!(code, 1, "missing version files must count as behind: {out}");

    let (out, _, code) = home.run(&["update", dest.to_str().expect("utf8")]);
    assert_eq!(code, 0, "{out}");
    assert_eq!(
        std::fs::read_to_string(dest.join(".template_version")).unwrap(),
        "21\n"
    );
    assert_eq!(
        std::fs::read_to_string(dest.join(".template_type")).unwrap(),
        "frontend\n"
    );

    let (out, _, code) = home.run(&["update", dest.to_str().expect("utf8"), "--check"]);
    assert_eq!(code, 0, "{out}");
    assert!(out.contains("up to date"), "{out}");
}

#[test]
fn update_detects_large_file_changes() {
    let home = Home::fresh("large-file");
    let src = home.dir.join("tpl");
    let v1 = "a".repeat(600 * 1024);
    write_template(&src, "21\n", &v1);
    let dest = home.dir.join("demo");
    scaffold_from(&home, &src, &dest);

    let v2 = format!("{}b", "a".repeat(600 * 1024));
    std::fs::write(src.join("app.txt"), &v2).unwrap();

    let (out, _, code) = home.run(&["update", dest.to_str().expect("utf8"), "--check"]);
    assert_eq!(code, 1, "large file change must be detected: {out}");

    let (out, _, code) = home.run(&["update", dest.to_str().expect("utf8")]);
    assert_eq!(code, 0, "{out}");
    assert_eq!(std::fs::read_to_string(dest.join("app.txt")).unwrap(), v2);
}

#[test]
fn doctor_json_with_fix_stays_valid_json() {
    let home = Home::fresh("doctor-json-fix");
    let gone = home.dir.join("gone");
    let (out, _, code) = home.run(&[
        "projects",
        "add",
        "gone",
        "--path",
        gone.to_str().expect("utf8"),
    ]);
    assert_eq!(code, 0, "{out}");

    let (out, err, code) = home.run(&["doctor", "--json", "--fix"]);
    assert_eq!(code, 0, "{out} {err}");
    let parsed: serde_json::Value = serde_json::from_str(&out).expect("stdout must stay JSON");
    assert!(parsed["checks"].is_array(), "{out}");
    assert!(err.contains("fix: pruned gone"), "{err}");

    let (out, _, code) = home.run(&["projects", "list"]);
    assert_eq!(code, 0, "{out}");
    assert!(!out.contains("gone"), "{out}");
}

#[test]
fn registry_name_wins_over_local_path() {
    let home = Home::fresh("registry-wins");
    let registered = home.dir.join("registered");
    std::fs::create_dir_all(&registered).unwrap();
    std::fs::write(registered.join("package.json"), "{}").unwrap();
    std::fs::write(registered.join(".template_version"), "21\n").unwrap();
    std::fs::write(registered.join(".template_type"), "frontend\n").unwrap();
    let (out, _, code) = home.run(&[
        "projects",
        "add",
        "demo",
        "--path",
        registered.to_str().expect("utf8"),
    ]);
    assert_eq!(code, 0, "{out}");

    let cwd = home.dir.join("cwd");
    std::fs::create_dir_all(cwd.join("demo")).unwrap();

    let (out, _, _) = swag_in_dir(&home.dir, &cwd, &["doctor", "demo", "--json"]);
    let parsed: serde_json::Value = serde_json::from_str(&out).expect("valid JSON");
    let project_dir = parsed["checks"]
        .as_array()
        .expect("checks")
        .iter()
        .find(|c| c["name"] == "project-dir")
        .expect("project-dir check");
    let message = project_dir["message"].as_str().expect("message");
    assert!(
        message.contains(registered.canonicalize().unwrap().to_str().unwrap()),
        "registry path must win over ./demo: {message}"
    );
}
