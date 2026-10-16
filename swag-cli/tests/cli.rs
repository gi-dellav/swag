//! CLI integration tests: no network, no bun required.
//!
//! Happy-path scaffolding (git clone + bun install) is covered by unit tests
//! in `swag-corelib` and manual e2e runs; these tests pin the argument
//! surface that must never silently regress.

use std::process::Command;

fn swag_with_home(home: &std::path::Path, args: &[&str]) -> (String, String, i32) {
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

fn swag(args: &[&str]) -> (String, String, i32) {
    // Isolate registries: the dev machine's ~/.swag must never leak into tests.
    let home = std::env::temp_dir().join(format!(
        "swag-cli-test-home-{}",
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .expect("time")
            .as_nanos()
    ));
    let res = swag_with_home(&home, args);
    let _ = std::fs::remove_dir_all(&home);
    res
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
    let nope = std::env::temp_dir().join("swag-cli-test-nope");
    let nope = nope.to_str().expect("utf8");
    let (_, err, code) = swag(&[
        "new",
        "--template",
        "svelte-native",
        "demo",
        "--output",
        nope,
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
    let nope = std::env::temp_dir().join("swag-cli-test-nope");
    let nope = nope.to_str().expect("utf8");
    let (_, err, code) = swag(&[
        "new",
        "--template",
        "clean",
        "--output",
        nope,
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
    let nope = std::env::temp_dir().join("swag-cli-test-nope");
    let nope = nope.to_str().expect("utf8");
    let (_, err, code) = swag(&[
        "new",
        "--template",
        "clean",
        "demo",
        "--output",
        nope,
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

#[test]
fn help_mentions_colorized_flag() {
    for args in [
        &["new", "--help"][..],
        &["init", "--help"][..],
        &["templates", "--help"][..],
        &["list", "--help"][..],
    ] {
        let (out, _, code) = swag(args);
        assert_eq!(code, 0, "{args:?}: {out}");
        assert!(out.contains("--colorized"), "{args:?}:\n{out}");
    }
}

#[test]
fn banner_heads_new_init_and_vc_stderr() {
    // The `⬢ SWAG {version}` banner prints first on stderr, even when the
    // command then fails (stderr is piped here, so it stays plain).
    let nope = std::env::temp_dir().join("swag-cli-test-nope");
    let nope = nope.to_str().expect("utf8");
    let (_, err, code) = swag(&[
        "new",
        "--template",
        "svelte-native",
        "demo",
        "--output",
        nope,
        "--no-install",
        "--no-git-init",
    ]);
    assert_ne!(code, 0);
    assert!(
        err.lines().next().unwrap_or("").starts_with("⬢ SWAG "),
        "{err}"
    );

    let missing = std::env::temp_dir().join("swag-cli-test-missing-init");
    let (_, err, code) = swag(&[
        "init",
        "--template",
        "clean",
        "demo",
        "--output",
        missing.to_str().expect("utf8"),
        "--no-install",
        "--no-git-init",
    ]);
    assert_ne!(code, 0);
    assert!(
        err.lines().next().unwrap_or("").starts_with("⬢ SWAG "),
        "{err}"
    );

    let (_, err, code) = swag(&["vc", "missing-xyz"]);
    assert_ne!(code, 0);
    assert!(
        err.lines().next().unwrap_or("").starts_with("⬢ SWAG "),
        "{err}"
    );
}

#[test]
fn colorized_flag_adds_ansi_to_banner_and_templates() {
    // Piped output stays plain by default; `--colorized` forces ANSI codes
    // on both the stderr banner (light blue) and the template catalogue.
    let nope = std::env::temp_dir().join("swag-cli-test-nope");
    let nope = nope.to_str().expect("utf8");
    let (_, err, code) = swag(&[
        "new",
        "--colorized",
        "--template",
        "svelte-native",
        "demo",
        "--output",
        nope,
        "--no-install",
        "--no-git-init",
    ]);
    assert_ne!(code, 0);
    assert!(
        err.lines()
            .next()
            .unwrap_or("")
            .starts_with("\u{1b}[94m⬢ SWAG "),
        "{err}"
    );

    let (out, _, code) = swag(&["templates"]);
    assert_eq!(code, 0, "{out}");
    assert!(!out.contains('\u{1b}'), "piped output must stay plain");

    for args in [
        &["templates", "--colorized"][..],
        &["list", "--colorized"][..],
    ] {
        let (out, _, code) = swag(args);
        assert_eq!(code, 0, "{args:?}: {out}");
        assert!(out.contains("\u{1b}["), "{args:?} must emit ANSI:\n{out}");
    }

    // `--json` output stays machine-readable (no banner on stdout, no color).
    let (out, err, code) = swag(&["templates", "--json"]);
    assert_eq!(code, 0, "{out} {err}");
    let parsed: serde_json::Value = serde_json::from_str(&out).expect("valid JSON");
    assert!(parsed.is_array(), "{out}");
    assert!(!out.contains('\u{1b}'), "{out}");
}
