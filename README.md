# swag
(static webapp generator) ~ website development for the agentic era

A Rust workspace that scaffolds bun-managed Svelte webapps from opinionated
templates. `swag-corelib` holds all the logic; `swag-cli` is a thin
`clap`-derive CLI over it.

## Templates

| id | description |
|---|---|
| `svelte-clean-template` (alias `clean`) | Static PWA: Svelte 5 + Vite + TailwindCSS 4, GitHub Pages deploy |
| `svelte-tauri-template` (alias `tauri`) | Desktop app: Svelte 5 + Tauri 2, Rust core in `src-tauri/` |
| `svelte-fullstack-template` (alias `fullstack`) | Fullstack: Rust + Axum backend, TypeScript + Svelte frontend |

Templates are fetched with `git clone --depth 1` from
`github.com/gi-dellav/<template>`, so scaffolding always uses the latest
`main`. `bun` (≥ 1.1) is the canonical project tool: install, dev, build,
check, test and preview all shell out to `bun`, mirroring the
`svelte-clean-template` workflow (`bun install`, `bun run dev/check/test/build`).

## `~/.swag` home

`swag` keeps state in `~/.swag` (overridable with `$SWAG_HOME`; on Windows
`%USERPROFILE%\.swag`, falling back to `%APPDATA%\swag`):

```
~/.swag/
  projects.json    # registered projects (swag new auto-registers)
  templates.json   # custom templates deriving from the 3 built-ins
```

Registry files are written atomically and pretty-printed JSON, so they are
safe to inspect or edit by hand.

## Projects & templates

```bash
swag projects                 # list registered projects
swag projects add demo --path ./demo --template clean
swag projects show demo
swag projects remove demo
swag projects prune           # drop entries whose dirs are gone (--dry-run to preview)

swag templates                # list built-in + custom templates
swag templates add blog-acme --base clean --git https://github.com/acme/blog-base.git [--ref main]
swag templates add local-iter --base tauri --path ./my-base --description "local fork"
swag templates show blog-acme
swag templates remove blog-acme   # built-ins cannot be removed

swag cd demo                  # open a subshell in the project dir (exit to return)
swag cd demo --print          # print the path only (for scripting)
```

A child process cannot change its parent's working directory, so `swag cd`
spawns an interactive shell (`$SHELL`, `cmd` on Windows) with the project
dir as CWD instead of `cd`-ing your current shell.

Custom templates derive from one builtin (`--base clean|tauri|fullstack`),
which sets the personalization rules and Rust name validation; the content
comes from `--git <url> [--ref <branch|tag>]` (cloned fresh at scaffold
time) or `--path <dir>` (copied at scaffold time). `swag new --template
blog-acme …` rewrites both the custom name and the base id, then
auto-registers the new project in `projects.json`.

## Usage
cargo run -p swag-cli -- list
cargo run -p swag-cli -- new --template clean my-blog --owner my-handle
cargo run -p swag-cli -- new --template tauri my-desktop-app
cd my-blog
swag dev        # bun run dev
swag check      # bun run check (svelte-check)
swag test       # bun run test (bun test)
swag build      # bun run build -> dist/
swag preview    # bun run preview
```

`swag new` clones the template, rewrites template-name occurrences
(`package.json` + `bun.lock`, PWA manifest / SEO site names,
`src-tauri/Cargo.toml` + `Cargo.lock`, `tauri.conf.json` productName /
identifier, Svelte eyebrows, README, …) to the project name — and, when
`--owner` differs from the default `gi-dellav`, rewrites repo slugs, GitHub
URLs, Pages domains and the Tauri `com.gidellav.*` identifier to your handle
(hyphens stripped there, where they are illegal). It then detaches the
template git history (`git init` fresh, unless `--no-git-init`), and runs
`bun install` (unless `--no-install`). Other flags: `--output <dir>`,
`--git-ref <branch|tag>`, `--force`. `--owner` is also read from the
`SWAG_GITHUB_OWNER` environment variable.

## Workspace layout

```
Cargo.toml          # workspace (swag-corelib + swag-cli)
swag-corelib/src/
  lib.rs            # public API surface
  template.rs       # Template enum: ids, clone URLs, aliases
  project.rs        # scaffold/list/personalize (clone|copy + rewrite + git init + bun install)
  custom_templates.rs # custom template registry + resolve (builtin wins)
  projects.rs       # project registry (upsert/get/prune)
  home.rs           # ~/.swag resolution ($SWAG_HOME override) + atomic JSON
  bun.rs            # Bun wrapper (version check, install/dev/build/check/test/preview)
  error.rs          # thiserror Error
swag-cli/src/main.rs  # clap-derive CLI: new/list/projects/templates/cd/install/dev/build/check/test/preview
```

## Development

```bash
cargo fmt
cargo clippy --all-targets -- -D warnings
cargo test
```
