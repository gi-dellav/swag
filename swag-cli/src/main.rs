//! `swag` — static webapp generator.
//!
//! Thin clap-derive CLI over [`swag_corelib`]. All logic lives in the
//! library; this binary only parses arguments and reports results.

use std::path::{Path, PathBuf};

use clap::{Parser, Subcommand};

use swag_corelib::{Error, Project, ScaffoldOptions, Template, project};

/// Static webapp generator: scaffold bun-managed Svelte projects from templates.
#[derive(Debug, Parser)]
#[command(
    name = "swag",
    version,
    about = "Static webapp generator for the agentic era"
)]
struct Cli {
    /// Subcommand to run.
    #[command(subcommand)]
    command: Command,
}

#[derive(Debug, Subcommand)]
enum Command {
    /// Scaffold a new project from a template (git clone + personalize + bun install).
    New {
        /// Template id: svelte-clean-template (clean), svelte-tauri-template (tauri),
        /// svelte-fullstack-template (fullstack), or a custom `swag templates add` name.
        #[arg(short, long, default_value = "svelte-clean-template")]
        template: String,

        /// Project name (directory + package name).
        name: String,

        /// Destination directory (defaults to <cwd>/<name>).
        #[arg(short, long)]
        output: Option<PathBuf>,

        /// Clone a branch or tag instead of the default branch.
        /// (For custom git templates: wins over the registered ref.)
        #[arg(long)]
        git_ref: Option<String>,

        /// GitHub owner replacing `gi-dellav` in repo slugs, URLs, Pages
        /// domains and the Tauri `com.gidellav.*` bundle identifier.
        /// Also read from the SWAG_GITHUB_OWNER environment variable.
        #[arg(long, env = "SWAG_GITHUB_OWNER")]
        owner: Option<String>,

        /// Remove the destination first if it already exists.
        #[arg(short, long)]
        force: bool,

        /// Skip `bun install` after scaffolding.
        #[arg(long)]
        no_install: bool,

        /// Skip the fresh `git init` after detaching template history.
        #[arg(long)]
        no_git_init: bool,
    },

    /// List the available templates (built-in + custom).
    #[command(alias = "list-templates")]
    List {
        /// Print the catalogue as JSON.
        #[arg(long)]
        json: bool,
    },

    /// Manage registered projects in ~/.swag/projects.json.
    Projects {
        /// Subcommand; defaults to `list` when omitted.
        #[command(subcommand)]
        command: Option<ProjectsCommand>,

        /// Print the project list as JSON (only without a subcommand).
        #[arg(long)]
        json: bool,
    },

    /// Manage custom templates in ~/.swag/templates.json.
    Templates {
        /// Subcommand; defaults to `list` when omitted.
        #[command(subcommand)]
        command: Option<TemplatesCommand>,

        /// Print the template list as JSON (only without a subcommand).
        #[arg(long)]
        json: bool,
    },

    /// Open a subshell in a registered project's directory.
    ///
    /// A child process cannot change the caller's working directory, so
    /// `swag cd` spawns an interactive shell with the project dir as CWD;
    /// `exit` returns to where you were. Use `--print` for scripting.
    Cd {
        /// Registered project name (see `swag projects list`).
        name: String,

        /// Print the project path instead of spawning a subshell.
        #[arg(long)]
        print: bool,
    },

    /// Run `bun install` in a project directory.
    Install {
        /// Project directory (defaults to the current directory).
        #[arg(default_value = ".")]
        dir: PathBuf,
    },

    /// Run the dev server (`bun run dev`) in a project directory.
    Dev {
        /// Project directory (defaults to the current directory).
        #[arg(default_value = ".")]
        dir: PathBuf,

        /// Extra arguments forwarded after `--` to the bun script.
        #[arg(last = true)]
        extra: Vec<String>,
    },

    /// Production build (`bun run build`) in a project directory.
    Build {
        /// Project directory (defaults to the current directory).
        #[arg(default_value = ".")]
        dir: PathBuf,

        /// Extra arguments forwarded after `--` to the bun script.
        #[arg(last = true)]
        extra: Vec<String>,
    },

    /// Type-check (`bun run check`) in a project directory.
    Check {
        /// Project directory (defaults to the current directory).
        #[arg(default_value = ".")]
        dir: PathBuf,

        /// Extra arguments forwarded after `--` to the bun script.
        #[arg(last = true)]
        extra: Vec<String>,
    },

    /// Unit tests (`bun run test`) in a project directory.
    Test {
        /// Project directory (defaults to the current directory).
        #[arg(default_value = ".")]
        dir: PathBuf,

        /// Extra arguments forwarded after `--` to the bun script.
        #[arg(last = true)]
        extra: Vec<String>,
    },

    /// Preview the production build (`bun run preview`) in a project directory.
    Preview {
        /// Project directory (defaults to the current directory).
        #[arg(default_value = ".")]
        dir: PathBuf,

        /// Extra arguments forwarded after `--` to the bun script.
        #[arg(last = true)]
        extra: Vec<String>,
    },
}

#[derive(Debug, Subcommand)]
enum ProjectsCommand {
    /// List registered projects (default).
    List {
        /// Print as JSON.
        #[arg(long)]
        json: bool,
    },
    /// Register a project directory.
    Add {
        /// Registry name for the project.
        name: String,

        /// Project directory (defaults to the current directory).
        #[arg(long, default_value = ".")]
        path: PathBuf,

        /// Template id recorded for the entry (informational).
        #[arg(long, default_value = "svelte-clean-template")]
        template: String,

        /// GitHub owner recorded for the entry (informational).
        #[arg(long)]
        owner: Option<String>,
    },
    /// Remove a project from the registry (leaves the directory alone).
    Remove {
        /// Registered project name.
        name: String,
    },
    /// Show one registered project.
    Show {
        /// Registered project name.
        name: String,

        /// Print as JSON.
        #[arg(long)]
        json: bool,
    },
    /// Drop registry entries whose directories no longer exist.
    Prune {
        /// List what would be pruned without changing the registry.
        #[arg(long)]
        dry_run: bool,
    },
}

#[derive(Debug, Subcommand)]
enum TemplatesCommand {
    /// List custom + built-in templates (default).
    List {
        /// Print as JSON.
        #[arg(long)]
        json: bool,
    },
    /// Register a custom template deriving from a built-in.
    Add {
        /// Registry name (lowercase, must not collide with built-ins).
        name: String,

        /// Built-in this template derives from (personalization + Rust rules).
        #[arg(long)]
        base: String,

        /// Git URL to clone at scaffold time.
        #[arg(long, conflicts_with = "path")]
        git: Option<String>,

        /// Branch or tag for `--git` (defaults to the remote default branch).
        #[arg(long, requires = "git")]
        git_ref: Option<String>,

        /// Local directory to copy at scaffold time.
        #[arg(long, conflicts_with = "git")]
        path: Option<PathBuf>,

        /// One-line description.
        #[arg(long, default_value = "")]
        description: String,

        /// Overwrite an existing entry with the same name.
        #[arg(long)]
        force: bool,
    },
    /// Remove a custom template (built-ins cannot be removed).
    Remove {
        /// Custom template name.
        name: String,
    },
    /// Show one template (built-in or custom).
    Show {
        /// Template id or custom name.
        name: String,

        /// Print as JSON.
        #[arg(long)]
        json: bool,
    },
}

fn main() {
    if let Err(err) = run() {
        eprintln!("swag: {err}");
        std::process::exit(1);
    }
}

fn run() -> Result<(), Error> {
    let cli = Cli::parse();
    match cli.command {
        Command::New {
            template,
            name,
            output,
            git_ref,
            owner,
            force,
            no_install,
            no_git_init,
        } => {
            let options = ScaffoldOptions {
                template,
                name,
                destination: output,
                git_ref,
                owner,
                force,
                no_install,
                no_git_init,
            };
            let project = project::scaffold(&options)?;
            println!(
                "created {} from {} in {}",
                project.name,
                project.template,
                project.path.display()
            );
            println!("registered '{}' in ~/.swag/projects.json", project.name);
            if no_install {
                println!("skipped `bun install` — run `bun install` in the project dir");
            } else {
                println!("installed dependencies with `bun install`");
            }
        }
        Command::List { json } => {
            print_templates(json)?;
        }
        Command::Projects { command, json } => match command {
            None => print_projects(json)?,
            Some(ProjectsCommand::List { json: inner }) => print_projects(inner)?,
            Some(ProjectsCommand::Add {
                name,
                path,
                template,
                owner,
            }) => {
                // Validate the registry name with the most permissive rules;
                // re-check Rust-bearing templates for dots below.
                project::validate_project_name(Template::SvelteClean, &name).map_err(|_| {
                    Error::InvalidName(
                        name.clone(),
                        "use letters, digits, '-', '_' or '.'".to_string(),
                    )
                })?;
                if let Ok(resolved) = swag_corelib::custom_templates::resolve_id(&template) {
                    project::validate_project_name_resolved(&resolved, &name)?;
                }
                let previous =
                    swag_corelib::projects::upsert(&name, &path, &template, owner.as_deref())?;
                let entry = swag_corelib::projects::get(&name)?;
                if previous.is_some() {
                    println!("updated project '{}' → {}", name, entry.path.display());
                } else {
                    println!("added project '{}' → {}", name, entry.path.display());
                }
            }
            Some(ProjectsCommand::Remove { name }) => {
                swag_corelib::projects::remove(&name)?;
                println!("removed project '{name}'");
            }
            Some(ProjectsCommand::Show { name, json }) => {
                let entry = swag_corelib::projects::get(&name)?;
                if json {
                    let rendered = serde_json::to_string_pretty(&entry)
                        .map_err(|e| Error::CommandFailed(format!("could not render JSON: {e}")))?;
                    println!("{rendered}");
                } else {
                    print_project_human(&entry);
                }
            }
            Some(ProjectsCommand::Prune { dry_run }) => {
                if dry_run {
                    let stale: Vec<_> = swag_corelib::projects::list()?
                        .into_iter()
                        .filter(|e| !e.path.exists())
                        .collect();
                    if stale.is_empty() {
                        println!("nothing to prune");
                    } else {
                        for e in &stale {
                            println!("would prune {} ({})", e.name, e.path.display());
                        }
                    }
                } else {
                    let pruned = swag_corelib::projects::prune()?;
                    if pruned.is_empty() {
                        println!("nothing to prune");
                    } else {
                        for e in &pruned {
                            println!("pruned {} ({})", e.name, e.path.display());
                        }
                    }
                }
            }
        },
        Command::Templates { command, json } => match command {
            None => print_templates(json)?,
            Some(TemplatesCommand::List { json: inner }) => print_templates(inner)?,
            Some(TemplatesCommand::Add {
                name,
                base,
                git,
                git_ref,
                path,
                description,
                force,
            }) => {
                let base = Template::parse_id(&base)?;
                let source = match (git, path) {
                    (Some(url), None) => swag_corelib::TemplateSource::Git { url, git_ref },
                    (None, Some(path)) => swag_corelib::TemplateSource::Path { path },
                    _ => {
                        return Err(Error::InvalidSource(
                            name.clone(),
                            "pass exactly one of --git <url> or --path <dir>".to_string(),
                        ));
                    }
                };
                let template =
                    swag_corelib::CustomTemplate::new(&name, base, &description, source)?;
                swag_corelib::custom_templates::add(template, force)?;
                println!("added template '{name}'");
            }
            Some(TemplatesCommand::Remove { name }) => {
                // Built-ins are not in the custom registry: refuse with the
                // same UnknownTemplate error `get` produces.
                if Template::parse_id(&name).is_ok() {
                    return Err(Error::CommandFailed(format!(
                        "cannot remove built-in template '{name}'"
                    )));
                }
                swag_corelib::custom_templates::remove(&name)?;
                println!("removed template '{name}'");
            }
            Some(TemplatesCommand::Show { name, json }) => {
                if json {
                    if let Ok(custom) = swag_corelib::custom_templates::get(&name) {
                        let rendered = serde_json::to_string_pretty(&custom).map_err(|e| {
                            Error::CommandFailed(format!("could not render JSON: {e}"))
                        })?;
                        println!("{rendered}");
                    } else {
                        let builtin = Template::parse_id(&name)?;
                        let info = project::TemplateInfo {
                            id: builtin.id().to_string(),
                            description: builtin.description().to_string(),
                            clone_url: builtin.clone_url(),
                            needs_rust: builtin.needs_rust(),
                            builtin: true,
                            base: None,
                        };
                        let rendered = serde_json::to_string_pretty(&info).map_err(|e| {
                            Error::CommandFailed(format!("could not render JSON: {e}"))
                        })?;
                        println!("{rendered}");
                    }
                } else if let Ok(custom) = swag_corelib::custom_templates::get(&name) {
                    println!(
                        "{}\n  base: {}\n  {}\n  {}",
                        custom.name,
                        custom.base.id(),
                        if custom.description.is_empty() {
                            "(no description)"
                        } else {
                            &custom.description
                        },
                        custom.source.describe()
                    );
                } else {
                    let builtin = Template::parse_id(&name)?;
                    println!(
                        "{}\n  {}\n  {}",
                        builtin.id(),
                        builtin.description(),
                        builtin.clone_url()
                    );
                }
            }
        },
        Command::Cd { name, print } => {
            let entry = swag_corelib::projects::get(&name)?;
            if !entry.path.exists() {
                return Err(Error::CommandFailed(format!(
                    "project '{}' points at '{}', which no longer exists (run `swag projects prune`)",
                    name,
                    entry.path.display()
                )));
            }
            swag_corelib::projects::touch(&name)?;
            if print {
                println!("{}", entry.path.display());
            } else {
                enter_subshell(&entry.path)?;
            }
        }
        Command::Install { dir } => {
            let bun = swag_corelib::Bun::new()?;
            bun.install(&dir)?;
            println!("`bun install` OK in {}", dir.display());
        }
        Command::Dev { dir, extra } => {
            project_at(&dir).dev(&extra)?;
        }
        Command::Build { dir, extra } => {
            project_at(&dir).build(&extra)?;
            println!("`bun run build` OK in {}", dir.display());
        }
        Command::Check { dir, extra } => {
            project_at(&dir).check(&extra)?;
            println!("`bun run check` OK in {}", dir.display());
        }
        Command::Test { dir, extra } => {
            project_at(&dir).test(&extra)?;
            println!("`bun run test` OK in {}", dir.display());
        }
        Command::Preview { dir, extra } => {
            project_at(&dir).preview(&extra)?;
        }
    }
    Ok(())
}

fn print_templates(json: bool) -> Result<(), Error> {
    let templates = project::list_templates();
    if json {
        let rendered = serde_json::to_string_pretty(&templates)
            .map_err(|e| Error::CommandFailed(format!("could not render JSON: {e}")))?;
        println!("{rendered}");
    } else {
        for t in templates {
            if t.builtin {
                println!("{}\n  {}\n  {}", t.id, t.description, t.clone_url);
            } else {
                println!(
                    "{} (custom, base {})\n  {}\n  {}",
                    t.id,
                    t.base.as_deref().unwrap_or("?"),
                    if t.description.is_empty() {
                        "(no description)"
                    } else {
                        &t.description
                    },
                    t.clone_url
                );
            }
        }
    }
    Ok(())
}

fn print_projects(json: bool) -> Result<(), Error> {
    let entries = swag_corelib::projects::list()?;
    if json {
        let rendered = serde_json::to_string_pretty(&entries)
            .map_err(|e| Error::CommandFailed(format!("could not render JSON: {e}")))?;
        println!("{rendered}");
    } else if entries.is_empty() {
        println!(
            "no projects registered (swag new auto-registers; see `swag projects add --help`)"
        );
    } else {
        for e in entries {
            print_project_human(&e);
        }
    }
    Ok(())
}

fn print_project_human(entry: &swag_corelib::ProjectEntry) {
    println!(
        "{} → {} (template {})",
        entry.name,
        entry.path.display(),
        entry.template
    );
}

/// Spawn an interactive shell with `dir` as CWD.
///
/// A child process cannot change its parent's working directory, so this is
/// a subshell: `exit` returns to the original directory.
fn enter_subshell(dir: &Path) -> Result<(), Error> {
    eprintln!("entering {} (exit to return)", dir.display());
    #[cfg(windows)]
    let (program, arg) = (
        std::env::var("COMSPEC").unwrap_or_else(|_| "cmd".to_string()),
        None,
    );
    #[cfg(not(windows))]
    let (program, arg): (String, Option<&str>) = (
        std::env::var("SHELL").unwrap_or_else(|_| "sh".to_string()),
        None,
    );
    let mut cmd = std::process::Command::new(&program);
    if let Some(arg) = arg {
        cmd.arg(arg);
    }
    let status = cmd
        .current_dir(dir)
        .status()
        .map_err(|e| Error::CommandFailed(format!("could not spawn subshell '{program}': {e}")))?;
    if status.success() {
        Ok(())
    } else if let Some(code) = status.code() {
        std::process::exit(code);
    } else {
        Err(Error::CommandFailed(format!(
            "subshell in '{}' terminated abnormally ({status})",
            dir.display()
        )))
    }
}

/// Wrap a directory as a [`Project`] for the bun passthrough commands.
/// The template field is informational here; real metadata comes from
/// the project directory itself.
fn project_at(dir: &Path) -> Project {
    let name = dir
        .file_name()
        .map(|s| s.to_string_lossy().into_owned())
        .unwrap_or_else(|| ".".to_string());
    Project {
        name,
        template: Template::SvelteClean.id().to_string(),
        path: dir.to_path_buf(),
    }
}
