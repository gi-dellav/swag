//! CLI integration tests: no network, no bun required.
//!
//! Happy-path scaffolding (git clone + bun install) is covered by unit tests
//! in `swag-corelib` and manual e2e runs; these tests pin the argument
//! surface that must never silently regress.

use std::process::Command;

fn swag(args: &[&str]) -> (String, String, i32) {
    let output = Command::new(env!("CARGO_BIN_EXE_swag"))
        .args(args)
        .output()
        .expect("could not spawn swag binary");
    (
        String::from_utf8_lossy(&output.stdout).into_owned(),
        String::from_utf8_lossy(&output.stderr).into_owned(),
        output.status.code().unwrap_or(-1),
    )
}

#[test]
fn list_shows_all_three_templates() {
    let (out, _, code) = swag(&["list"]);
    assert_eq!(code, 0, "{out}");
    for id in [
        "svelte-clean-template",
        "svelte-tauri-template",
        "svelte-fullstack-template",
    ] {
        assert!(out.contains(id), "{out}");
    }
}

#[test]
fn list_alias_and_json_work() {
    let (out, _, code) = swag(&["list-templates"]);
    assert_eq!(code, 0, "{out}");
    assert!(out.contains("svelte-clean-template"), "{out}");

    let (out, _, code) = swag(&["list", "--json"]);
    assert_eq!(code, 0, "{out}");
    let parsed: serde_json::Value = serde_json::from_str(&out).expect("valid JSON");
    let ids: Vec<&str> = parsed
        .as_array()
        .expect("JSON array")
        .iter()
        .map(|t| t["id"].as_str().expect("id field"))
        .collect();
    assert_eq!(
        ids,
        [
            "svelte-clean-template",
            "svelte-tauri-template",
            "svelte-fullstack-template"
        ]
    );
}

#[test]
fn unknown_template_errors_helpfully() {
    let (_, err, code) = swag(&[
        "new",
        "--template",
        "svelte-native",
        "demo",
        "--output",
        "/tmp/opencode/swag-cli-test-nope",
        "--no-install",
        "--no-git-init",
    ]);
    assert_ne!(code, 0);
    assert!(err.contains("svelte-native"), "{err}");
    assert!(err.contains("svelte-clean-template"), "{err}");
}

#[test]
fn invalid_name_errors_before_network() {
    // Empty / bad names must fail before any `git clone` is attempted.
    // (`--` separates the positional NAME from clap flag parsing so that
    // leading-dash names reach our own validator.)
    let (_, err, code) = swag(&[
        "new",
        "--template",
        "clean",
        "--output",
        "/tmp/opencode/swag-cli-test-nope",
        "--no-install",
        "--no-git-init",
        "--",
        "-bad",
    ]);
    assert_ne!(code, 0);
    assert!(err.contains("invalid project name"), "{err}");
}

#[test]
fn invalid_owner_errors_before_network() {
    let (_, err, code) = swag(&[
        "new",
        "--template",
        "clean",
        "demo",
        "--output",
        "/tmp/opencode/swag-cli-test-nope",
        "--owner",
        "not_an_owner!",
        "--no-install",
        "--no-git-init",
    ]);
    assert_ne!(code, 0);
    assert!(err.contains("invalid GitHub owner"), "{err}");
}

#[test]
fn help_mentions_owner_flag() {
    let (out, _, code) = swag(&["new", "--help"]);
    assert_eq!(code, 0, "{out}");
    assert!(out.contains("--owner"), "{out}");
    assert!(out.contains("SWAG_GITHUB_OWNER"), "{out}");
}
