//! `swag update`: bring a scaffolded project up to the template HEAD.
//!
//! Deterministic and agent-free: the project directory alone tells us what
//! the project is (`.swag.json` provenance, falling back to
//! `.template_version` / `.template_type` plus the project registry for
//! legacy projects). The fresh template is fetched to a temp dir,
//! re-personalized with the recorded name/owner, and diffed file-by-file
//! against the project. The content diff is the ground truth for
//! up-to-dateness; `.template_version` is reported for information only,
//! so content shipped without a version bump is still detected.
//!
//! Safety: `--check` / `--dry-run` never touch the project; apply refuses
//! on a dirty git tree unless `--force`; removed upstream files are only
//! deleted with `--prune` (otherwise reported for manual handling).

use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

use serde::{Deserialize, Serialize};

use crate::error::{Error, Result};
use crate::provenance::{self, Provenance};

/// Directories never compared or copied (build artefacts, VCS metadata,
/// local Vercel link state).
const SKIPPED_DIRS: [&str; 6] = [
    ".git",
    "node_modules",
    "target",
    "dist",
    ".swag-tmp",
    ".vercel",
];
/// Files managed by swag itself, excluded from the content diff.
const SKIPPED_FILES: [&str; 1] = [".swag.json"];
/// Largest file compared byte-for-byte (binary-asset heuristic, mirrors scaffold).
const MAX_DIFF_BYTES: u64 = 512 * 1024;

/// Options controlling [`update`].
#[derive(Debug, Clone)]
pub struct UpdateOptions {
    /// Project directory (defaults to cwd).
    pub dir: PathBuf,
    /// Only report; implies no filesystem changes.
    pub check_only: bool,
    /// Show what would change without applying.
    pub dry_run: bool,
    /// Apply even with uncommitted git changes.
    pub force: bool,
    /// Delete files removed upstream (default: report only).
    pub prune: bool,
    /// Branch / tag overriding the registered ref for git templates.
    pub git_ref: Option<String>,
}

/// Outcome of [`update`].
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UpdateReport {
    /// Project directory.
    pub path: PathBuf,
    /// Template the project was created from.
    pub template: String,
    /// Version recorded in the project (`.template_version`).
    pub local_version: Option<u64>,
    /// Version of the freshly fetched template.
    pub upstream_version: Option<u64>,
    /// True when the project content matches the fresh personalized template
    /// (the version numbers are reported for information only).
    pub up_to_date: bool,
    /// Files differing between project and fresh personalized template.
    pub changed: Vec<String>,
    /// Files present upstream but missing in the project.
    pub added: Vec<String>,
    /// Files present in the project but gone upstream.
    pub removed: Vec<String>,
    /// Whether files were actually written.
    pub applied: bool,
}

/// Resolve a CLI target: registry name wins, otherwise a filesystem path.
pub fn resolve_target(target: &str) -> Result<PathBuf> {
    if let Ok(entry) = crate::projects::get(target) {
        return Ok(entry.path);
    }
    let path = PathBuf::from(target);
    if path.exists() {
        return Ok(path);
    }
    Err(Error::NotAProject(format!(
        "no registered project '{target}' and path '{}' does not exist",
        path.display()
    )))
}

/// Check for updates and optionally apply them.
pub fn update(options: &UpdateOptions) -> Result<UpdateReport> {
    let dir = options.dir.clone();
    if !dir.is_dir() {
        return Err(Error::NotAProject(format!(
            "'{}' is not a directory",
            dir.display()
        )));
    }
    let provenance = load_or_infer(&dir)?;
    let resolved = crate::custom_templates::resolve_id(&provenance.template).map_err(|_| {
        Error::NotAProject(format!(
            "project template '{}' is unknown (see `swag templates`)",
            provenance.template
        ))
    })?;
    let local_version = crate::template::read_template_version(&dir);

    // Fetch + personalize once; the temp dir is reused for the copy step so
    // the template is cloned a single time per `update` run.
    let tmp = fresh_temp_dir()?;
    let outcome = (|| -> Result<UpdateReport> {
        let git_ref = options.git_ref.as_deref();
        crate::project::fetch_template(&resolved, &tmp, git_ref)?;
        let ids = crate::project::resolved_template_ids(&resolved);
        crate::project::personalize_ids(&tmp, &ids, &provenance.name, provenance.owner.as_deref())?;
        let upstream_version = crate::template::read_template_version(&tmp);
        let upstream_commit = crate::project::head_commit(&tmp);
        let (changed, added, removed) = diff_trees(&tmp, &dir)?;

        // The content diff is ground truth: a clean diff means up to date
        // even when `.template_version` was not bumped. Version numbers are
        // reported for information only.
        let up_to_date = changed.is_empty() && added.is_empty() && removed.is_empty();

        let read_only = options.check_only || options.dry_run;
        if read_only || up_to_date {
            return Ok(UpdateReport {
                path: dir.clone(),
                template: provenance.template.clone(),
                local_version,
                upstream_version,
                up_to_date,
                changed,
                added,
                removed,
                applied: false,
            });
        }

        ensure_clean_tree(&dir, options.force)?;

        for rel in changed.iter().chain(added.iter()) {
            let src = tmp.join(rel);
            let dst = dir.join(rel);
            if let Some(parent) = dst.parent() {
                fs::create_dir_all(parent).map_err(|e| Error::Io {
                    context: format!("could not create '{}'", parent.display()),
                    source: e,
                })?;
            }
            fs::copy(&src, &dst).map_err(|e| Error::Io {
                context: format!("could not update '{}'", dst.display()),
                source: e,
            })?;
        }
        if options.prune {
            for rel in &removed {
                let dst = dir.join(rel);
                if dst.is_file() {
                    fs::remove_file(&dst).map_err(|e| Error::Io {
                        context: format!("could not remove '{}'", dst.display()),
                        source: e,
                    })?;
                }
            }
        }
        sync_version_file(&tmp, &dir)?;
        let mut updated = Provenance::new(
            &provenance.name,
            &provenance.template,
            provenance.base.as_deref(),
            crate::template::read_template_version(&dir),
            crate::template::read_template_type(&dir),
            provenance.owner.as_deref(),
        );
        // Pin the freshly fetched revision, falling back to the recorded one.
        updated.commit = upstream_commit.or_else(|| provenance.commit.clone());
        provenance::save(&dir, &updated)?;

        Ok(UpdateReport {
            path: dir.clone(),
            template: provenance.template.clone(),
            local_version,
            upstream_version,
            up_to_date: false,
            changed,
            added,
            removed,
            applied: true,
        })
    })();
    let _ = fs::remove_dir_all(&tmp);
    outcome
}

/// Load `.swag.json`, or infer provenance for legacy projects from the
/// version/type files plus a reverse registry lookup by path.
fn load_or_infer(dir: &Path) -> Result<Provenance> {
    if let Ok(prov) = provenance::load(dir) {
        return Ok(prov);
    }
    let version = crate::template::read_template_version(dir);
    let template_type = crate::template::read_template_type(dir);
    let name = dir
        .file_name()
        .map(|s| s.to_string_lossy().into_owned())
        .unwrap_or_else(|| "project".to_string());
    let canonical = dir.canonicalize().unwrap_or_else(|_| dir.to_path_buf());
    if let Ok(entries) = crate::projects::list() {
        for entry in entries {
            let entry_canonical = entry
                .path
                .canonicalize()
                .unwrap_or_else(|_| entry.path.clone());
            if entry_canonical == canonical {
                return Ok(Provenance::new(
                    &entry.name,
                    &entry.template,
                    None,
                    version,
                    template_type,
                    entry.owner.as_deref(),
                ));
            }
        }
    }
    if version.is_none() && template_type.is_none() {
        return Err(Error::NotAProject(format!(
            "no {} in '{}' and no registry entry matches",
            crate::provenance::PROVENANCE_FILE,
            dir.display()
        )));
    }
    Ok(Provenance::new(
        &name,
        infer_template_id(template_type),
        None,
        version,
        template_type,
        None,
    ))
}

/// Best-effort template guess for legacy projects without provenance:
/// map the type file back to its canonical builtin.
fn infer_template_id(template_type: Option<crate::template::TemplateType>) -> &'static str {
    use crate::template::TemplateType as T;
    match template_type {
        Some(T::Desktop) => "svelte-tauri-template",
        Some(T::Backend) => "svelte-fullstack-template",
        _ => "svelte-clean-template",
    }
}

fn fresh_temp_dir() -> Result<PathBuf> {
    let dir = std::env::temp_dir().join(format!(
        "swag-update-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_nanos())
            .unwrap_or(0)
    ));
    fs::create_dir_all(&dir).map_err(|e| Error::Io {
        context: format!("could not create temp dir '{}'", dir.display()),
        source: e,
    })?;
    Ok(dir)
}

fn ensure_clean_tree(dir: &Path, force: bool) -> Result<()> {
    if force {
        return Ok(());
    }
    let output = Command::new("git")
        .args(["status", "--porcelain"])
        .current_dir(dir)
        .output();
    let Ok(output) = output else { return Ok(()) };
    if !output.status.success() {
        return Ok(());
    }
    if String::from_utf8_lossy(&output.stdout).trim().is_empty() {
        Ok(())
    } else {
        Err(Error::DirtyTree(format!(
            "'{}' has uncommitted changes (use --force to overwrite)",
            dir.display()
        )))
    }
}

/// Copy `.template_version` / `.template_type` from the fresh template so the
/// project advertises its new version (personalization never touches them).
fn sync_version_file(tmp: &Path, dir: &Path) -> Result<()> {
    for file in [
        crate::template::TEMPLATE_VERSION_FILE,
        crate::template::TEMPLATE_TYPE_FILE,
    ] {
        let src = tmp.join(file);
        if src.is_file() {
            fs::copy(&src, dir.join(file)).map_err(|e| Error::Io {
                context: format!("could not sync '{file}'"),
                source: e,
            })?;
        }
    }
    Ok(())
}

fn snapshot_tree(root: &Path) -> Result<BTreeMap<String, Vec<u8>>> {
    let mut out = BTreeMap::new();
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
            let file_type = entry.file_type().map_err(|e| Error::Io {
                context: format!("could not stat '{}'", path.display()),
                source: e,
            })?;
            if file_type.is_symlink() {
                continue;
            }
            let rel = path
                .strip_prefix(root)
                .map_err(|e| Error::Internal(format!("could not diff template tree: {e}")))?;
            if rel.components().any(|c| {
                let s = c.as_os_str().to_string_lossy();
                SKIPPED_DIRS.contains(&s.as_ref())
            }) {
                continue;
            }
            if file_type.is_dir() {
                stack.push(path);
                continue;
            }
            if !file_type.is_file() {
                continue;
            }
            if let Some(name) = rel.file_name().and_then(|s| s.to_str())
                && SKIPPED_FILES.contains(&name)
            {
                continue;
            }
            let bytes = fs::read(&path).map_err(|e| Error::Io {
                context: format!("could not read '{}'", path.display()),
                source: e,
            })?;
            if bytes.len() as u64 > MAX_DIFF_BYTES {
                continue;
            }
            if let Some(rel_str) = rel.to_str() {
                out.insert(rel_str.to_string(), bytes);
            }
        }
    }
    Ok(out)
}

fn diff_trees(fresh: &Path, project: &Path) -> Result<(Vec<String>, Vec<String>, Vec<String>)> {
    let fresh_map = snapshot_tree(fresh)?;
    let project_map = snapshot_tree(project)?;
    let fresh_keys: BTreeSet<&String> = fresh_map.keys().collect();
    let project_keys: BTreeSet<&String> = project_map.keys().collect();
    let mut changed = Vec::new();
    let mut added = Vec::new();
    let mut removed = Vec::new();
    for key in fresh_keys.intersection(&project_keys) {
        if fresh_map[*key] != project_map[*key] {
            changed.push((*key).clone());
        }
    }
    for key in fresh_keys.difference(&project_keys) {
        // Files that only exist to pin the template itself are synced
        // silently, not reported as additions.
        if *key == crate::template::TEMPLATE_VERSION_FILE
            || *key == crate::template::TEMPLATE_TYPE_FILE
        {
            continue;
        }
        added.push((*key).clone());
    }
    for key in project_keys.difference(&fresh_keys) {
        removed.push((*key).clone());
    }
    Ok((changed, added, removed))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_env_lock::ENV_LOCK;

    fn stamp(tag: &str) -> PathBuf {
        std::env::temp_dir().join(format!(
            "swag-update-test-{tag}-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .expect("time")
                .as_nanos()
        ))
    }

    fn write_template_source(dir: &Path, version: &str, extra: &str) {
        fs::create_dir_all(dir).unwrap();
        fs::write(dir.join("package.json"), r#"{"name": "demo"}"#).unwrap();
        fs::write(dir.join(".template_version"), version).unwrap();
        fs::write(dir.join(".template_type"), "frontend\n").unwrap();
        fs::write(dir.join("app.txt"), extra).unwrap();
    }

    fn register_custom(name: &str, src: &Path) {
        let tpl = crate::CustomTemplate::new(
            name,
            crate::Template::SvelteClean,
            "test",
            crate::TemplateSource::Path {
                path: src.to_path_buf(),
            },
        )
        .unwrap();
        crate::custom_templates::add(tpl, true).unwrap();
    }

    fn scaffold_like(dest: &Path, src: &Path) {
        fs::create_dir_all(dest).unwrap();
        for entry in fs::read_dir(src).unwrap() {
            let entry = entry.unwrap();
            if entry.file_type().unwrap().is_file() {
                fs::copy(entry.path(), dest.join(entry.file_name())).unwrap();
            }
        }
    }

    #[test]
    fn up_to_date_when_versions_match() {
        let _guard = ENV_LOCK.lock().unwrap();
        let old = std::env::var("SWAG_HOME").ok();
        let home = stamp("home1");
        unsafe { std::env::set_var("SWAG_HOME", &home) };

        let src = stamp("src1");
        write_template_source(&src, "21\n", "v1");
        register_custom("tpl-upd-1", &src);

        let dest = stamp("dest1");
        let options = crate::ScaffoldOptions {
            template: "tpl-upd-1".to_string(),
            name: "demo".to_string(),
            destination: Some(dest.clone()),
            git_ref: None,
            force: false,
            owner: None,
            no_install: true,
            no_git_init: true,
            init: false,
        };
        crate::project::scaffold(&options).unwrap();

        let report = update(&UpdateOptions {
            dir: dest.clone(),
            check_only: true,
            dry_run: false,
            force: false,
            prune: false,
            git_ref: None,
        })
        .unwrap();
        assert!(report.up_to_date, "{report:?}");
        assert_eq!(report.local_version, Some(21));
        assert_eq!(report.upstream_version, Some(21));

        unsafe {
            match old {
                Some(v) => std::env::set_var("SWAG_HOME", v),
                None => std::env::remove_var("SWAG_HOME"),
            }
        }
        let _ = fs::remove_dir_all(&home);
        let _ = fs::remove_dir_all(&src);
        let _ = fs::remove_dir_all(&dest);
    }

    #[test]
    fn detects_and_applies_upstream_bump() {
        let _guard = ENV_LOCK.lock().unwrap();
        let old = std::env::var("SWAG_HOME").ok();
        let home = stamp("home2");
        unsafe { std::env::set_var("SWAG_HOME", &home) };

        let src = stamp("src2");
        write_template_source(&src, "21\n", "v1");
        register_custom("tpl-upd-2", &src);

        let dest = stamp("dest2");
        let options = crate::ScaffoldOptions {
            template: "tpl-upd-2".to_string(),
            name: "demo".to_string(),
            destination: Some(dest.clone()),
            git_ref: None,
            force: false,
            owner: None,
            no_install: true,
            no_git_init: true,
            init: false,
        };
        crate::project::scaffold(&options).unwrap();

        fs::write(src.join("app.txt"), "v2").unwrap();
        fs::write(src.join(".template_version"), "22\n").unwrap();

        let check = update(&UpdateOptions {
            dir: dest.clone(),
            check_only: true,
            dry_run: false,
            force: false,
            prune: false,
            git_ref: None,
        })
        .unwrap();
        assert!(!check.up_to_date, "{check:?}");
        assert_eq!(check.local_version, Some(21));
        assert_eq!(check.upstream_version, Some(22));
        assert!(check.changed.contains(&"app.txt".to_string()), "{check:?}");
        assert!(!check.applied);

        let applied = update(&UpdateOptions {
            dir: dest.clone(),
            check_only: false,
            dry_run: false,
            force: true,
            prune: false,
            git_ref: None,
        })
        .unwrap();
        assert!(applied.applied);
        assert_eq!(fs::read_to_string(dest.join("app.txt")).unwrap(), "v2");
        assert_eq!(
            fs::read_to_string(dest.join(".template_version")).unwrap(),
            "22\n"
        );

        let again = update(&UpdateOptions {
            dir: dest.clone(),
            check_only: true,
            dry_run: false,
            force: false,
            prune: false,
            git_ref: None,
        })
        .unwrap();
        assert!(again.up_to_date, "{again:?}");

        unsafe {
            match old {
                Some(v) => std::env::set_var("SWAG_HOME", v),
                None => std::env::remove_var("SWAG_HOME"),
            }
        }
        let _ = fs::remove_dir_all(&home);
        let _ = fs::remove_dir_all(&src);
        let _ = fs::remove_dir_all(&dest);
    }

    #[test]
    fn refuses_unknown_directory() {
        let missing = stamp("missing");
        let err = update(&UpdateOptions {
            dir: missing,
            check_only: true,
            dry_run: false,
            force: false,
            prune: false,
            git_ref: None,
        })
        .unwrap_err();
        assert!(matches!(err, Error::NotAProject(_)), "{err:?}");
    }

    #[test]
    fn dirty_tree_guard() {
        assert!(ensure_clean_tree(&stamp("no-repo-xyz"), false).is_ok());
    }

    #[test]
    fn scaffold_like_helper_covers_copy_path() {
        let src = stamp("helper-src");
        let dest = stamp("helper-dest");
        write_template_source(&src, "7\n", "x");
        scaffold_like(&dest, &src);
        assert_eq!(crate::template::read_template_version(&dest), Some(7));
        let _ = fs::remove_dir_all(&src);
        let _ = fs::remove_dir_all(&dest);
    }

    /// Content shipped without a `.template_version` bump must still be
    /// detected and applied (the diff is the ground truth).
    #[test]
    fn detects_content_change_without_version_bump() {
        let _guard = ENV_LOCK.lock().unwrap();
        let old = std::env::var("SWAG_HOME").ok();
        let home = stamp("home3");
        unsafe { std::env::set_var("SWAG_HOME", &home) };

        let src = stamp("src3");
        write_template_source(&src, "21\n", "v1");
        register_custom("tpl-upd-3", &src);

        let dest = stamp("dest3");
        let options = crate::ScaffoldOptions {
            template: "tpl-upd-3".to_string(),
            name: "demo".to_string(),
            destination: Some(dest.clone()),
            git_ref: None,
            force: false,
            owner: None,
            no_install: true,
            no_git_init: true,
            init: false,
        };
        crate::project::scaffold(&options).unwrap();

        // Change content, keep `.template_version` at 21.
        fs::write(src.join("app.txt"), "v1-fixed").unwrap();

        let check = update(&UpdateOptions {
            dir: dest.clone(),
            check_only: true,
            dry_run: false,
            force: false,
            prune: false,
            git_ref: None,
        })
        .unwrap();
        assert!(
            !check.up_to_date,
            "content change without a version bump must be detected: {check:?}"
        );
        assert_eq!(check.local_version, Some(21));
        assert_eq!(check.upstream_version, Some(21));
        assert!(check.changed.contains(&"app.txt".to_string()), "{check:?}");

        unsafe {
            match old {
                Some(v) => std::env::set_var("SWAG_HOME", v),
                None => std::env::remove_var("SWAG_HOME"),
            }
        }
        let _ = fs::remove_dir_all(&home);
        let _ = fs::remove_dir_all(&src);
        let _ = fs::remove_dir_all(&dest);
    }
}
