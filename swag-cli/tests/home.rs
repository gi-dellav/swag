//! Projects / templates / cd integration tests: no network, no bun required.
//!
//! Each test gets an isolated `SWAG_HOME` so registries never leak between
//! runs or into the developer's real `~/.swag`.

use std::path::PathBuf;
use std::process::{Command, Stdio};

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

/// Like [`swag`] but with `stdin` piped in, for the interactive prompts'
/// non-terminal fallback (empty lines accept defaults, EOF fails).
fn swag_with_stdin(home: &std::path::Path, args: &[&str], stdin: &str) -> (String, String, i32) {
    use std::io::Write;
    let mut child = Command::new(env!("CARGO_BIN_EXE_swag"))
        .env("SWAG_HOME", home)
        .args(args)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("could not spawn swag binary");
    child
        .stdin
        .as_mut()
        .expect("piped stdin")
        .write_all(stdin.as_bytes())
        .expect("could not write stdin");
    let output = child
        .wait_with_output()
        .expect("could not wait for swag binary");
    (
        String::from_utf8_lossy(&output.stdout).into_owned(),
        String::from_utf8_lossy(&output.stderr).into_owned(),
        output.status.code().unwrap_or(-1),
    )
}

/// Like [`swag`] but runs the binary with `cwd` as working directory.
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

    fn run_with_stdin(&self, args: &[&str], stdin: &str) -> (String, String, i32) {
        swag_with_stdin(&self.dir, args, stdin)
    }

    /// Register a local template standing in for a git clone.
    fn register_template(&self, name: &str, src: &std::path::Path) -> String {
        let (out, _, code) = self.run(&[
            "templates",
            "add",
            name,
            "--base",
            "clean",
            "--path",
            src.to_str().expect("utf8"),
        ]);
        assert_eq!(code, 0, "{out}");
        out
    }

    /// Local template tree containing the base id, ready to personalize.
    fn write_template(&self, tag: &str) -> PathBuf {
        let src = self.dir.join(tag);
        std::fs::create_dir_all(&src).unwrap();
        std::fs::write(
            src.join("package.json"),
            r#"{"name": "svelte-clean-template"}"#,
        )
        .unwrap();
        src
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
    let missing = std::env::temp_dir().join("swag-definitely-missing-xyz");

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
        missing.to_str().expect("utf8"),
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

/// `swag new` with no NAME and no `--template` asks for both on stdin.
#[test]
fn new_prompts_name_and_template() {
    let home = Home::fresh("new-prompts");
    let src = home.write_template("tpl-prompts");
    home.register_template("local-prompts", &src);

    let dest = home.dir.join("out").join("demo");
    let (out, err, code) = home.run_with_stdin(
        &[
            "new",
            "--output",
            dest.to_str().expect("utf8"),
            "--no-install",
            "--no-git-init",
        ],
        "demo\nlocal-prompts\n",
    );
    assert_eq!(code, 0, "{out} {err}");
    assert!(err.contains("Project name"), "{err}");
    assert!(err.contains("Template"), "{err}");
    assert!(out.contains("created demo from local-prompts"), "{out}");
    assert!(
        std::fs::read_to_string(dest.join("package.json"))
            .expect("scaffolded file")
            .contains("demo"),
        "the piped name must personalize the project"
    );
}

/// A blank template line in the prompt falls back to the default template.
///
/// The default (`svelte-clean-template`) would need a network clone, so this
/// points `--output` at a non-empty directory: the destination guard fires
/// before the fetch, and its specific error proves the blank line resolved
/// to a valid template rather than failing as unknown.
#[test]
fn new_template_prompt_defaults_on_blank_line() {
    let home = Home::fresh("new-default-tpl");
    let dest = home.dir.join("busy");
    std::fs::create_dir_all(&dest).unwrap();
    std::fs::write(dest.join("stale.txt"), "stale").unwrap();
    let (_, err, code) = home.run_with_stdin(
        &[
            "new",
            "demo",
            "--output",
            dest.to_str().expect("utf8"),
            "--no-install",
            "--no-git-init",
        ],
        "\n",
    );
    assert_ne!(code, 0);
    assert!(err.contains("Template [svelte-clean-template]"), "{err}");
    assert!(err.contains("already exists"), "{err}");
}

/// EOF on stdin without a NAME is an error, not an empty project name.
#[test]
fn new_without_name_on_eof_errors() {
    let home = Home::fresh("new-eof");
    let dest = home.dir.join("out").join("demo");
    let (_, err, code) = home.run_with_stdin(
        &[
            "new",
            "--template",
            "clean",
            "--output",
            dest.to_str().expect("utf8"),
            "--no-install",
            "--no-git-init",
        ],
        "",
    );
    assert_ne!(code, 0, "{err}");
    assert!(err.contains("no project name"), "{err}");
    assert!(!dest.exists(), "nothing must be scaffolded without a name");
}

/// `swag init` scaffolds into a pre-existing directory; a blank name line
/// defaults to the directory basename.
#[test]
fn init_scaffolds_into_preexisting_dir() {
    let home = Home::fresh("init-existing");
    let src = home.write_template("tpl-init");
    home.register_template("local-init", &src);

    let dest = home.dir.join("my-app");
    std::fs::create_dir_all(&dest).unwrap();
    let (out, err, code) = home.run_with_stdin(
        &[
            "init",
            "--output",
            dest.to_str().expect("utf8"),
            "--no-install",
            "--no-git-init",
        ],
        "\nlocal-init\n",
    );
    assert_eq!(code, 0, "{out} {err}");
    assert!(err.contains("Project name [my-app]"), "{err}");
    assert!(out.contains("initialized my-app from local-init"), "{out}");
    assert!(
        std::fs::read_to_string(dest.join("package.json"))
            .expect("scaffolded file")
            .contains("my-app"),
        "the directory basename must become the project name"
    );
}

/// `swag init` refuses missing and non-empty destinations; `--force` empties
/// a non-empty destination but keeps the directory itself.
#[test]
fn init_destination_guards_and_force() {
    let home = Home::fresh("init-guards");
    let src = home.write_template("tpl-guards");
    home.register_template("local-guards", &src);

    let missing = home.dir.join("nope");
    let (_, err, code) = home.run_with_stdin(
        &[
            "init",
            "--template",
            "local-guards",
            "--output",
            missing.to_str().expect("utf8"),
            "--no-install",
            "--no-git-init",
        ],
        "demo\n",
    );
    assert_ne!(code, 0);
    assert!(err.contains("pre-existing directory"), "{err}");

    let dirty = home.dir.join("dirty");
    std::fs::create_dir_all(&dirty).unwrap();
    std::fs::write(dirty.join("stale.txt"), "stale").unwrap();
    let (_, err, code) = home.run_with_stdin(
        &[
            "init",
            "--template",
            "local-guards",
            "--output",
            dirty.to_str().expect("utf8"),
            "--no-install",
            "--no-git-init",
        ],
        "demo\n",
    );
    assert_ne!(code, 0);
    assert!(err.contains("already exists"), "{err}");

    let (out, err, code) = home.run_with_stdin(
        &[
            "init",
            "--template",
            "local-guards",
            "--output",
            dirty.to_str().expect("utf8"),
            "--force",
            "--no-install",
            "--no-git-init",
        ],
        "demo\n",
    );
    assert_eq!(code, 0, "{out} {err}");
    assert!(dirty.is_dir(), "the directory itself must be kept");
    assert!(!dirty.join("stale.txt").exists(), "stale files are cleared");
    assert!(dirty.join("package.json").is_file());
}

/// The `--template` help documents the default without an explicit
/// `default_value` (an absent flag prompts instead).
#[test]
fn help_documents_template_default() {
    let home = Home::fresh("help");
    let (out, _, code) = home.run(&["new", "--help"]);
    assert_eq!(code, 0, "{out}");
    assert!(
        out.contains("template id: [default: svelte-clean-template]"),
        "{out}"
    );

    let (out, _, code) = home.run(&["init", "--help"]);
    assert_eq!(code, 0, "{out}");
    assert!(
        out.contains("template id: [default: svelte-clean-template]"),
        "{out}"
    );
}

#[test]
fn relative_template_path_is_canonicalized() {
    let home = Home::fresh("rel-template");
    let src = home.dir.join("src");
    std::fs::create_dir_all(&src).unwrap();
    std::fs::write(
        src.join("package.json"),
        r#"{"name": "svelte-clean-template"}"#,
    )
    .unwrap();

    let (out, err, code) = swag_in_dir(
        &home.dir,
        &home.dir,
        &[
            "templates",
            "add",
            "local-rel",
            "--base",
            "clean",
            "--path",
            "src",
        ],
    );
    assert_eq!(code, 0, "{out} {err}");

    let (out, _, code) = home.run(&["templates", "show", "local-rel", "--json"]);
    assert_eq!(code, 0, "{out}");
    let parsed: serde_json::Value = serde_json::from_str(&out).expect("valid JSON");
    let stored = parsed["source"]["path"].as_str().expect("source path");
    assert!(
        std::path::Path::new(stored).is_absolute(),
        "stored path must be absolute: {stored}"
    );
    assert_eq!(
        std::path::Path::new(stored).canonicalize().unwrap(),
        src.canonicalize().unwrap()
    );

    // Scaffold from a different working directory: the stored absolute path
    // must still resolve.
    let other = unique_home("rel-template-cwd");
    std::fs::create_dir_all(&other).unwrap();
    let dest = home.dir.join("out").join("demo");
    let (out, err, code) = swag_in_dir(
        &home.dir,
        &other,
        &[
            "new",
            "--template",
            "local-rel",
            "demo",
            "--output",
            dest.to_str().expect("utf8"),
            "--no-install",
            "--no-git-init",
        ],
    );
    assert_eq!(code, 0, "{out} {err}");
    assert!(
        std::fs::read_to_string(dest.join("package.json"))
            .expect("scaffolded package.json")
            .contains("demo")
    );
    let _ = std::fs::remove_dir_all(&other);
}

#[test]
fn force_refuses_to_remove_cwd_or_ancestor() {
    let home = Home::fresh("force-guard");
    let base = home.dir.join("base");
    let child = base.join("child");
    std::fs::create_dir_all(&child).unwrap();
    std::fs::write(child.join("keep.txt"), "keep").unwrap();

    let (_, err, code) = swag_in_dir(
        &home.dir,
        &child,
        &[
            "new",
            "--force",
            "--template",
            "clean",
            "demo",
            "--output",
            base.to_str().expect("utf8"),
        ],
    );
    assert_ne!(code, 0);
    assert!(err.contains("unsafe destination"), "{err}");
    assert!(child.join("keep.txt").is_file(), "files must be untouched");

    let (_, err, code) = swag_in_dir(
        &home.dir,
        &child,
        &[
            "new",
            "--force",
            "--template",
            "clean",
            "demo",
            "--output",
            ".",
        ],
    );
    assert_ne!(code, 0);
    assert!(err.contains("unsafe destination"), "{err}");
    assert!(child.join("keep.txt").is_file(), "files must be untouched");
}

#[test]
fn projects_add_validates_owner_and_template() {
    let home = Home::fresh("add-validation");
    let target = std::env::temp_dir();
    let target = target.to_str().expect("utf8 temp dir");

    let (_, err, code) = home.run(&[
        "projects",
        "add",
        "demo",
        "--path",
        target,
        "--owner",
        "bad owner",
    ]);
    assert_ne!(code, 0);
    assert!(err.contains("invalid GitHub owner"), "{err}");

    let (_, err, code) = home.run(&[
        "projects",
        "add",
        "demo",
        "--path",
        target,
        "--template",
        "no-such-template",
    ]);
    assert_ne!(code, 0);
    assert!(err.contains("unknown template"), "{err}");

    let (out, _, code) = home.run(&[
        "projects",
        "add",
        "demo",
        "--path",
        target,
        "--template",
        "clean",
    ]);
    assert_eq!(code, 0, "{out}");
    let (out, _, code) = home.run(&["projects", "show", "demo", "--json"]);
    assert_eq!(code, 0, "{out}");
    let parsed: serde_json::Value = serde_json::from_str(&out).expect("valid JSON");
    assert_eq!(parsed["template"], "svelte-clean-template");
}
