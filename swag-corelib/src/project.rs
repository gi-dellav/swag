//! Project scaffolding.
//!
//! The default source of truth is GitHub: each template is fetched with
//! `git clone --depth 1 https://github.com/gi-dellav/<template>.git`,
//! then personalized (template id → project name, template owner → `--owner`),
//! detached from the template's git history, and optionally provisioned with
//! `swag install` (Bun + Cargo, depending on the template).
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
//! - `svelte-fullstack-template`: `frontend/package.json` +
//!   `frontend/bun.lock` and `backend/Cargo.toml` + `backend/Cargo.lock`
//!   (names); the generic text-file walk below covers the rest.
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

use crate::custom_templates::{self, ResolvedTemplate, TemplateSource};
use crate::error::{Error, Result};
use crate::template::Template;

/// Directories never entered during personalization (`.vercel` holds local
/// link state created by `swag vc`, never template content).
const SKIPPED_DIRS: [&str; 5] = [".git", "node_modules", "target", "dist", ".vercel"];
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
    /// Which template to clone (builtin id/alias or custom template name).
    pub template: String,
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
    /// Skip the template-aware dependency provisioning step.
    pub no_install: bool,
    /// Skip the fresh `git init` after detaching template history.
    pub no_git_init: bool,
    /// Scaffold into a pre-existing directory instead of creating a new one
    /// (`swag init`): the destination must already exist, and `--force` is
    /// not allowed to delete it first.
    pub init: bool,
}

impl ScaffoldOptions {
    /// Build minimal options for `template` + `name`.
    pub fn new(template: Template, name: &str) -> Self {
        Self {
            template: template.id().to_string(),
            name: name.to_string(),
            destination: None,
            git_ref: None,
            force: false,
            owner: None,
            no_install: false,
            no_git_init: false,
            init: false,
        }
    }
}

/// A scaffolded project on disk.
#[derive(Debug, Clone)]
pub struct Project {
    /// Project (package / directory) name.
    pub name: String,
    /// Template it was created from (builtin id or custom name).
    pub template: String,
    /// Absolute path of the project root.
    pub path: PathBuf,
}

impl Project {
    /// Run a template-aware action against this project.
    ///
    /// `web` / `rust` are the CLI selection flags; passing neither drives
    /// every component the template provides (web + Rust for fullstack and
    /// desktop, web only for the static template). `extra` is forwarded only
    /// when the action resolves to a single command.
    pub fn action(
        &self,
        action: crate::tasks::Action,
        web: bool,
        rust: bool,
        extra: &[String],
    ) -> Result<()> {
        let layout = crate::layout::ProjectLayout::detect(&self.path)?;
        let plan = crate::tasks::plan(&layout, action, web, rust, extra)?;
        crate::tasks::execute(&plan)
    }

    /// Install Bun and Cargo dependencies for every component.
    pub fn install(&self) -> Result<()> {
        self.action(crate::tasks::Action::Install, false, false, &[])
    }

    /// Start the template's development servers.
    pub fn dev(&self, extra: &[String]) -> Result<()> {
        self.action(crate::tasks::Action::Dev, false, false, extra)
    }

    /// Build every component for production.
    pub fn build(&self, extra: &[String]) -> Result<()> {
        self.action(crate::tasks::Action::Build, false, false, extra)
    }

    /// Run type checks, formatting and lints for every component.
    pub fn check(&self, extra: &[String]) -> Result<()> {
        self.action(crate::tasks::Action::Check, false, false, extra)
    }

    /// Run unit tests for every component.
    pub fn test(&self, extra: &[String]) -> Result<()> {
        self.action(crate::tasks::Action::Test, false, false, extra)
    }

    /// Preview the web production build.
    pub fn preview(&self, extra: &[String]) -> Result<()> {
        self.action(crate::tasks::Action::Preview, false, false, extra)
    }

    /// Run the production runtime (requires a prior build).
    pub fn run(&self, extra: &[String]) -> Result<()> {
        self.action(crate::tasks::Action::Run, false, false, extra)
    }
}

/// JSON-serializable template catalogue entry (for `swag list --json`).
#[derive(Debug, Clone, Serialize)]
pub struct TemplateInfo {
    /// CLI-facing id, e.g. `svelte-clean-template` (or a custom name).
    pub id: String,
    /// One-line description.
    pub description: String,
    /// HTTPS clone URL, git URL or local path of the template source.
    pub clone_url: String,
    /// Whether the template ships Rust code needing `cargo`.
    pub needs_rust: bool,
    /// Whether this is one of the three built-in templates.
    pub builtin: bool,
    /// For custom templates: the builtin id they derive from.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub base: Option<String>,
}

/// Catalogue of built-in plus user-registered templates.
#[must_use]
pub fn list_templates() -> Vec<TemplateInfo> {
    let mut out: Vec<TemplateInfo> = Template::ALL
        .iter()
        .map(|t| TemplateInfo {
            id: t.id().to_string(),
            description: t.description().to_string(),
            clone_url: t.clone_url(),
            needs_rust: t.needs_rust(),
            builtin: true,
            base: None,
        })
        .collect();
    if let Ok(customs) = custom_templates::list() {
        out.extend(customs.iter().map(|c| TemplateInfo {
            id: c.name.clone(),
            description: c.description.clone(),
            clone_url: c.source.describe(),
            needs_rust: c.base.needs_rust(),
            builtin: false,
            base: Some(c.base.id().to_string()),
        }));
    }
    out
}

/// Clone, personalize, detach history and optionally install dependencies.
///
/// Steps:
/// 1. validate the project name (and `--owner` when given);
/// 2. resolve the destination (`--output`, `<cwd>/<name>`, or the
///    pre-existing directory for `init` mode);
/// 3. fetch the template: builtin/custom-git → `git clone --depth 1`,
///    custom-path → recursive copy;
/// 4. rewrite template-name occurrences → project name (custom templates also
///    rewrite their base id; and, when `--owner` differs from the template
///    default, `gi-dellav` → owner everywhere, `gidellav` → owner with
///    hyphens stripped in the Tauri identifier);
/// 5. delete the cloned `.git/` and (unless `no_git_init`) `git init`;
/// 6. run the template-aware install (Bun and/or Cargo, unless `no_install`);
/// 7. auto-register the project in `~/.swag/projects.json`.
///
/// In `init` mode (`options.init`) the destination must already exist:
/// `swag init` scaffolds into a pre-existing directory instead of creating
/// a new one, so `--force` empties it (keeping the directory itself)
/// instead of deleting it first.
pub fn scaffold(options: &ScaffoldOptions) -> Result<Project> {
    let resolved = custom_templates::resolve_id(&options.template)?;
    validate_project_name_resolved(&resolved, &options.name)?;
    let owner = resolve_owner(options.owner.as_deref())?;
    let dest = if options.init {
        resolve_init_destination(options.destination.as_deref())?
    } else {
        resolve_destination(&options.name, options.destination.as_deref())?
    };
    prepare_destination(&dest, options.force, options.init)?;
    fetch_template(&resolved, &dest, options.git_ref.as_deref())?;
    // Capture the exact upstream revision while the clone's `.git/` still
    // exists (it is deleted just below): provenance then pins the source.
    let commit = head_commit(&dest);
    let template_ids = resolved_template_ids(&resolved);
    personalize_ids(&dest, &template_ids, &options.name, owner.as_deref())?;
    let template_files = template_manifest(&dest)?;
    write_provenance(
        &dest,
        &resolved,
        &options.name,
        owner.as_deref(),
        commit,
        Some(template_files),
    )?;
    detach_history(&dest)?;
    if !options.no_git_init {
        git_init(&dest)?;
    }

    let project = Project {
        name: options.name.clone(),
        template: resolved.id(),
        path: dest.clone(),
    };
    if !options.no_install {
        project.install()?;
    }
    // Auto-register (best effort resolved through the registry layer; a
    // registry failure after a successful scaffold must fail loudly so the
    // user knows the project exists but is not tracked).
    let previous = crate::projects::upsert(&options.name, &dest, &resolved.id(), owner.as_deref())?;
    if previous.is_some_and(|p| p.path != dest) {
        eprintln!(
            "swag: project '{}' re-registered (previous path replaced)",
            options.name
        );
    }
    Ok(project)
}

/// Ids rewritten during personalization: the resolved id plus, for custom
/// templates, the base builtin id (forks still contain base strings).
/// Capture `.swag.json` provenance right after personalization so `update`
/// and `doctor` work from the project directory alone.
fn write_provenance(
    dest: &Path,
    resolved: &ResolvedTemplate,
    name: &str,
    owner: Option<&str>,
    commit: Option<String>,
    template_files: Option<Vec<String>>,
) -> Result<()> {
    let (template, base) = match resolved {
        ResolvedTemplate::Builtin(t) => (t.id().to_string(), None),
        ResolvedTemplate::Custom(c) => (c.name.clone(), Some(c.base.id().to_string())),
    };
    let mut provenance = crate::provenance::Provenance::new(
        name,
        &template,
        base.as_deref(),
        crate::template::read_template_version(dest),
        crate::template::read_template_type(dest),
        owner,
    );
    provenance.commit = commit;
    provenance.template_files = template_files;
    crate::provenance::save(dest, &provenance)
}

/// Relative paths of every regular file in a template tree, sorted.
///
/// This is the manifest recorded in `.swag.json`: `swag update --prune` only
/// deletes files that were template-managed before and disappeared upstream,
/// never files the user created.
pub(crate) fn template_manifest(root: &Path) -> Result<Vec<String>> {
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
            if file_type.is_symlink() {
                continue;
            }
            if file_type.is_dir() {
                if !SKIPPED_DIRS.contains(&file_name.as_str()) {
                    stack.push(path);
                }
                continue;
            }
            if !file_type.is_file() {
                continue;
            }
            if file_name == crate::provenance::PROVENANCE_FILE {
                continue;
            }
            let rel = path
                .strip_prefix(root)
                .map_err(|e| Error::Internal(format!("could not build template manifest: {e}")))?;
            if let Some(rel) = rel.to_str() {
                out.push(rel.to_string());
            }
        }
    }
    out.sort();
    Ok(out)
}

/// Best-effort `git rev-parse HEAD` in `dir`, used to record the exact
/// upstream revision a project was scaffolded from (templates are cloned
/// unpinned from the default branch; the commit makes builds reproducible).
///
/// `dir` must itself be the repository root: `git` walks parent directories,
/// so without this check scaffolding a local template into a subdirectory of
/// an existing repo would record the enclosing repo's HEAD as the template
/// revision.
pub(crate) fn head_commit(dir: &Path) -> Option<String> {
    let top = Command::new("git")
        .args(["rev-parse", "--show-toplevel"])
        .current_dir(dir)
        .output()
        .ok()?;
    if !top.status.success() {
        return None;
    }
    let top = PathBuf::from(String::from_utf8_lossy(&top.stdout).trim());
    let top = top.canonicalize().ok()?;
    let dir = dir.canonicalize().ok()?;
    if top != dir {
        return None;
    }
    let output = Command::new("git")
        .args(["rev-parse", "HEAD"])
        .current_dir(dir)
        .output()
        .ok()?;
    if !output.status.success() {
        return None;
    }
    let commit = String::from_utf8_lossy(&output.stdout).trim().to_string();
    (!commit.is_empty()).then_some(commit)
}

pub(crate) fn resolved_template_ids(resolved: &ResolvedTemplate) -> Vec<String> {
    match resolved {
        ResolvedTemplate::Builtin(t) => vec![t.id().to_string()],
        ResolvedTemplate::Custom(c) => {
            let mut ids = vec![c.name.clone()];
            let base = c.base.id().to_string();
            if base != c.name {
                ids.push(base);
            }
            ids
        }
    }
}

pub(crate) fn fetch_template(
    resolved: &ResolvedTemplate,
    dest: &Path,
    git_ref: Option<&str>,
) -> Result<()> {
    match resolved {
        ResolvedTemplate::Builtin(template) => {
            clone_template_url(&template.clone_url(), template.id(), dest, git_ref)
        }
        ResolvedTemplate::Custom(custom) => match &custom.source {
            TemplateSource::Git {
                url,
                git_ref: base_ref,
            } => {
                // Explicit --git-ref wins over the registered ref.
                let effective = git_ref.or(base_ref.as_deref());
                clone_template_url(url, &custom.name, dest, effective)
            }
            TemplateSource::Path { path } => copy_template_dir(path, &custom.name, dest),
        },
    }
}

/// Ensure `name` is usable as a directory and a `package.json` name.
///
/// Dots are accepted for plain web projects (npm allows `my.app`) but
/// rejected for the Rust-bearing templates (tauri, fullstack): a `.`
/// would leak into `src-tauri/Cargo.toml` package names, where Cargo
/// forbids it.
pub fn validate_project_name(template: Template, name: &str) -> Result<()> {
    validate_project_name_needs_rust(template.needs_rust(), template.id(), name)
}

/// Same as [`validate_project_name`] but for a resolved (builtin/custom)
/// template, so custom templates inherit their base's Rust rules.
pub fn validate_project_name_resolved(resolved: &ResolvedTemplate, name: &str) -> Result<()> {
    let label = match resolved {
        ResolvedTemplate::Builtin(t) => t.id().to_string(),
        ResolvedTemplate::Custom(c) => format!("{} (base {})", c.name, c.base.id()),
    };
    validate_project_name_needs_rust(resolved.needs_rust(), &label, name)
}

fn validate_project_name_needs_rust(needs_rust: bool, label: &str, name: &str) -> Result<()> {
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
    if needs_rust && name.contains('.') {
        return Err(Error::InvalidName(
            name.to_string(),
            format!(
                "the {label} template ships Rust code (Cargo package names forbid '.'); use '-' or '_' instead",
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
///
/// Exposed so `swag projects add --owner` can validate the value it records
/// (legacy updates use the registry owner for personalization).
pub fn validate_owner(owner: &str) -> Result<()> {
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

/// Resolve the `init` destination: `--output` when given, else the current
/// directory. Unlike [`resolve_destination`], the directory must already
/// exist — `swag init` never creates it.
fn resolve_init_destination(output: Option<&Path>) -> Result<PathBuf> {
    if let Some(dir) = output {
        if !dir.is_dir() {
            return Err(Error::DestinationMissing(dir.display().to_string()));
        }
        return Ok(dir.to_path_buf());
    }
    std::env::current_dir().map_err(|e| Error::Io {
        context: "could not determine current directory".to_string(),
        source: e,
    })
}

/// Remove the destination when `--force` is given, else refuse to overwrite.
///
/// In `init` mode the destination must already exist: `--force` empties it
/// (keeping the directory itself) instead of deleting and recreating it,
/// so `swag init --force` reuses the pre-existing directory.
fn prepare_destination(dest: &Path, force: bool, init: bool) -> Result<()> {
    if !dest.exists() {
        if init {
            return Err(Error::DestinationMissing(dest.display().to_string()));
        }
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
        ensure_destination_is_safe(dest, init)?;
        if init {
            if non_empty {
                eprintln!(
                    "swag: emptying '{}' before scaffolding (--force)",
                    dest.display()
                );
            }
            clear_contents(dest)?;
        } else {
            fs::remove_dir_all(dest).map_err(|e| Error::Io {
                context: format!("could not remove destination '{}'", dest.display()),
                source: e,
            })?;
        }
    }
    Ok(())
}

/// Refuse `--force` targets that would delete the current working directory
/// or one of its ancestors.
///
/// `swag init --force` intentionally empties the current directory (the
/// directory itself is kept), so equality is allowed for init; for `new` even
/// equality is refused because the destination is removed and recreated.
fn ensure_destination_is_safe(dest: &Path, init: bool) -> Result<()> {
    let cwd = std::env::current_dir().map_err(|e| Error::Io {
        context: "could not determine current directory".to_string(),
        source: e,
    })?;
    // Canonicalize both sides the same way (on Windows `canonicalize` adds a
    // verbatim prefix, which would otherwise make the comparison fail).
    let cwd = canonicalize_lenient(&cwd);
    let dest = canonicalize_lenient(dest);
    if destination_is_unsafe(&dest, &cwd, init) {
        return Err(Error::UnsafeDestination(dest.display().to_string()));
    }
    Ok(())
}

fn destination_is_unsafe(dest: &Path, cwd: &Path, init: bool) -> bool {
    if dest == cwd {
        return !init;
    }
    cwd.starts_with(dest)
}

/// Remove every entry inside `dir` while keeping `dir` itself (used by
/// `init --force`). The template fetch (`git clone` into an empty dir, or
/// a local copy) then proceeds exactly like a fresh scaffold.
fn clear_contents(dir: &Path) -> Result<()> {
    let entries = fs::read_dir(dir).map_err(|e| Error::Io {
        context: format!("could not read destination '{}'", dir.display()),
        source: e,
    })?;
    for entry in entries {
        let entry = entry.map_err(|e| Error::Io {
            context: format!("could not read destination '{}'", dir.display()),
            source: e,
        })?;
        let path = entry.path();
        let file_type = entry.file_type().map_err(|e| Error::Io {
            context: format!("could not stat '{}'", path.display()),
            source: e,
        })?;
        let res = if file_type.is_dir() && !file_type.is_symlink() {
            fs::remove_dir_all(&path)
        } else {
            fs::remove_file(&path)
        };
        res.map_err(|e| Error::Io {
            context: format!("could not remove '{}'", path.display()),
            source: e,
        })?;
    }
    Ok(())
}

fn clone_template_url(url: &str, label: &str, dest: &Path, git_ref: Option<&str>) -> Result<()> {
    // Re-validate at use time: a stored custom template may have been edited
    // by hand since registration, and the url/ref are handed straight to
    // `git clone` as arguments. Rejecting a leading '-' stops them being
    // parsed as git options (argument injection).
    custom_templates::validate_git_url(url).map_err(|e| Error::FetchFailed {
        template: label.to_string(),
        reason: e.to_string(),
    })?;
    if let Some(git_ref) = git_ref {
        custom_templates::validate_git_ref(git_ref).map_err(|e| Error::FetchFailed {
            template: label.to_string(),
            reason: e.to_string(),
        })?;
    }
    let mut cmd = Command::new("git");
    cmd.arg("clone").arg("--depth").arg("1");
    if let Some(git_ref) = git_ref {
        cmd.arg("--branch").arg(git_ref);
    }
    cmd.arg("--").arg(url).arg(dest);
    let status = cmd.status().map_err(|e| Error::FetchFailed {
        template: label.to_string(),
        reason: format!("could not spawn git: {e} (is git installed?)"),
    })?;
    if !status.success() {
        return Err(Error::FetchFailed {
            template: label.to_string(),
            reason: format!("`git clone` exited with {status}"),
        });
    }
    Ok(())
}

/// Copy a local template directory into `dest` (skips `.git`, dependency
/// trees, build output and local Vercel link state, like the clone path).
fn copy_template_dir(src: &Path, label: &str, dest: &Path) -> Result<()> {
    let src = src.canonicalize().map_err(|e| Error::FetchFailed {
        template: label.to_string(),
        reason: format!(
            "template source '{}' is not a readable directory: {e}",
            src.display()
        ),
    })?;
    if !src.is_dir() {
        return Err(Error::FetchFailed {
            template: label.to_string(),
            reason: format!("template source '{}' is not a directory", src.display()),
        });
    }
    // Copying a template into a directory inside itself would recurse
    // forever (the destination is discovered by the source walk).
    let dest_abs = canonicalize_lenient(&absolute_path(dest)?);
    if dest_abs == src || dest_abs.starts_with(&src) {
        return Err(Error::FetchFailed {
            template: label.to_string(),
            reason: format!(
                "destination '{}' is inside the template source '{}'",
                dest.display(),
                src.display()
            ),
        });
    }
    fs::create_dir_all(dest).map_err(|e| Error::Io {
        context: format!("could not create '{}'", dest.display()),
        source: e,
    })?;
    let mut stack = vec![src.clone()];
    while let Some(dir) = stack.pop() {
        let rel = dir
            .strip_prefix(&src)
            .map_err(|e| Error::CommandFailed(format!("could not copy template directory: {e}")))?;
        let target_dir = dest.join(rel);
        fs::create_dir_all(&target_dir).map_err(|e| Error::Io {
            context: format!("could not create '{}'", target_dir.display()),
            source: e,
        })?;
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
            let file_type = entry.file_type().map_err(|e| Error::Io {
                context: format!("could not stat '{}'", path.display()),
                source: e,
            })?;
            if file_type.is_symlink() {
                continue;
            }
            let rel = path.strip_prefix(&src).map_err(|e| {
                Error::CommandFailed(format!("could not copy template directory: {e}"))
            })?;
            if rel.components().any(|c| {
                let name = c.as_os_str().to_string_lossy();
                SKIPPED_DIRS.contains(&name.as_ref())
            }) {
                continue;
            }
            let target = dest.join(rel);
            if file_type.is_dir() {
                stack.push(path);
            } else if file_type.is_file() {
                fs::copy(&path, &target).map_err(|e| Error::Io {
                    context: format!("could not copy '{}'", path.display()),
                    source: e,
                })?;
            }
        }
    }
    Ok(())
}

/// Absolute, lexically-normalized form of `path` (does not resolve symlinks).
fn absolute_path(path: &Path) -> Result<PathBuf> {
    let base = if path.is_absolute() {
        path.to_path_buf()
    } else {
        std::env::current_dir()
            .map_err(|e| Error::Io {
                context: "could not determine current directory".to_string(),
                source: e,
            })?
            .join(path)
    };
    let mut out = PathBuf::new();
    for component in base.components() {
        match component {
            std::path::Component::CurDir => {}
            std::path::Component::ParentDir => {
                out.pop();
            }
            other => out.push(other.as_os_str()),
        }
    }
    Ok(out)
}

/// Canonicalize `path` even when it does not exist yet by resolving the
/// deepest existing ancestor and appending the remaining components.
fn canonicalize_lenient(path: &Path) -> PathBuf {
    let mut current = path.to_path_buf();
    let mut tail: Vec<std::ffi::OsString> = Vec::new();
    loop {
        if let Ok(mut resolved) = current.canonicalize() {
            for name in tail.iter().rev() {
                resolved.push(name);
            }
            return resolved;
        }
        let (Some(parent), Some(name)) = (current.parent(), current.file_name()) else {
            return path.to_path_buf();
        };
        tail.push(name.to_os_string());
        current = parent.to_path_buf();
    }
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
/// Rewrite every template-name occurrence under `root` (builtin path kept
/// for tests; prefers [`personalize_ids`] for custom templates).
#[allow(dead_code)]
fn personalize(root: &Path, template: Template, name: &str, owner: Option<&str>) -> Result<()> {
    personalize_ids(root, &[template.id().to_string()], name, owner)
}

/// Rewrite every template-name occurrence under `root`.
///
/// `template_ids` holds every id to rewrite (custom name first, then its base
/// builtin id). Covers the inventoried spots (`package.json` + `bun.lock`
/// names, PWA manifest / SEO site names, `Cargo.toml` name / `default-run` /
/// repository, `tauri.conf.json` productName / identifier, Svelte eyebrows,
/// README links) plus anything future templates add, by walking text files.
/// When `owner` is `Some`, `gi-dellav` → owner everywhere and
/// `gidellav` → owner-without-hyphens (keeps the Tauri identifier legal).
pub(crate) fn personalize_ids(
    root: &Path,
    template_ids: &[String],
    name: &str,
    owner: Option<&str>,
) -> Result<()> {
    let replacements = replacement_table_ids(template_ids, name, owner);
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
#[allow(dead_code)]
fn replacement_table(template: Template, name: &str, owner: Option<&str>) -> Vec<(String, String)> {
    replacement_table_ids(&[template.id().to_string()], name, owner)
}

fn replacement_table_ids(
    template_ids: &[String],
    name: &str,
    owner: Option<&str>,
) -> Vec<(String, String)> {
    let mut table = Vec::new();
    for id in template_ids {
        let snake_from = id.replace('-', "_");
        let snake_to = name.replace('-', "_");
        for pair in [
            (snake_from, snake_to),
            (title_case(id), title_case(name)),
            (id.clone(), name.to_string()),
        ] {
            if !table.contains(&pair) {
                table.push(pair);
            }
        }
    }
    if let Some(owner) = owner.filter(|o| *o != TEMPLATE_OWNER) {
        table.push((TEMPLATE_OWNER_COMPACT.to_string(), owner.replace('-', "")));
        table.push((TEMPLATE_OWNER.to_string(), owner.to_string()));
    }
    table
}

/// Apply all replacements in one left-to-right pass, preferring the longest
/// pattern at each position and never re-scanning inserted replacement text.
///
/// Sequential `str::replace` calls are unsafe here: a short custom template
/// name (e.g. `app`) would rewrite parts of unrelated words and file names,
/// and a custom name that is a prefix of the base id would corrupt the base
/// id before its own longer replacement had a chance to match.
fn apply_replacements(content: &str, replacements: &[(String, String)]) -> String {
    let mut patterns: Vec<&(String, String)> = replacements
        .iter()
        .filter(|(from, _)| !from.is_empty())
        .collect();
    // Longest first so `svelte-clean-template` wins over `svelte-clean`.
    patterns.sort_by(|a, b| b.0.len().cmp(&a.0.len()));
    if patterns.is_empty() {
        return content.to_string();
    }
    let mut out = String::with_capacity(content.len());
    let mut cursor = 0;
    while cursor < content.len() {
        let rest = &content[cursor..];
        let mut best: Option<(usize, &str, &str)> = None;
        for (from, to) in &patterns {
            for (rel, _) in rest.match_indices(from.as_str()) {
                let start = cursor + rel;
                let end = start + from.len();
                if !replacement_boundaries_ok(content, start, end, from) {
                    continue;
                }
                let wins = match best {
                    None => true,
                    Some((best_rel, best_from, _)) => {
                        rel < best_rel || (rel == best_rel && from.len() > best_from.len())
                    }
                };
                if wins {
                    best = Some((rel, from.as_str(), to.as_str()));
                }
                break;
            }
        }
        match best {
            Some((rel, from, to)) => {
                out.push_str(&content[cursor..cursor + rel]);
                out.push_str(to);
                cursor += rel + from.len();
            }
            None => {
                let ch = content[cursor..]
                    .chars()
                    .next()
                    .expect("cursor is inside content");
                out.push(ch);
                cursor += ch.len_utf8();
            }
        }
    }
    out
}

/// Only rewrite complete tokens: `app` must not match `application` or
/// `app.css`, and `svelte-clean-template` must not match a longer
/// `svelte-clean-template-fork`. Multi-word ids, snake/kebab ids and the
/// owner tokens may sit next to dots (URLs, bundle identifiers).
fn replacement_boundaries_ok(content: &str, start: usize, end: usize, pattern: &str) -> bool {
    let allow_dot = pattern.contains('-')
        || pattern.contains('_')
        || pattern.contains(' ')
        || pattern == TEMPLATE_OWNER
        || pattern == TEMPLATE_OWNER_COMPACT;
    let is_bad = |c: char| c.is_alphanumeric() || c == '_' || c == '-' || (!allow_dot && c == '.');
    let before = content[..start].chars().next_back();
    let after = content[end..].chars().next();
    before.is_none_or(|c| !is_bad(c)) && after.is_none_or(|c| !is_bad(c))
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
    use crate::test_env_lock::ENV_LOCK;
    use std::fs;

    /// Isolated `SWAG_HOME` for tests that scaffold (auto-registers).
    /// Returns the previous value; callers must restore + clean up.
    fn isolated_home(tag: &str) -> (PathBuf, Option<String>) {
        let dir = std::env::temp_dir().join(format!(
            "swag-scaffold-test-{tag}-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .expect("time")
                .as_nanos()
        ));
        let old = std::env::var("SWAG_HOME").ok();
        unsafe { std::env::set_var("SWAG_HOME", &dir) };
        (dir, old)
    }

    fn restore_home(home: &Path, old: Option<String>) {
        unsafe {
            match old {
                Some(v) => std::env::set_var("SWAG_HOME", v),
                None => std::env::remove_var("SWAG_HOME"),
            }
        }
        let _ = fs::remove_dir_all(home);
    }

    fn stamp(tag: &str) -> PathBuf {
        std::env::temp_dir().join(format!(
            "swag-scaffold-test-{tag}-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .expect("time")
                .as_nanos()
        ))
    }

    /// Minimal local template tree: personalized name + marker file.
    fn write_template_source(dir: &Path) {
        fs::create_dir_all(dir).unwrap();
        fs::write(
            dir.join("package.json"),
            r#"{"name": "svelte-clean-template"}"#,
        )
        .unwrap();
    }

    fn register_custom(name: &str, src: &Path) {
        let tpl = crate::CustomTemplate::new(
            name,
            Template::SvelteClean,
            "test",
            crate::TemplateSource::Path {
                path: src.to_path_buf(),
            },
        )
        .unwrap();
        crate::custom_templates::add(tpl, true).unwrap();
    }
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

    /// `init` scaffolds into a pre-existing empty directory.
    #[test]
    fn init_uses_preexisting_dir() {
        let _guard = ENV_LOCK.lock().unwrap();
        let (home, old) = isolated_home("init-ok");
        let src = stamp("init-ok-src");
        write_template_source(&src);
        register_custom("tpl-init-ok", &src);

        let dest = stamp("init-ok-dest");
        fs::create_dir_all(&dest).unwrap();
        let options = ScaffoldOptions {
            template: "tpl-init-ok".to_string(),
            name: "demo".to_string(),
            destination: Some(dest.clone()),
            git_ref: None,
            force: false,
            owner: None,
            no_install: true,
            no_git_init: true,
            init: true,
        };
        let project = scaffold(&options).unwrap();
        assert_eq!(project.path, dest);
        assert!(
            fs::read_to_string(dest.join("package.json"))
                .unwrap()
                .contains("demo"),
            "personalization must rewrite the base id"
        );

        restore_home(&home, old);
        let _ = fs::remove_dir_all(&src);
        let _ = fs::remove_dir_all(&dest);
    }

    /// `init` refuses a destination that does not exist yet.
    #[test]
    fn init_refuses_missing_dir() {
        let _guard = ENV_LOCK.lock().unwrap();
        let (home, old) = isolated_home("init-missing");
        let src = stamp("init-missing-src");
        write_template_source(&src);
        register_custom("tpl-init-missing", &src);

        let dest = stamp("init-missing-dest");
        assert!(!dest.exists());
        let options = ScaffoldOptions {
            template: "tpl-init-missing".to_string(),
            name: "demo".to_string(),
            destination: Some(dest.clone()),
            git_ref: None,
            force: false,
            owner: None,
            no_install: true,
            no_git_init: true,
            init: true,
        };
        let err = scaffold(&options).unwrap_err();
        assert!(
            matches!(err, Error::DestinationMissing(_)),
            "expected DestinationMissing, got {err:?}"
        );

        restore_home(&home, old);
        let _ = fs::remove_dir_all(&src);
        let _ = fs::remove_dir_all(&dest);
    }

    /// A plain `new` scaffold into a missing destination keeps working.
    #[test]
    fn new_creates_missing_dir() {
        let _guard = ENV_LOCK.lock().unwrap();
        let (home, old) = isolated_home("new-creates");
        let src = stamp("new-creates-src");
        write_template_source(&src);
        register_custom("tpl-new-creates", &src);

        let dest = stamp("new-creates-dest");
        assert!(!dest.exists());
        let options = ScaffoldOptions {
            template: "tpl-new-creates".to_string(),
            name: "demo".to_string(),
            destination: Some(dest.clone()),
            git_ref: None,
            force: false,
            owner: None,
            no_install: true,
            no_git_init: true,
            init: false,
        };
        scaffold(&options).unwrap();
        assert!(dest.join("package.json").is_file());
        let prov = crate::provenance::load(&dest).unwrap();
        let files = prov.template_files.expect("manifest must be recorded");
        assert!(files.contains(&"package.json".to_string()), "{files:?}");

        restore_home(&home, old);
        let _ = fs::remove_dir_all(&src);
        let _ = fs::remove_dir_all(&dest);
    }

    /// `init --force` empties the pre-existing directory but keeps it.
    #[test]
    fn init_force_clears_but_keeps_dir() {
        let _guard = ENV_LOCK.lock().unwrap();
        let (home, old) = isolated_home("init-force");
        let src = stamp("init-force-src");
        write_template_source(&src);
        register_custom("tpl-init-force", &src);

        let dest = stamp("init-force-dest");
        fs::create_dir_all(&dest).unwrap();
        fs::write(dest.join("stale.txt"), "stale").unwrap();
        let options = ScaffoldOptions {
            template: "tpl-init-force".to_string(),
            name: "demo".to_string(),
            destination: Some(dest.clone()),
            git_ref: None,
            force: true,
            owner: None,
            no_install: true,
            no_git_init: true,
            init: true,
        };
        scaffold(&options).unwrap();
        assert!(dest.is_dir(), "init --force must keep the directory");
        assert!(!dest.join("stale.txt").exists(), "stale files are cleared");
        assert!(dest.join("package.json").is_file());

        restore_home(&home, old);
        let _ = fs::remove_dir_all(&src);
        let _ = fs::remove_dir_all(&dest);
    }

    /// `init` into a non-empty directory without `--force` is refused.
    #[test]
    fn init_refuses_nonempty_without_force() {
        let _guard = ENV_LOCK.lock().unwrap();
        let (home, old) = isolated_home("init-dirty");
        let src = stamp("init-dirty-src");
        write_template_source(&src);
        register_custom("tpl-init-dirty", &src);

        let dest = stamp("init-dirty-dest");
        fs::create_dir_all(&dest).unwrap();
        fs::write(dest.join("stale.txt"), "stale").unwrap();
        let options = ScaffoldOptions {
            template: "tpl-init-dirty".to_string(),
            name: "demo".to_string(),
            destination: Some(dest.clone()),
            git_ref: None,
            force: false,
            owner: None,
            no_install: true,
            no_git_init: true,
            init: true,
        };
        let err = scaffold(&options).unwrap_err();
        assert!(
            matches!(err, Error::DestinationExists(_)),
            "expected DestinationExists, got {err:?}"
        );

        restore_home(&home, old);
        let _ = fs::remove_dir_all(&src);
        let _ = fs::remove_dir_all(&dest);
    }

    #[test]
    fn destination_guard_rejects_cwd_and_ancestors() {
        let base = stamp("guard");
        let child = base.join("child");
        let sibling = base.join("sibling");
        fs::create_dir_all(&child).unwrap();
        fs::create_dir_all(&sibling).unwrap();

        assert!(
            destination_is_unsafe(&base, &child, false),
            "ancestor of cwd"
        );
        assert!(
            destination_is_unsafe(&child, &child, false),
            "cwd itself for new"
        );
        assert!(
            !destination_is_unsafe(&child, &child, true),
            "init --force may empty cwd"
        );
        assert!(
            destination_is_unsafe(&base, &child, true),
            "init must not clear an ancestor of cwd"
        );
        assert!(
            !destination_is_unsafe(&sibling, &child, false),
            "sibling is safe"
        );
        let _ = fs::remove_dir_all(&base);
    }

    #[test]
    fn replacements_are_longest_first_and_token_aware() {
        let table = replacement_table_ids(
            &[
                "svelte-clean".to_string(),
                "svelte-clean-template".to_string(),
            ],
            "demo-app",
            None,
        );
        let content = r#"{"name":"svelte-clean-template"} application app.css svelte-clean"#;
        let out = apply_replacements(content, &table);
        assert!(out.contains(r#""demo-app""#), "{out}");
        assert!(out.contains("demo-app"), "{out}");
        assert!(out.contains("application"), "{out}");
        assert!(out.contains("app.css"), "{out}");
        assert!(!out.contains("svelte-clean"), "{out}");
    }

    #[test]
    fn short_custom_names_do_not_corrupt_words_or_extensions() {
        let table = replacement_table_ids(&["app".to_string()], "demo", None);
        let out = apply_replacements("application app.css \"app\" App", &table);
        assert!(out.contains("application"), "{out}");
        assert!(out.contains("app.css"), "{out}");
        assert!(out.contains("\"demo\""), "{out}");
        assert!(out.contains("Demo"), "{out}");
    }

    #[test]
    fn owner_replacement_still_works_in_domains_and_identifiers() {
        let table = replacement_table_ids(
            &[Template::SvelteClean.id().to_string()],
            "demo",
            Some("octo-cat"),
        );
        let out = apply_replacements(
            "https://gi-dellav.github.io/x com.gidellav.svelte_clean_template",
            &table,
        );
        assert!(out.contains("https://octo-cat.github.io/x"), "{out}");
        assert!(out.contains("com.octocat.demo"), "{out}");
    }

    #[test]
    fn copy_template_dir_skips_build_artifacts_and_vercel_state() {
        let src = stamp("copy-src");
        let dest = stamp("copy-dest");
        fs::create_dir_all(src.join("src")).unwrap();
        for skipped in ["node_modules", "target", "dist", ".vercel"] {
            fs::create_dir_all(src.join(skipped)).unwrap();
            fs::write(src.join(skipped).join("artifact.txt"), "x").unwrap();
        }
        fs::write(src.join("package.json"), "{}").unwrap();
        fs::write(src.join("src").join("keep.txt"), "keep").unwrap();

        copy_template_dir(&src, "tpl", &dest).unwrap();

        assert!(dest.join("package.json").is_file());
        assert!(dest.join("src").join("keep.txt").is_file());
        for skipped in ["node_modules", "target", "dist", ".vercel"] {
            assert!(!dest.join(skipped).exists(), "{skipped} must be skipped");
        }
        let _ = fs::remove_dir_all(&src);
        let _ = fs::remove_dir_all(&dest);
    }

    #[test]
    fn copy_template_dir_refuses_destination_inside_source() {
        let src = stamp("copy-self-src");
        fs::create_dir_all(&src).unwrap();
        fs::write(src.join("package.json"), "{}").unwrap();
        let dest = src.join("out");

        let err = copy_template_dir(&src, "tpl", &dest).unwrap_err();
        assert!(
            err.to_string().contains("inside the template source"),
            "{err}"
        );
        assert!(!dest.exists(), "must refuse before creating anything");
        let _ = fs::remove_dir_all(&src);
    }

    #[test]
    fn head_commit_ignores_enclosing_repository() {
        if Command::new("git").arg("--version").output().is_err() {
            return;
        }
        let repo = stamp("head-repo");
        let nested = repo.join("nested");
        fs::create_dir_all(&nested).unwrap();
        assert!(
            Command::new("git")
                .args(["init", "--quiet"])
                .current_dir(&repo)
                .status()
                .unwrap()
                .success()
        );
        assert!(
            Command::new("git")
                .args([
                    "-c",
                    "user.email=test@example.com",
                    "-c",
                    "user.name=Test",
                    "commit",
                    "--allow-empty",
                    "-m",
                    "init",
                ])
                .current_dir(&repo)
                .output()
                .unwrap()
                .status
                .success()
        );
        assert!(head_commit(&repo).is_some());
        assert!(
            head_commit(&nested).is_none(),
            "nested dir must not inherit the parent repo's HEAD"
        );
        let _ = fs::remove_dir_all(&repo);
    }
}
