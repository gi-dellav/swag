# swag
*(static webapp generator) ~ website development for the agentic era*

*SWAG* is a toolkit for creating web applications by using high-quality templates with design systems and agent-ready knowledge bases.

An example of the design system used by *SWAG*'s default Templates can be found [here](https://gi-dellav.github.io/svelte-clean-template/)

## Features

- *Lightweight*: *SWAG* is a Rust library+CLI, with a desktop app in development.
- *High-quality*: All templates are audited and managed by human developers.
- *Ready*: Templates ship with libraries, design systems, utility functions.
- *Native*: *SWAG* ships PWA for browser applications and uses Tauri for desktop applications.
- *Expandable*: All code and UIs shipped by the templates are well documented, allowing for simple edits.
- *Deployable*: *SWAG* has integrations for Github Pages and for Vercel.
- *Updatable*: All templates ship with incremental versioning, allowing for simple updates.

## Installation

You can download the *SWAG* CLI from:

- Github Releases

- Cargo: `cargo install swag-cli`

## Usage

### Projects

A Project can be created with `swag new`, you can run a development server with `swag dev`, you can build a production version with `swag build`, and you can deploy to Vercel with `swag vc`.

`swag new` prompts for a missing project name and template; use `swag init` to scaffold into an existing directory instead of creating a new one (name and template are prompted there too).

You can manage user-level Projects with `swag projects`.

### Templates

All projects derived from Templates; the default templates are:

| id | description | repo |
|---|---|---|
| `svelte-clean-template` (alias `clean`) | Static PWA: Svelte 5 + Vite + TailwindCSS 4, GitHub Pages deploy | [Github](https://github.com/gi-dellav/svelte-clean-template) |
| `svelte-tauri-template` (alias `tauri`) | Desktop app: Svelte 5 + Tauri 2, Rust core in `src-tauri/` | [Github](https://github.com/gi-dellav/svelte-tauri-template) |
| `svelte-fullstack-template` (alias `fullstack`) | Fullstack: Rust + Axum backend, TypeScript + Svelte frontend | [Github](https://github.com/gi-dellav/svelte-fullstack-template) |

You can manage custom Templates with `swag templates` and you can update the Template used by a project using `swag update`.
