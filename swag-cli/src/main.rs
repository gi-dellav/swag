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
        /// svelte-fullstack-template (fullstack).
        #[arg(short, long, default_value = "svelte-clean-template")]
        template: String,

        /// Project name (directory + package name).
        name: String,

        /// Destination directory (defaults to <cwd>/<name>).
        #[arg(short, long)]
        output: Option<PathBuf>,

        /// Clone a branch or tag instead of the default branch.
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

    /// List the available templates.
    #[command(alias = "list-templates")]
    List {
        /// Print the catalogue as JSON.
        #[arg(long)]
        json: bool,
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
            let template = Template::parse_id(&template)?;
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
            if no_install {
                println!("skipped `bun install` — run `bun install` in the project dir");
            } else {
                println!("installed dependencies with `bun install`");
            }
        }
        Command::List { json } => {
            let templates = project::list_templates();
            if json {
                let rendered = serde_json::to_string_pretty(&templates)
                    .map_err(|e| Error::CommandFailed(format!("could not render JSON: {e}")))?;
                println!("{rendered}");
            } else {
                for t in templates {
                    println!("{}\n  {}\n  {}", t.id, t.description, t.clone_url);
                }
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
        template: Template::SvelteClean,
        path: dir.to_path_buf(),
    }
}
