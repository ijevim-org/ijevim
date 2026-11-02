# AGENTS.md

## Project
Rust TUI Vim-like editor (ijevim v0.1.0). Single Cargo package, edition 2021.

## Commands
- Build: `cargo build` (debug) / `cargo build --release`
- Run: `cargo run -- [file]`
- Test: `cargo test`
- Lint: `cargo clippy`
- Format: `cargo fmt` (auto) / `cargo fmt --check`
- Docs: `cd docs && npm install && npm run dev` (dev) / `npm run build` (static build)

## Critical Gotchas
1. **Clean project**: All Bun/TypeScript leftover files (biome.json, etc.) have been removed. All config files are Rust-specific.
2. **Cargo.lock**: `.gitignore` incorrectly ignores `Cargo.lock`. For executables, commit it (see `.gitignore` comment).
3. **No Node.js/Bun for the editor**: This is a Rust project. Never use `bun`/`npm`/`node` to build or test the editor. The only Node exception is the VitePress docs site under `docs/`.
4. **CI**: `.github/workflows/ci.yml` is a valid Rust CI (fmt, clippy, test, audit). No `release.yml` exists.
5. **Commits**: Conventional Commits (via `commitlint`).

## Structure
- Entry: `src/main.rs` (tokio + clap CLI)
- Plugins: `src/plugins/` (multi-language: mlua, rust_lisp, quickjs-rusty, libloading)
- Syntax highlighting: `src/highlight/` (syntect)
- Text buffers: `ropey` crate
- TUI: `crossterm` crate
