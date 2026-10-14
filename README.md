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
`bun install --ignore-scripts` (unless `--no-install`) so that a template's
lifecycle/`postinstall` scripts are never executed — custom templates can
point at an untrusted git source. Before deleting the clone's `.git/`, the
exact upstream revision (`git rev-parse HEAD`) is recorded in `.swag.json`
provenance, pinning an otherwise unpinned `main` clone. Other flags:
`--output <dir>`, `--git-ref <branch|tag>`, `--force`. `--owner` is also
read from the `SWAG_GITHUB_OWNER` environment variable.

## Workspace layout

```
Cargo.toml          # workspace (swag-corelib + swag-cli)
swag-corelib/src/
  lib.rs            # public API surface
  template.rs       # Template enum + TemplateType (.template_version/.template_type)
  project.rs        # scaffold/list/personalize (clone|copy + rewrite + git init + bun install)
  provenance.rs     # .swag.json written at scaffold time (template/base/version/type/owner)
  update.rs         # version-aware update (fetch fresh, personalize, diff, apply)
  doctor.rs         # read-only env/home/project health checks (+ safe --fix)
  custom_templates.rs # custom template registry + resolve (builtin wins)
  projects.rs       # project registry (upsert/get/prune)
  home.rs           # ~/.swag resolution ($SWAG_HOME override) + atomic JSON
  bun.rs            # Bun wrapper (version check, install/dev/build/check/test/preview)
  error.rs          # thiserror Error
swag-cli/src/main.rs  # clap-derive CLI: new/list/projects/templates/cd/update/doctor/install/dev/build/check/test/preview
```

## Template versioning

New templates ship two files (no agent needed, plain file reads):

- `.template_version`: a single number (e.g. `21`), bumped on every template change.
- `.template_type`: `frontend`, `backend` or `desktop`; drives name validation
  (Cargo forbids `.` for `backend`/`desktop`) and `doctor` expectations
  (`backend`/`desktop` want a Rust layout, `desktop` wants `src-tauri/`).

Built-ins map to `clean → frontend`, `tauri → desktop`, `fullstack → backend`.
`scaffold` copies both files into the project and records them plus the
template id in `.swag.json` provenance, so `update`/`doctor` work from the
project directory alone (legacy projects without `.swag.json` fall back to
the version/type files plus a reverse registry lookup).

## Updating projects (`swag update`)

```bash
swag update                   # current dir: diff against template HEAD
swag update demo --check      # report only; exit 1 when behind
swag update demo --dry-run    # list M/A/D files without writing
swag update demo --json       # machine-readable report
swag update demo --prune      # also delete files removed upstream
swag update demo --force      # apply despite uncommitted git changes
swag update demo --git-ref main
```

The file diff is ground truth: `--check`/`--dry-run`/apply all compare the
project against a freshly fetched, re-personalized template. `.template_version`
is reported for information only, so a template that changes content without
bumping its version is still detected. Removed upstream files are reported,
never deleted, unless `--prune`. Apply refuses on a dirty git tree unless
`--force`, and records the fetched revision in `.swag.json`.

## Health checks (`swag doctor`)

```bash
swag doctor                   # bun/git/cargo, ~/.swag, registries
swag doctor demo              # …plus project checks (version/type/package)
swag doctor --json
swag doctor --fix             # only: create ~/.swag, prune stale entries
```

Exit 1 on any `FAIL` (missing `package.json`, unparsable version, unknown
type…); warnings (no `node_modules`, legacy files, dirty tree) tolerate.

## Development

```bash
cargo fmt
cargo clippy --all-targets -- -D warnings
cargo test
```
