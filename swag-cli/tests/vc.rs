//! `swag vc` integration tests: no network, no real `vercel` required.
//!
//! A stub `vercel` shell script on PATH stands in for the real CLI
//! (unix only); refusal paths (desktop, missing package.json, unknown
//! target) need no binary at all.

use std::path::PathBuf;
use std::process::Command;

fn unique_home(tag: &str) -> PathBuf {
    std::env::temp_dir().join(format!(
        "swag-vc-test-{tag}-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .expect("time")
            .as_nanos()
    ))
}

fn swag(home: &std::path::Path, args: &[&str]) -> (String, String, i32) {
    swag_with_env(home, args, &[])
}

fn swag_with_env(
    home: &std::path::Path,
    args: &[&str],
    extra_env: &[(&str, &str)],
) -> (String, String, i32) {
    let mut cmd = Command::new(env!("CARGO_BIN_EXE_swag"));
    cmd.env("SWAG_HOME", home).args(args);
    for (key, value) in extra_env {
        cmd.env(key, value);
    }
    let output = cmd.output().expect("could not spawn swag binary");
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

/// Minimal frontend fixture: type file (+ package.json unless omitted).
fn write_frontend(dir: &std::path::Path, with_package: bool) {
    std::fs::create_dir_all(dir).unwrap();
    std::fs::write(dir.join(".template_type"), "frontend\n").unwrap();
    if with_package {
        std::fs::write(dir.join("package.json"), "{}").unwrap();
    }
}

#[test]
fn vc_help_shows_flags() {
    let home = Home::fresh("help");
    let (out, _, code) = home.run(&["vc", "--help"]);
    assert_eq!(code, 0, "{out}");
    for flag in [
        "--prod",
        "--yes",
        "--project",
        "--token",
        "VERCEL_TOKEN",
        "--env",
        "--build-env",
        "--no-wait",
        "--force",
    ] {
        assert!(out.contains(flag), "missing {flag} in:\n{out}");
    }
}

#[test]
fn vc_rejects_unknown_target() {
    let home = Home::fresh("unknown");
    let (_, err, code) = home.run(&["vc", "missing-xyz"]);
    assert_ne!(code, 0);
    assert!(!err.is_empty(), "error must go to stderr");
}

#[test]
fn vc_refuses_desktop() {
    let home = Home::fresh("desktop");
    let dest = home.dir.join("tauri-app");
    std::fs::create_dir_all(&dest).unwrap();
    std::fs::write(dest.join(".template_type"), "desktop\n").unwrap();
    let (_, err, code) = home.run(&["vc", dest.to_str().expect("utf8")]);
    assert_ne!(code, 0);
    assert!(
        err.to_lowercase().contains("desktop"),
        "must mention desktop: {err}"
    );
}

#[test]
fn vc_frontend_needs_package_json() {
    let home = Home::fresh("nopkg");
    let dest = home.dir.join("demo");
    write_frontend(&dest, false);
    let (_, err, code) = home.run(&["vc", dest.to_str().expect("utf8")]);
    assert_ne!(code, 0);
    assert!(err.contains("package.json"), "{err}");
}

/// Stub `vercel` on PATH (unix only): logs link/deploy calls, fakes link by
/// writing `.vercel/project.json`, prints a deployment URL on deploy.
#[cfg(unix)]
fn install_stub_vercel(home: &Home) -> (PathBuf, String) {
    use std::os::unix::fs::PermissionsExt;
    let bin = home.dir.join("bin");
    std::fs::create_dir_all(&bin).unwrap();
    let script = bin.join("vercel");
    std::fs::write(
        &script,
        r#"#!/bin/sh
if [ "$1" = "--version" ]; then echo "Vercel CLI 99-test"; exit 0; fi
echo "$@" >> ./vercel-calls.log
if [ "$1" = "link" ]; then
  mkdir -p .vercel
  printf '{"projectId":"prj_test"}' > .vercel/project.json
  exit 0
fi
if [ "$1" = "deploy" ]; then echo "building..." >&2; echo "https://demo-test.vercel.app"; exit 0; fi
echo "unexpected: $*" >&2
exit 1
"#,
    )
    .unwrap();
    let mut perms = std::fs::metadata(&script).unwrap().permissions();
    perms.set_mode(0o755);
    std::fs::set_permissions(&script, perms).unwrap();
    let path = format!(
        "{}:{}",
        bin.to_str().expect("utf8"),
        std::env::var("PATH").unwrap_or_default()
    );
    (bin, path)
}

#[test]
#[cfg(unix)]
fn vc_links_then_deploys_with_stub() {
    let home = Home::fresh("stub");
    let (_bin, path) = install_stub_vercel(&home);
    let dest = home.dir.join("demo");
    write_frontend(&dest, true);
    let path_str = path.as_str();
    let home_dir = home.dir.clone();
    let (out, err, code) = swag_with_env(
        &home_dir,
        &["vc", dest.to_str().expect("utf8"), "--yes"],
        &[("PATH", path_str)],
    );
    assert_eq!(code, 0, "stdout: {out}\nstderr: {err}");
    assert!(out.contains("https://demo-test.vercel.app"), "{out}");
    assert!(
        dest.join(".vercel").join("project.json").is_file(),
        "link must write .vercel/project.json"
    );
    let log = std::fs::read_to_string(dest.join("vercel-calls.log")).unwrap();
    assert!(log.contains("link --yes"), "{log}");
    assert!(log.contains("deploy --yes"), "{log}");
}

#[test]
#[cfg(unix)]
fn vc_skips_link_when_already_linked() {
    let home = Home::fresh("linked");
    let (_bin, path) = install_stub_vercel(&home);
    let dest = home.dir.join("demo");
    write_frontend(&dest, true);
    std::fs::create_dir_all(dest.join(".vercel")).unwrap();
    std::fs::write(dest.join(".vercel").join("project.json"), "{}").unwrap();
    let home_dir = home.dir.clone();
    let (out, err, code) = swag_with_env(
        &home_dir,
        &["vc", dest.to_str().expect("utf8"), "--prod", "--yes"],
        &[("PATH", path.as_str())],
    );
    assert_eq!(code, 0, "stdout: {out}\nstderr: {err}");
    assert!(out.contains("https://demo-test.vercel.app"), "{out}");
    let log = std::fs::read_to_string(dest.join("vercel-calls.log")).unwrap();
    assert!(!log.contains("link"), "link must be skipped: {log}");
    assert!(log.contains("deploy --prod --yes"), "{log}");
}
