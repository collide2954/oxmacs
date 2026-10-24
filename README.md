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

Most of oxmacs is bundled Ox plugins. Those on by default load the first time you use them; opt-in plugins stay off until `(use-plugin name)` in `init.ox`. The rest of the former Cargo-feature applets (Atlas, the database browser, the PDF viewer, Python, the extra themes and the animated start screen) return the same way.

| Plugin | Version | Default | What it does |
| --- | --- | --- | --- |
| `calendar` | 0.1.0 | on | Calendars in .ics files, with CalDAV sync, reminders and invitations |
| `customize` | 0.1.1 | on | Change the editor's and plugins' settings in a form |
| `dired` | 0.2.2 | on | List directories and manage their files |
| `git` | 0.2.3 | on | Git status and staging in the style of Magit |
| `irc` | 0.8.1 | on | An IRCv3 client in the style of ERC |
| `life` | 0.2.0 | opt-in | Conway's Game of Life on a canvas |
| `manual` | 0.5.0 | on | The oxmacs tutorial and the Ox manual |
| `markdown` | 0.1.1 | on | Markdown rendered as styled text |
| `processes` | 0.1.0 | on | List the processes oxmacs runs, with their usage |
| `profiler` | 0.1.1 | on | Find where the Ox code in oxmacs spends its time |
| `r` | 0.18.0 | opt-in | R consoles, plots, data and help in the style of ESS and RStudio |
| `remote` | 0.1.0 | on | Browse and edit files on other machines over ssh, sftp and ftp |
| `screensaver` | 0.1.1 | on | A starfield or matrix rain after a while without input |
| `vault` | 0.1.0 | on | An encrypted password vault with one-time codes and imports |
| `weather` | 0.2.1 | on | Forecasts from Open-Meteo |

Ox code uses these built-in modules, importing them as in `(import (ox file))`; the prelude needs no import:

| Module | What it gives |
| --- | --- |
| prelude | Numbers, collections, text, records, errors, pattern matching, tasks, channels and promises, there without an import |
| `ox/string` | Text: searching, splitting, joining, trimming, padding and changing case |
| `ox/regex` | Regular expressions: matching, finding, replacing and splitting |
| `ox/json` | Reading and writing JSON |
| `ox/csv` | Reading CSV into rows |
| `ox/time` | Dates and times: the clock, parsing, formatting, time zones and durations |
| `ox/path` | Taking paths apart and joining them, the home folder and ~ expansion |
| `ox/file` | Reading, writing, listing, copying, renaming and removing files and folders |
| `ox/process` | Running programs, talking to them while they run, and the environment |
| `ox/net` | TCP connections, with or without TLS |
| `ox/http` | HTTP requests and URL encoding |
| `ox/crypto` | Hashes, encryption with a password, random numbers, base64 and one-time codes |
| `ox/image` | Opening, editing, drawing on and saving images, with filters and color adjustments |
| `ox/ui` | Widgets for plugin views: text, rows, columns, buttons, images and canvases; in oxmacs only |

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
