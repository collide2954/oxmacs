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

## Installing on Fedora

Fedora 44 and 45 have a package repository, built by CI:

```sh
sudo dnf config-manager addrepo --from-repofile=https://collide2954.github.io/oxmacs/rpm/oxmacs.repo
sudo dnf install oxmacs
```

`oxmacs-nightly.repo` in the same folder has a build of every change on master.

## Extending oxmacs

oxmacs is moving its features into Ox, an extension language of its own: a modern Scheme with a fast bytecode VM, built in [crates/ox](crates/ox) and documented in the Ox manual inside oxmacs, on `C-h i`. The `ox` command runs `.ox` files and opens a REPL, and `ox check`, `ox test`, `ox profile` and `ox perf` check, test, profile and benchmark Ox code:

```sh
cargo run -p ox-cli -- hello.ox
```

Dired, git, weather, an IRC client, a calendar, a password vault, a remote file browser for ssh, sftp and ftp, and the screensavers are bundled Ox plugins. Conway's Game of Life is bundled too, as a small example plugin that stays off until `(use-plugin life)` in `init.ox`. The rest of the former Cargo-feature applets (Markdown, Atlas, the database browser, the PDF viewer, Python, the extra themes and the animated start screen) return the same way.

## Configuration

oxmacs is configured in Ox, in `~/.oxmacs/init.ox` (`OXMACS_HOME` moves the folder):

```clojure
(settings {theme: :adwaita-dark font-size: 12 tab-width: 4})
(use-plugin weather :config {location: "Berlin" mode-line: true})
(bind-key "C-c w" 'weather)
```

`M-x customize` changes settings in a form and saves them to `~/.oxmacs/custom.ox`, which runs before `init.ox`, so `init.ox` wins. `M-x find-init-file` opens `init.ox`, and saving it applies it at once.

## Development

Run `cargo fmt --all` and `cargo clippy --workspace --all-targets --all-features -- -D warnings` before committing. CI runs clippy, a build and the tests on Linux, macOS and Windows for pushes to master and pull requests. Changes are recorded in CHANGELOG.md.

## License

oxmacs is licensed under the [Apache License, Version 2.0](LICENSE-APACHE). Contributions are accepted under the same terms.

The bundled JetBrains Mono font is under the [SIL Open Font License 1.1](crates/oxmacs-gui/fonts/JetBrainsMono-OFL.txt), and the highlight queries adapted from tree-sitter grammars keep their [MIT notices](crates/oxmacs-core/queries/NOTICE.md).
