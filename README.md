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

## Usage

```bash
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
  project.rs        # scaffold/list/personalize (clone + rewrite + git init + bun install)
  bun.rs            # Bun wrapper (version check, install/dev/build/check/test/preview)
  error.rs          # thiserror Error
swag-cli/src/main.rs  # clap-derive CLI: new/list/install/dev/build/check/test/preview
```

## Development

```bash
cargo fmt
cargo clippy --all-targets -- -D warnings
cargo test
```
