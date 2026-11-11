# ijevim

A minimal Vim-like TUI editor written in Rust.

## Features

- Vim modes: Normal, Insert, Visual, Command, Replace
- Vim operators: d, y, c, p, >, etc.
- Registers (yank/paste)
- Undo/Redo
- Syntax highlighting (syntect)
- Multi-language plugin support (Lua, Lisp, JavaScript, Rust, Nix)

## CLI usage examples

```sh
# Start with Vim keymap
ijevim vim

# Start with Emacs keymap
ijevim emacs

# Open a file (defaults to Vim keymap)
ijevim <file>
```

## Documentation

- [Documentation site](https://ijevim-org.github.io/)
- [Keybinding specification](https://ijevim-org.github.io/keybindings)

## Install

```sh
cargo install ijevim
```

## キーマップ互換レベル

- [Vim キーマップ互換レベル](https://ijevim-org.github.io/keymap-vim)
- [Emacs キーマップ互換レベル](https://ijevim-org.github.io/keymap-emacs)

## Development

```sh
cargo build
cargo run -- [file]
cargo test
```

## Dependencies

<!-- deps:start -->
Direct dependencies only. Transitive crate counts include the workspace crates.

| Scope | crates in graph |
| --- | --- |
| Default build | 57 |
| `--all-features` | 89 |
| Default + dev-dependencies | 95 |

### Runtime

| Crate | Version | Used by | License |
| --- | --- | --- | --- |
| [crossterm](https://crates.io/crates/crossterm) | 0.29.0 | `ijevim`, `ijevim-tui` | MIT |
| [regex](https://crates.io/crates/regex) | 1.13.1 | `ijevim`, `ijevim-core` | MIT OR Apache-2.0 |
| [ropey](https://crates.io/crates/ropey) | 1.6.1 | `ijevim-core` | MIT |
| [thiserror](https://crates.io/crates/thiserror) | 2.0.21 | `ijevim-core` | MIT OR Apache-2.0 |
| [tokio](https://crates.io/crates/tokio) | 1.53.3 | `ijevim`, `ijevim-core` | MIT |
| [toml](https://crates.io/crates/toml) | 0.8.23 | `ijevim-core` | MIT OR Apache-2.0 |
| [unicode-width](https://crates.io/crates/unicode-width) | 0.2.3 | `ijevim-tui` | MIT OR Apache-2.0 |

### Optional

Only compiled when the matching cargo feature is enabled.

| Crate | Version | Used by | License |
| --- | --- | --- | --- |
| [libloading](https://crates.io/crates/libloading) | 0.8.9 | `ijevim-plugin` | ISC |
| [mlua](https://crates.io/crates/mlua) | 0.9.9 | `ijevim-plugin` | MIT |
| [quickjs-rusty](https://crates.io/crates/quickjs-rusty) | 0.11.1 | `ijevim-plugin` | MIT |
| [rust_lisp](https://crates.io/crates/rust_lisp) | 0.18.0 | `ijevim-plugin` | MIT |
| [serde_json](https://crates.io/crates/serde_json) | 1.0.154 | `ijevim-plugin` | MIT OR Apache-2.0 |
| [syntect](https://crates.io/crates/syntect) | 5.3.0 | `ijevim-highlight` | MIT |

### Development

| Crate | Version | Used by | License |
| --- | --- | --- | --- |
| [criterion](https://crates.io/crates/criterion) | 0.5.1 | `ijevim` | Apache-2.0 OR MIT |
| [tempfile](https://crates.io/crates/tempfile) | 3.27.0 | `ijevim` | MIT OR Apache-2.0 |

All licenses are permissive (MIT, Apache-2.0, ISC, BSD, Unlicense, Zlib, 0BSD,
Unicode-3.0). No copyleft license appears in the dependency graph.
<!-- deps:end -->

## License

MIT
