# oxmacs

An Emacs-inspired editor, written in Rust.

oxmacs feels like Emacs to use: the same keys, a minibuffer, `M-x`, buffers, windows and special buffers such as dired. Underneath it is its own design: a native Rust core, drawn with [Iced](https://iced.rs), that works the same on Windows, Linux and macOS, extended in Ox, its own modern Scheme.

## Building

You need a stable Rust toolchain; `rust-toolchain.toml` picks it for you.

```sh
cargo run -p oxmacs --release
```

To install the `oxmacs` binary:

```sh
cargo install --path crates/oxmacs
```

## Extending oxmacs

oxmacs is moving its features into Ox, an extension language of its own: a modern Scheme with a fast bytecode VM, built in [crates/ox](crates/ox) and designed in [docs/ox-design.md](docs/ox-design.md). The `ox` command runs `.ox` files and opens a REPL:

```sh
cargo run -p ox -- hello.ox
```

Dired, git and weather are back as bundled Ox plugins. The rest of the former Cargo-feature applets (Markdown, Atlas, IRC, the database browser, the PDF viewer, Python, the extra themes and the animated start screen) return the same way.
## Configuration

Settings live in `config.toml` in the oxmacs config folder (for example `~/.config/oxmacs/config.toml` on Linux and `%APPDATA%\oxmacs\config.toml` on Windows). `M-x customize` edits them in a settings buffer, `M-x find-config-file` opens the file itself, and `M-x oxmacs-doctor` reports what oxmacs found on your system.

## Development

```sh
git config core.hooksPath .githooks
```

The pre-commit and pre-push hooks run `cargo fmt --check`, `cargo clippy` and the tests, which takes a minute or two once built. Changes are recorded in CHANGELOG.md.
