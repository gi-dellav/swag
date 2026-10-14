//! `swag` — static webapp generator.
//!
//! Thin clap-derive CLI over [`swag_corelib`]. All logic lives in the
//! library; this binary only parses arguments and reports results.

use std::io::{BufRead, IsTerminal, Write};
use std::path::{Path, PathBuf};

use clap::{Parser, Subcommand};
use dialoguer::{Input, Select};

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
        /// template id: [default: svelte-clean-template]
        #[arg(short, long)]
        template: Option<String>,

        /// Project name (directory + package name); prompted when omitted.
        name: Option<String>,

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

    /// Scaffold into a pre-existing directory instead of creating a new one.
    ///
    /// Like `swag new`, but the destination directory must already exist and
    /// is used in place: `--force` empties it first (the directory itself is
    /// kept). The destination defaults to the current directory.
    Init {
        /// template id: [default: svelte-clean-template]
        #[arg(short, long)]
        template: Option<String>,

        /// Project name (package name); prompted when omitted.
        name: Option<String>,

        /// Destination directory (defaults to the current directory).
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

        /// Empty the destination first (the directory itself is kept).
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

    /// Update a scaffolded project to its template HEAD (version-aware).
    Update {
        /// Registered project name or directory (defaults to cwd).
        #[arg(default_value = ".")]
        target: String,

        /// Only report; exit 1 when an update is available.
        #[arg(long)]
        check: bool,

        /// Show what would change without writing files.
        #[arg(long)]
        dry_run: bool,

        /// Apply even with uncommitted git changes.
        #[arg(long)]
        force: bool,

        /// Delete files removed upstream (default: report only).
        #[arg(long)]
        prune: bool,

        /// Branch or tag overriding the registered ref for git templates.
        #[arg(long)]
        git_ref: Option<String>,

        /// Print the report as JSON.
        #[arg(long)]
        json: bool,
    },

    /// Check environment, home registries and project health.
    Doctor {
        /// Registered project name or directory (omit for env checks only).
        target: Option<String>,

        /// Print the report as JSON.
        #[arg(long)]
        json: bool,

        /// Safe repairs: create ~/.swag and prune stale registry entries.
        #[arg(long)]
        fix: bool,
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
            run_scaffold(
                template,
                name,
                output,
                git_ref,
                owner,
                force,
                no_install,
                no_git_init,
                false,
            )?;
        }
        Command::Init {
            template,
            name,
            output,
            git_ref,
            owner,
            force,
            no_install,
            no_git_init,
        } => {
            run_scaffold(
                template,
                name,
                output,
                git_ref,
                owner,
                force,
                no_install,
                no_git_init,
                true,
            )?;
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
                if !entry.path.exists() {
                    eprintln!(
                        "warning: '{}' does not exist yet (registered anyway; `swag cd`/`update` will fail until it does)",
                        entry.path.display()
                    );
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
                        .map_err(|e| Error::Internal(format!("could not render JSON: {e}")))?;
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
                        let rendered = serde_json::to_string_pretty(&custom)
                            .map_err(|e| Error::Internal(format!("could not render JSON: {e}")))?;
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
                        let rendered = serde_json::to_string_pretty(&info)
                            .map_err(|e| Error::Internal(format!("could not render JSON: {e}")))?;
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
        Command::Update {
            target,
            check,
            dry_run,
            force,
            prune,
            git_ref,
            json,
        } => {
            let dir = resolve_update_target(&target)?;
            let report = swag_corelib::update::update(&swag_corelib::update::UpdateOptions {
                dir,
                check_only: check,
                dry_run,
                force,
                prune,
                git_ref,
            })?;
            if json {
                let rendered = serde_json::to_string_pretty(&report)
                    .map_err(|e| Error::Internal(format!("could not render JSON: {e}")))?;
                println!("{rendered}");
            } else {
                print_update_human(&report, check || dry_run);
            }
            if check && !report.up_to_date {
                std::process::exit(1);
            }
        }
        Command::Doctor { target, json, fix } => {
            let dir = match target {
                Some(t) => Some(resolve_update_target(&t)?),
                None => None,
            };
            let report = swag_corelib::doctor::doctor(dir.as_deref());
            if json {
                let rendered = serde_json::to_string_pretty(&report)
                    .map_err(|e| Error::Internal(format!("could not render JSON: {e}")))?;
                println!("{rendered}");
            } else {
                print_doctor_human(&report);
            }
            if fix {
                for line in swag_corelib::doctor::fix()? {
                    println!("fix: {line}");
                }
            }
            if !report.healthy() {
                return Err(Error::CommandFailed(
                    "doctor found failing checks".to_string(),
                ));
            }
        }
    }
    Ok(())
}

/// Template id used when the user does not pick one.
const DEFAULT_TEMPLATE: &str = "svelte-clean-template";

/// Shared implementation of `swag new` and `swag init`.
///
/// `init` scaffolds into a pre-existing directory instead of creating one
/// (see [`ScaffoldOptions::init`]). Both commands accept an optional NAME and
/// `--template`: when either is omitted the user is asked on stdin, either
/// with a `dialoguer` prompt (terminal) or by reading one line from stdin
/// (piped input), so `echo demo | swag new` works.
#[allow(clippy::too_many_arguments)]
fn run_scaffold(
    template: Option<String>,
    name: Option<String>,
    output: Option<PathBuf>,
    git_ref: Option<String>,
    owner: Option<String>,
    force: bool,
    no_install: bool,
    no_git_init: bool,
    init: bool,
) -> Result<(), Error> {
    let name = match name {
        Some(name) => name,
        None => {
            let default = if init {
                default_init_name(output.as_deref())
            } else {
                None
            };
            prompt_project_name(default)?
        }
    };
    let template = match template {
        Some(template) => resolve_template_id(&template)?,
        None => prompt_template()?,
    };
    let options = ScaffoldOptions {
        template,
        name,
        destination: output,
        git_ref,
        force,
        owner,
        no_install,
        no_git_init,
        init,
    };
    let project = project::scaffold(&options)?;
    println!(
        "{} {} from {} in {}",
        if init { "initialized" } else { "created" },
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
    Ok(())
}

/// Canonical id for a user-supplied template id or short alias; unknown ids
/// fail with the catalogue list, exactly like the scaffolder does.
fn resolve_template_id(id: &str) -> Result<String, Error> {
    Ok(swag_corelib::custom_templates::resolve_id(id)?.id())
}

/// Default project name for `swag init`: the basename of the directory being
/// initialized (`--output` when given, else the current directory).
///
/// Returns `None` when the directory has no usable basename (filesystem root)
/// or when that basename is not a valid project name, so the caller asks
/// without offering a default instead of failing later in the scaffolder.
fn default_init_name(output: Option<&Path>) -> Option<String> {
    let dir = match output {
        Some(dir) => dir.to_path_buf(),
        None => std::env::current_dir().ok()?,
    };
    // `--output .` and `--output ..` have no basename of their own.
    let dir = dir.canonicalize().unwrap_or(dir);
    let name = dir.file_name()?.to_string_lossy().into_owned();
    project::validate_project_name(Template::SvelteClean, &name)
        .is_ok()
        .then_some(name)
}

/// Ask for the project name (dialoguer on a terminal, one stdin line
/// otherwise). `default` is used for an empty answer when there is one.
fn prompt_project_name(default: Option<String>) -> Result<String, Error> {
    if !stdin_prompts_supported() {
        // Piped stdin: the next line is the name, empty = default.
        let prompt = match &default {
            Some(default) => format!("Project name [{default}]"),
            None => "Project name".to_string(),
        };
        return match read_prompt_line(&prompt)? {
            Some(line) if !line.is_empty() => Ok(line),
            _ => default.ok_or_else(|| {
                Error::CommandFailed(
                    "no project name given (pass NAME, or pipe it on stdin)".to_string(),
                )
            }),
        };
    }
    let mut input = Input::<String>::new()
        .with_prompt("Project name")
        .report(false)
        .validate_with(|name: &String| {
            project::validate_project_name(Template::SvelteClean, name).map_err(|e| e.to_string())
        });
    if let Some(default) = default {
        input = input.default(default);
    }
    interact_text(input)
}

/// Ask which template to use, from the `swag templates` catalogue (built-in
/// templates first, then custom ones).
fn prompt_template() -> Result<String, Error> {
    let templates = project::list_templates();
    if !stdin_prompts_supported() {
        // Piped stdin: one template id or alias per line, empty = default.
        let line = read_prompt_line(&format!("Template [{}]", DEFAULT_TEMPLATE))?;
        return match line.as_deref() {
            None | Some("") => Ok(DEFAULT_TEMPLATE.to_string()),
            Some(id) => resolve_template_id(id),
        };
    }
    let items: Vec<String> = templates.iter().map(describe_template).collect();
    let default = templates
        .iter()
        .position(|t| t.id == DEFAULT_TEMPLATE)
        .unwrap_or(0);
    let selection = Select::new()
        .with_prompt("Template")
        .items(&items)
        .default(default)
        .interact()
        .map_err(|e| prompt_failed("template", e))?;
    Ok(templates[selection].id.clone())
}

/// One `Select` row: the id plus a short qualifier for custom templates.
fn describe_template(info: &project::TemplateInfo) -> String {
    match &info.base {
        Some(base) => format!("{} (custom, base {base})", info.id),
        None => info.id.clone(),
    }
}

/// Run a dialoguer text prompt.
fn interact_text(input: Input<'_, String>) -> Result<String, Error> {
    input.interact_text().map_err(|e| prompt_failed("input", e))
}

/// Whether an interactive `dialoguer` prompt can run: it reads keys from
/// stdin and renders on stderr, so both must be terminals. Anything else
/// (pipes, `/dev/null`, CI) takes the plain-stdin fallback below.
fn stdin_prompts_supported() -> bool {
    std::io::stdin().is_terminal() && std::io::stderr().is_terminal()
}

/// Non-interactive fallback: print `prompt` to stderr and read one line from
/// stdin. Returns `None` on EOF, `Some("")` for a blank line.
fn read_prompt_line(prompt: &str) -> Result<Option<String>, Error> {
    eprint!("{prompt}: ");
    let _ = std::io::stderr().flush();
    let mut line = String::new();
    let read = std::io::stdin()
        .lock()
        .read_line(&mut line)
        .map_err(|e| Error::Io {
            context: "could not read from stdin".to_string(),
            source: e,
        })?;
    if read == 0 {
        return Ok(None);
    }
    Ok(Some(line.trim().to_string()))
}

/// Map a dialoguer failure (interrupted prompt, closed terminal) to a swag
/// error. The `NotConnected` case is already excluded by
/// [`stdin_prompts_supported`], so this is either EOF or Ctrl-C.
fn prompt_failed(prompt: &str, e: dialoguer::Error) -> Error {
    Error::Io {
        context: format!("could not read the {prompt} prompt"),
        source: e.into(),
    }
}

fn print_templates(json: bool) -> Result<(), Error> {
    let templates = project::list_templates();
    if json {
        let rendered = serde_json::to_string_pretty(&templates)
            .map_err(|e| Error::Internal(format!("could not render JSON: {e}")))?;
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
            .map_err(|e| Error::Internal(format!("could not render JSON: {e}")))?;
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

/// Resolve an `update`/`doctor` target: `.`/paths directly, otherwise a
/// registered project name (registry wins, like `swag cd --print`).
fn resolve_update_target(target: &str) -> Result<std::path::PathBuf, Error> {
    if target == "." {
        return Ok(PathBuf::from("."));
    }
    let as_path = Path::new(target);
    if as_path.exists() {
        return Ok(as_path.to_path_buf());
    }
    swag_corelib::update::resolve_target(target)
}

fn print_update_human(report: &swag_corelib::update::UpdateReport, preview: bool) {
    let local = report
        .local_version
        .map(|v| format!("v{v}"))
        .unwrap_or_else(|| "?".to_string());
    let upstream = report
        .upstream_version
        .map(|v| format!("v{v}"))
        .unwrap_or_else(|| "?".to_string());
    if report.up_to_date {
        println!(
            "{} up to date (local {local}, upstream {upstream})",
            report.path.display()
        );
        return;
    }
    println!(
        "{} behind (local {local} → upstream {upstream})",
        report.path.display()
    );
    for f in &report.changed {
        println!("  M {f}");
    }
    for f in &report.added {
        println!("  A {f}");
    }
    for f in &report.removed {
        println!("  D {f} (kept; re-run with --prune to delete)");
    }
    if report.applied {
        println!("applied update from {}", report.template);
    } else if preview {
        println!("preview only; re-run without --check/--dry-run to apply");
    }
}

fn print_doctor_human(report: &swag_corelib::doctor::DoctorReport) {
    for check in &report.checks {
        let mark = match check.status {
            swag_corelib::doctor::Status::Ok => "ok",
            swag_corelib::doctor::Status::Warn => "warn",
            swag_corelib::doctor::Status::Fail => "FAIL",
        };
        if check.hint.is_empty() {
            println!("[{mark}] {}: {}", check.name, check.message);
        } else {
            println!(
                "[{mark}] {}: {} (hint: {})",
                check.name, check.message, check.hint
            );
        }
    }
}

/// Wrap a directory as a [`Project`] for the bun passthrough commands.
/// The template field is informational here; real metadata comes from
/// the project directory itself (`.swag.json`), falling back to the
/// default built-in when there is no provenance to read.
fn project_at(dir: &Path) -> Project {
    let name = dir
        .file_name()
        .map(|s| s.to_string_lossy().into_owned())
        .unwrap_or_else(|| ".".to_string());
    let template = swag_corelib::provenance::load(dir)
        .map(|p| p.template)
        .unwrap_or_else(|_| Template::SvelteClean.id().to_string());
    Project {
        name,
        template,
        path: dir.to_path_buf(),
    }
}
