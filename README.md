# oxmacs

An Emacs-inspired editor, written in Rust.

oxmacs feels like Emacs to use: the same keys, a minibuffer, `M-x`, buffers, windows and special buffers such as dired. Underneath it is its own design. There is no Lisp and no scripting layer: every feature is native Rust, drawn with [Iced](https://iced.rs), and works the same on Windows, Linux and macOS.

## Building

You need a stable Rust toolchain; `rust-toolchain.toml` picks it for you.

```sh
cargo run -p oxmacs --release
```

To install the `oxmacs` binary:

```sh
cargo install --path crates/oxmacs
```

## Build features

Larger parts of oxmacs are applets, each a [Cargo feature](https://doc.rust-lang.org/cargo/reference/features.html) on the `oxmacs` crate. A few are built by default:

| Feature   | What it adds                                                                 |
|-----------|------------------------------------------------------------------------------|
| `weather` | `M-x weather` with the forecast from Open-Meteo, and the weather in the mode line |

`--no-default-features` leaves them out for a lean editor.

Others are only built when you ask for them. These experimental applets are versioned on their own, apart from oxmacs:

| Feature           | Version | What it adds                                                                  |
|-------------------|---------|-------------------------------------------------------------------------------|
| `animated-splash` | 0.1.0   | A start screen with the oxmacs logo animated above the keys; `a` cycles effects |
| `demo`            | 0.1.0   | An example applet to copy when writing your own; `M-x demo`                  |
| `extra-themes`    | 0.1.0   | 56 more color themes for `M-x load-theme`, from gruvbox to modus-vivendi       |

Turn one on with `--features`, and separate several with commas:

```sh
cargo run -p oxmacs --release --features animated-splash
```

A feature that is left off is not compiled at all, so it costs nothing in size or startup time. Experimental features may change or disappear between releases; CHANGELOG.md lists them under "Experimental", and `M-x oxmacs-doctor` shows which ones your build includes.

More applets will be opt-in features as well. Until each one is moved into its own crate it is not yet optional:

| Feature        | What it will add                                  | Today                                  |
|----------------|---------------------------------------------------|----------------------------------------|
| `irc`          | The IRC client and its bouncer                    | Still compiled into every build        |
| `atlas`        | Notes, tasks, the agenda and the calendar         | Still compiled into every build        |
| `sqlite`       | A SQLite database viewer and editor               | Not written yet                        |
| `pdf`          | A PDF viewer                                      | Not written yet                        |

Once moved, a build without the feature contains none of that applet's code.

## Applets, not plugins

oxmacs has no plugin system. It does not load code at runtime, embed a scripting language or download extensions. Anything oxmacs can do is Rust code compiled into the binary, checked by the compiler, linted and tested like the rest.

Larger pieces of functionality are applets: self-contained crates such as an IRC client, a knowledge system, a directory editor or a language mode. Each applet

- lives in its own crate (`oxmacs-irc`, `oxmacs-atlas`, …) and depends on `oxmacs-core`,
- implements the applet API from `oxmacs-core`: the buffers and modes it adds, its commands and their documentation, its key bindings, and how its buffers draw,
- is an optional dependency of the `oxmacs` binary crate behind a Cargo feature of the same name.

Because built-in applets use exactly the same API, a fork can add its own the same way:

1. Create `crates/oxmacs-yours` and implement the applet API.
2. Add it to `crates/oxmacs/Cargo.toml` as an optional dependency with a matching feature.
3. Register it in the binary's applet list.
4. Build with `--features yours`.

Nothing outside your crate and those few lines needs to change, and an applet you leave out is simply not compiled. [crates/oxmacs-demo](crates/oxmacs-demo/README.md) is a small, complete example to start from. [crates/oxmacs-themes](crates/oxmacs-themes) adds color themes and [crates/oxmacs-splash](crates/oxmacs-splash) draws the animated start screen, both through the same API.

The applet API is being extracted from the editor now; the built-in applets move into their own crates as it lands.

## Configuration

Settings live in `config.toml` in the oxmacs config folder (for example `~/.config/oxmacs/config.toml` on Linux and `%APPDATA%\oxmacs\config.toml` on Windows). `M-x customize` edits them in a settings buffer, `M-x find-config-file` opens the file itself, and `M-x oxmacs-doctor` reports what oxmacs found on your system.

## Development

```sh
git config core.hooksPath .githooks
```

The pre-commit hook runs `cargo fmt --check`, `cargo clippy` with every feature turned on, and the tests both with every feature and with none. Changes are recorded in CHANGELOG.md.
