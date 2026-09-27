//! Project scaffolding.
//!
//! The default source of truth is GitHub: each template is fetched with
//! `git clone --depth 1 https://github.com/gi-dellav/<template>.git`,
//! then personalized (template id → project name, template owner → `--owner`),
//! detached from the template's git history, and optionally provisioned with
//! `bun install`.
//!
//! Template-name occurrences were inventoried from the live templates:
//! - `svelte-clean-template`: `package.json` + `bun.lock` (name),
//!   `index.html` + `src/Seo.svelte` + `plugins/seo.ts` + `vite.config.ts`
//!   (PWA manifest / SEO site name), `public/favicon.svg` (aria-label),
//!   `src/App.svelte` (eyebrow + GitHub links), `README.md`;
//!   `vite.config.ts` derives the Pages base path dynamically so the
//!   sub-path itself needs no rewrite.
//! - `svelte-tauri-template`: `package.json` (name), `index.html` (title),
//!   `src-tauri/Cargo.toml` (name, `default-run`, repository) +
//!   `src-tauri/Cargo.lock` (name), `src-tauri/tauri.conf.json`
//!   (productName, identifier), `src/routes/Home.svelte` (eyebrow),
//!   `README.md`.
//! - `svelte-fullstack-template`: currently a stub (README only); the
//!   generic text-file walk below covers whatever it grows into.
//!
//! Owner occurrences (`gi-dellav` in repo slugs, URLs and Pages domains;
//! `gidellav` in the Tauri bundle identifier `com.gidellav.*`, where hyphens
//! are illegal) are rewritten only when `--owner` differs from the default.
//!
//! Rather than hard-coding that file list (which would rot), [`scaffold`]
//! rewrites every text file under the destination except VCS metadata
//! (`.git`), dependency build trees (`node_modules`, `target`) and build
//! output (`dist`). Symlinks are never followed, non-UTF8 files are left
//! alone, and files over 512 KiB are skipped (binary-asset heuristic).

use std::ffi::OsStr;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

use serde::Serialize;

use crate::bun::Bun;
use crate::error::{Error, Result};
use crate::template::Template;

/// Directories never entered during personalization.
const SKIPPED_DIRS: [&str; 4] = [".git", "node_modules", "target", "dist"];
/// Extensions considered text for the rewrite pass.
const TEXT_EXTENSIONS: [&str; 15] = [
    "json", "md", "html", "ts", "js", "svelte", "toml", "yml", "yaml", "css", "txt", "svg", "xml",
    "lock", "rs",
];
/// Files without an extension that are still rewritten when present.
const TEXT_BASENAMES: [&str; 5] = ["Dockerfile", "LICENSE", "README", "AGENTS.md", "DESIGN.md"];
/// Do not touch files larger than this (binary-asset heuristic).
const MAX_REWRITE_BYTES: u64 = 512 * 1024;

/// Options controlling [`scaffold`].
#[derive(Debug, Clone)]
pub struct ScaffoldOptions {
    /// Which template to clone.
    pub template: Template,
    /// New project name (directory + package name).
    pub name: String,
    /// Where to create the project. Defaults to `<cwd>/<name>`.
    pub destination: Option<PathBuf>,
    /// Clone a branch / tag instead of the default branch.
    pub git_ref: Option<String>,
    /// Remove the destination first if it already exists.
    pub force: bool,
    /// GitHub owner replacing `gi-dellav` in repo slugs, URLs, Pages domains
    /// and the Tauri `com.gidellav.*` bundle identifier. Defaults to
    /// `gi-dellav` (template author); set it to your own handle so the
    /// scaffolded README links, `Cargo.toml` repository and `identifier`
    /// point at you from the start.
    pub owner: Option<String>,
    /// Skip the `bun install` provisioning step.
    pub no_install: bool,
    /// Skip the fresh `git init` after detaching template history.
    pub no_git_init: bool,
}

impl ScaffoldOptions {
    /// Build minimal options for `template` + `name`.
    pub fn new(template: Template, name: &str) -> Self {
        Self {
            template,
            name: name.to_string(),
            destination: None,
            git_ref: None,
            force: false,
            owner: None,
            no_install: false,
            no_git_init: false,
        }
    }
}

/// A scaffolded project on disk.
#[derive(Debug, Clone)]
pub struct Project {
    /// Project (package / directory) name.
    pub name: String,
    /// Template it was created from.
    pub template: Template,
    /// Absolute path of the project root.
    pub path: PathBuf,
}

impl Project {
    /// `bun install` in the project root.
    pub fn install(&self) -> Result<()> {
        Bun::new()?.install(&self.path)
    }

    /// `bun run dev [-- extra...]` in the project root.
    pub fn dev(&self, extra: &[String]) -> Result<()> {
        Bun::new()?.dev(&self.path, extra)
    }

    /// `bun run build [-- extra...]` in the project root.
    pub fn build(&self, extra: &[String]) -> Result<()> {
        Bun::new()?.build(&self.path, extra)
    }

    /// `bun run check [-- extra...]` (svelte-check) in the project root.
    pub fn check(&self, extra: &[String]) -> Result<()> {
        Bun::new()?.check(&self.path, extra)
    }

    /// `bun run test [-- extra...]` (`bun test`) in the project root.
    pub fn test(&self, extra: &[String]) -> Result<()> {
        Bun::new()?.test(&self.path, extra)
    }

    /// `bun run preview [-- extra...]` in the project root.
    pub fn preview(&self, extra: &[String]) -> Result<()> {
        Bun::new()?.preview(&self.path, extra)
    }
}

/// JSON-serializable template catalogue entry (for `swag list --json`).
#[derive(Debug, Clone, Serialize)]
pub struct TemplateInfo {
    /// CLI-facing id, e.g. `svelte-clean-template`.
    pub id: String,
    /// One-line description.
    pub description: String,
    /// HTTPS clone URL.
    pub clone_url: String,
    /// Whether the template ships Rust code needing `cargo`.
    pub needs_rust: bool,
}

/// Catalogue of every built-in template.
#[must_use]
pub fn list_templates() -> Vec<TemplateInfo> {
    Template::ALL
        .iter()
        .map(|t| TemplateInfo {
            id: t.id().to_string(),
            description: t.description().to_string(),
            clone_url: t.clone_url(),
            needs_rust: t.needs_rust(),
        })
        .collect()
}

/// Clone, personalize, detach history and optionally `bun install`.
///
/// Steps:
/// 1. validate the project name (and `--owner` when given);
/// 2. resolve the destination (`--output` or `<cwd>/<name>`);
/// 3. `git clone --depth 1 <template-url> <dest>`;
/// 4. rewrite template-name occurrences → project name (and, when `--owner`
///    differs from the template default, `gi-dellav` → owner everywhere,
///    `gidellav` → owner with hyphens stripped in the Tauri identifier);
/// 5. delete the cloned `.git/` and (unless `no_git_init`) `git init`;
/// 6. run `bun install` (unless `no_install`).
pub fn scaffold(options: &ScaffoldOptions) -> Result<Project> {
    validate_project_name(options.template, &options.name)?;
    let owner = resolve_owner(options.owner.as_deref())?;
    let dest = resolve_destination(&options.name, options.destination.as_deref())?;
    prepare_destination(&dest, options.force)?;
    clone_template(options.template, &dest, options.git_ref.as_deref())?;
    personalize(&dest, options.template, &options.name, owner.as_deref())?;
    detach_history(&dest)?;
    if !options.no_git_init {
        git_init(&dest)?;
    }

    let project = Project {
        name: options.name.clone(),
        template: options.template,
        path: dest.clone(),
    };
    if !options.no_install {
        Bun::new()?.install(&dest)?;
    }
    Ok(project)
}

/// Ensure `name` is usable as a directory and a `package.json` name.
///
/// Dots are accepted for plain web projects (npm allows `my.app`) but
/// rejected for the Rust-bearing templates (tauri, fullstack): a `.`
/// would leak into `src-tauri/Cargo.toml` package names, where Cargo
/// forbids it.
pub fn validate_project_name(template: Template, name: &str) -> Result<()> {
    if name.is_empty() {
        return Err(Error::InvalidName(
            name.to_string(),
            "name must not be empty".to_string(),
        ));
    }
    if name.len() > 214 {
        return Err(Error::InvalidName(
            name.to_string(),
            "name must be at most 214 characters".to_string(),
        ));
    }
    let mut chars = name.chars();
    let first = chars.next().expect("non-empty checked above");
    if !first.is_ascii_alphanumeric() {
        return Err(Error::InvalidName(
            name.to_string(),
            "name must start with a letter or digit".to_string(),
        ));
    }
    if !name
        .chars()
        .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_' || c == '.')
    {
        return Err(Error::InvalidName(
            name.to_string(),
            "name may only contain letters, digits, '-', '_' and '.'".to_string(),
        ));
    }
    if template.needs_rust() && name.contains('.') {
        return Err(Error::InvalidName(
            name.to_string(),
            format!(
                "the {} template ships Rust code (Cargo package names forbid '.'); use '-' or '_' instead",
                template.id()
            ),
        ));
    }
    Ok(())
}

/// Resolve the effective GitHub owner: the template default (`gi-dellav`)
/// when `--owner` is absent, otherwise the validated custom handle.
///
/// GitHub usernames may only contain alphanumerics and hyphens, may not
/// start or end with a hyphen, and are at most 39 characters.
fn resolve_owner(owner: Option<&str>) -> Result<Option<String>> {
    let Some(raw) = owner else {
        return Ok(None);
    };
    validate_owner(raw)?;
    Ok(Some(raw.to_string()))
}

/// Ensure `owner` is a plausible GitHub username (see [`resolve_owner`]).
fn validate_owner(owner: &str) -> Result<()> {
    let bad = |why: &str| Error::InvalidOwner(owner.to_string(), why.to_string());
    if owner.is_empty() {
        return Err(bad("owner must not be empty"));
    }
    if owner.len() > 39 {
        return Err(bad("owner must be at most 39 characters"));
    }
    if !owner.chars().all(|c| c.is_ascii_alphanumeric() || c == '-') {
        return Err(bad(
            "owner may only contain letters, digits and '-' (GitHub username rules)",
        ));
    }
    if owner.starts_with('-') || owner.ends_with('-') {
        return Err(bad("owner must not start or end with '-'"));
    }
    Ok(())
}

/// If the user passed --output use it, else <cwd>/<name>.
fn resolve_destination(name: &str, output: Option<&Path>) -> Result<PathBuf> {
    if let Some(dir) = output {
        return Ok(dir.to_path_buf());
    }
    let cwd = std::env::current_dir().map_err(|e| Error::Io {
        context: "could not determine current directory".to_string(),
        source: e,
    })?;
    Ok(cwd.join(name))
}

/// Remove the destination when `--force` is given, else refuse to overwrite.
fn prepare_destination(dest: &Path, force: bool) -> Result<()> {
    if !dest.exists() {
        return Ok(());
    }
    let non_empty = dest
        .read_dir()
        .map_err(|e| Error::Io {
            context: format!("could not read destination '{}'", dest.display()),
            source: e,
        })?
        .next()
        .is_some();
    if non_empty && !force {
        return Err(Error::DestinationExists(dest.display().to_string()));
    }
    if force {
        fs::remove_dir_all(dest).map_err(|e| Error::Io {
            context: format!("could not remove destination '{}'", dest.display()),
            source: e,
        })?;
    }
    Ok(())
}

fn clone_template(template: Template, dest: &Path, git_ref: Option<&str>) -> Result<()> {
    let mut cmd = Command::new("git");
    cmd.arg("clone").arg("--depth").arg("1");
    if let Some(git_ref) = git_ref {
        cmd.arg("--branch").arg(git_ref);
    }
    cmd.arg(template.clone_url()).arg(dest);
    let status = cmd.status().map_err(|e| Error::FetchFailed {
        template: template.id().to_string(),
        reason: format!("could not spawn git: {e} (is git installed?)"),
    })?;
    if !status.success() {
        return Err(Error::FetchFailed {
            template: template.id().to_string(),
            reason: format!("`git clone` exited with {status}"),
        });
    }
    Ok(())
}

/// Template author handle baked into the templates: `gi-dellav` in repo
/// slugs, GitHub URLs and Pages domains.
const TEMPLATE_OWNER: &str = "gi-dellav";
/// Compact form used in the Tauri bundle identifier (`com.gidellav.*`),
/// where hyphens are illegal.
const TEMPLATE_OWNER_COMPACT: &str = "gidellav";

/// Rewrite every template-name occurrence under `root`.
///
/// Covers the inventoried spots (`package.json` + `bun.lock` names,
/// PWA manifest / SEO site names, `Cargo.toml` name / `default-run` /
/// repository, `tauri.conf.json` productName / identifier, Svelte eyebrows,
/// README links) plus anything future templates add, by walking text files.
/// When `owner` is `Some`, `gi-dellav` → owner everywhere and
/// `gidellav` → owner-without-hyphens (keeps the Tauri identifier legal).
fn personalize(root: &Path, template: Template, name: &str, owner: Option<&str>) -> Result<()> {
    let replacements = replacement_table(template, name, owner);
    let files = collect_text_files(root)?;
    for file in files {
        let bytes = fs::read(&file).map_err(|e| Error::Io {
            context: format!("could not read '{}'", file.display()),
            source: e,
        })?;
        // Skip binary blobs (icons, fonts): only valid UTF-8 is rewritten.
        let Ok(original) = String::from_utf8(bytes) else {
            continue;
        };
        let rewritten = apply_replacements(&original, &replacements);
        if rewritten != original {
            fs::write(&file, rewritten).map_err(|e| Error::Io {
                context: format!("could not write '{}'", file.display()),
                source: e,
            })?;
        }
    }
    Ok(())
}

/// Ordered (pattern → replacement) table. Order matters: snake-case first so
/// `svelte_clean_template` is rewritten before the kebab-case pass would
/// partially match, then Title Case, then the plain id; compact owner
/// (`gidellav`, no hyphens possible) before the plain owner so
/// `gi-dellav` never partially rewrites it.
fn replacement_table(template: Template, name: &str, owner: Option<&str>) -> Vec<(String, String)> {
    let id = template.id();
    let snake_from = id.replace('-', "_");
    let snake_to = name.replace('-', "_");
    let mut table = vec![
        (snake_from, snake_to),
        (title_case(id), title_case(name)),
        (id.to_string(), name.to_string()),
    ];
    if let Some(owner) = owner.filter(|o| *o != TEMPLATE_OWNER) {
        table.push((TEMPLATE_OWNER_COMPACT.to_string(), owner.replace('-', "")));
        table.push((TEMPLATE_OWNER.to_string(), owner.to_string()));
    }
    table
}

fn apply_replacements(content: &str, replacements: &[(String, String)]) -> String {
    let mut out = content.to_string();
    for (from, to) in replacements {
        out = out.replace(from, to);
    }
    out
}

/// "svelte-clean-template" → "Svelte Clean Template".
fn title_case(id: &str) -> String {
    id.split(['-', '_'])
        .filter(|w| !w.is_empty())
        .map(|w| {
            let mut chars = w.chars();
            let first = chars.next().expect("non-empty checked above");
            format!("{}{}", first.to_ascii_uppercase(), chars.as_str())
        })
        .collect::<Vec<_>>()
        .join(" ")
}

fn collect_text_files(root: &Path) -> Result<Vec<PathBuf>> {
    let mut out = Vec::new();
    let mut stack = vec![root.to_path_buf()];
    while let Some(dir) = stack.pop() {
        let entries = fs::read_dir(&dir).map_err(|e| Error::Io {
            context: format!("could not list '{}'", dir.display()),
            source: e,
        })?;
        for entry in entries {
            let entry = entry.map_err(|e| Error::Io {
                context: format!("could not list '{}'", dir.display()),
                source: e,
            })?;
            let path = entry.path();
            let file_name = entry.file_name().to_string_lossy().into_owned();
            let file_type = entry.file_type().map_err(|e| Error::Io {
                context: format!("could not stat '{}'", path.display()),
                source: e,
            })?;
            // Never follow symlinks: a template could link outside the tree.
            if file_type.is_symlink() {
                continue;
            }
            if file_type.is_dir() {
                if !SKIPPED_DIRS.contains(&file_name.as_str()) {
                    stack.push(path);
                }
                continue;
            }
            if is_text_file(&path, &file_name) {
                if let Ok(meta) = entry.metadata()
                    && meta.len() > MAX_REWRITE_BYTES
                {
                    continue;
                }
                out.push(path);
            }
        }
    }
    Ok(out)
}

fn is_text_file(path: &Path, file_name: &str) -> bool {
    if TEXT_BASENAMES.contains(&file_name) {
        return true;
    }
    match path.extension().and_then(OsStr::to_str) {
        Some(ext) => TEXT_EXTENSIONS.contains(&ext.to_ascii_lowercase().as_str()),
        None => false,
    }
}

/// Delete the template's `.git/` so the new project starts detached.
fn detach_history(dest: &Path) -> Result<()> {
    let git_dir = dest.join(".git");
    if git_dir.exists() {
        fs::remove_dir_all(&git_dir).map_err(|e| Error::Io {
            context: format!("could not remove template history '{}'", git_dir.display()),
            source: e,
        })?;
    }
    Ok(())
}

fn git_init(dest: &Path) -> Result<()> {
    let status = Command::new("git")
        .arg("init")
        .arg("--quiet")
        .current_dir(dest)
        .status()
        .map_err(|e| Error::CommandFailed(format!("failed to spawn `git init`: {e}")))?;
    if !status.success() {
        return Err(Error::CommandFailed(format!(
            "`git init` in '{}' exited with {status}",
            dest.display()
        )));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    #[test]
    fn validates_names() {
        assert!(validate_project_name(Template::SvelteClean, "my-app").is_ok());
        assert!(validate_project_name(Template::SvelteClean, "my_app.v2").is_ok());
        // Dots break Cargo package names in the Rust-bearing templates.
        assert!(validate_project_name(Template::SvelteTauri, "my.app").is_err());
        assert!(validate_project_name(Template::SvelteFullstack, "my.app").is_err());
        assert!(validate_project_name(Template::SvelteClean, "").is_err());
        assert!(validate_project_name(Template::SvelteClean, "-bad").is_err());
        assert!(validate_project_name(Template::SvelteClean, "has space").is_err());
        assert!(validate_project_name(Template::SvelteClean, "has/slash").is_err());
    }

    #[test]
    fn validates_owners() {
        assert!(validate_owner("some-user123").is_ok());
        assert!(validate_owner("").is_err());
        assert!(validate_owner("has space").is_err());
        assert!(validate_owner("has_underscore").is_err());
        assert!(validate_owner("-leading").is_err());
        assert!(validate_owner("trailing-").is_err());
    }

    #[test]
    fn default_owner_adds_no_replacements() {
        let table = replacement_table(Template::SvelteClean, "demo", None);
        assert_eq!(table.len(), 3);
        let table = replacement_table(Template::SvelteClean, "demo", Some("gi-dellav"));
        assert_eq!(table.len(), 3, "default owner must be a no-op");
    }

    #[test]
    fn title_cases_ids() {
        assert_eq!(title_case("svelte-clean-template"), "Svelte Clean Template");
        assert_eq!(title_case("my-app"), "My App");
    }

    #[test]
    fn rewrites_all_known_spots() {
        let content = r#"
{"name": "svelte-clean-template"}
# Svelte Clean Template
identifier: com.gidellav.svelte_clean_template
default-run = "svelte-clean-template"
repository = "https://github.com/gi-dellav/svelte-clean-template"
"#;
        let table = replacement_table(Template::SvelteClean, "my-blog", Some("octo-cat"));
        let out = apply_replacements(content, &table);
        assert!(out.contains("\"my-blog\""), "{out}");
        assert!(out.contains("My Blog"), "{out}");
        assert!(out.contains("my_blog"), "{out}");
        assert!(out.contains("com.octocat.my_blog"), "{out}");
        assert!(out.contains("https://github.com/octo-cat/my-blog"), "{out}");
        assert!(!out.contains("svelte-clean-template"), "{out}");
        assert!(!out.contains("gi-dellav"), "{out}");
        assert!(!out.contains("gidellav"), "{out}");
    }

    #[test]
    fn personalizes_a_fake_tree_and_skips_artefacts() {
        let root = std::env::temp_dir().join(format!(
            "swag-test-{}",
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .expect("time")
                .as_nanos()
        ));
        fs::create_dir_all(root.join("src")).unwrap();
        fs::create_dir_all(root.join("node_modules")).unwrap();
        fs::write(
            root.join("package.json"),
            r#"{"name": "svelte-clean-template"}"#,
        )
        .unwrap();
        fs::write(root.join("bun.lock"), "svelte-clean-template lock").unwrap();
        fs::write(
            root.join("node_modules/skipped.json"),
            "svelte-clean-template",
        )
        .unwrap();

        personalize(&root, Template::SvelteClean, "demo", None).unwrap();

        assert!(
            fs::read_to_string(root.join("package.json"))
                .unwrap()
                .contains("demo")
        );
        assert!(
            fs::read_to_string(root.join("bun.lock"))
                .unwrap()
                .contains("demo"),
            "bun.lock workspace name must be rewritten too"
        );
        assert!(
            fs::read_to_string(root.join("node_modules/skipped.json"))
                .unwrap()
                .contains("svelte-clean-template"),
            "node_modules must be skipped"
        );
        fs::remove_dir_all(&root).unwrap();
    }

    #[test]
    fn skips_symlinks_and_binary_blobs() {
        let stamp = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .expect("time")
            .as_nanos();
        let root = std::env::temp_dir().join(format!("swag-test-{stamp}"));
        let sibling = std::env::temp_dir().join(format!("swag-test-{stamp}-sibling"));
        fs::create_dir_all(&root).unwrap();
        fs::create_dir_all(&sibling).unwrap();
        let outside = sibling.join("outside.txt");
        fs::write(&outside, "svelte-clean-template").unwrap();
        #[cfg(unix)]
        std::os::unix::fs::symlink(&outside, root.join("link.json")).unwrap();
        // Invalid UTF-8 must be left untouched, not error.
        fs::write(root.join("blob.json"), [0x66, 0xff, 0x66]).unwrap();

        personalize(&root, Template::SvelteClean, "demo", None).unwrap();

        assert!(
            fs::read_to_string(&outside)
                .unwrap()
                .contains("svelte-clean-template"),
            "symlink target outside the walked tree must be untouched"
        );
        assert_eq!(
            fs::read(root.join("blob.json")).unwrap(),
            [0x66, 0xff, 0x66]
        );
        fs::remove_dir_all(&root).unwrap();
        fs::remove_dir_all(&sibling).unwrap();
    }
}
