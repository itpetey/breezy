# Repository Guidelines

Breezy is a small Rust-based GitHub Action that generates or updates draft releases based on merged PRs and a version file. Keep changes focused on action behavior and predictable, since it runs inside CI.

## Project Structure & Module Organization
- `src/main.rs` is the entrypoint that reads action inputs, resolves version/branch, and coordinates release updates.
- `src/config.rs` loads optional config from `.github/breezy.yml` or a provided path.
- `src/github.rs` wraps GitHub API calls for releases and PR search.
- `src/release_notes.rs` formats release notes and applies templates.
- `src/version.rs` resolves versions from `Cargo.toml` or `package.json`.
- `action.yml` and `Dockerfile` define the GitHub Action packaging.
- `Cargo.toml`/`Cargo.lock` define the Rust crate; `target/` is build output.

## Build, Test, and Development Commands
- `cargo build` compiles the action locally in debug mode.
- `cargo build --release` builds the optimized binary (used by the Docker image).
- `cargo run --` runs locally; set inputs as env vars, e.g. `INPUT_LANGUAGE=rust GITHUB_TOKEN=... GITHUB_REPOSITORY=org/repo GITHUB_REF_NAME=main`.
- `cargo test` runs unit/integration tests (none exist yet).
- `docker build -t breezy .` builds the action container locally.

## Coding Style & Naming Conventions
- Rust 2024 edition with standard rustfmt formatting (`cargo fmt`). Use 4-space indentation.
- Naming: `snake_case` for functions/vars, `UpperCamelCase` for types, `SCREAMING_SNAKE_CASE` for consts.
- Prefer `anyhow::Result` for error propagation and add context where failures would be ambiguous.

## Testing Guidelines
- Use Rust’s built-in test framework (`#[cfg(test)] mod tests`) for units and `tests/` for integration if needed.
- Avoid live GitHub API calls; mock or isolate parsing/formatting logic.
- Add tests when changing version parsing, config parsing, or release note templating.

## Commit & Pull Request Guidelines
- Current history uses short, sentence-case summaries (no prefixes). Follow the same style.
- PRs should include: a clear summary, testing performed (or “not run”), and any config/input changes.
- If you modify action inputs or templates, update `action.yml` or config examples accordingly.

## Configuration & Security Notes
- Action inputs are defined in `action.yml` (`github-token`, `language`, `tag-prefix`, `config-file`).
- Configuration can be provided via `.github/breezy.yml` or `config-file` input.
- Never log tokens or repository secrets; keep GitHub token scopes minimal.
