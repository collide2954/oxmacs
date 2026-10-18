# Changelog

Notable changes to mx. The format follows [Keep a Changelog](https://keepachangelog.com/en/1.1.0/) and versions follow [Semantic Versioning](https://semver.org/).

## [Unreleased]

### Added

- Mark and selection: `C-SPC`/`C-@` (twice deactivates), `C-x C-x`, `C-x h`/`s-a`; `C-g` deactivates.
- Minibuffer with `M-x` to run commands by name; `RET` runs, `C-g` aborts, `[No match]` on unknown names.
- Vertical `M-x` completion: space-separated substring matching, recent commands first, key hints, `C-n`/`C-p` to select, `TAB` to insert.
- Hollow cursor in the buffer while the minibuffer is active.

## [0.3.0] - 2026-10-17

### Added

- adwaita and adwaita-dark themes, following the OS light/dark setting live.
- `config.toml` in the platform config directory with `theme`, `font-size` (points), `tab-width` and `font-family`; invalid config falls back to defaults and reports in the echo area.
- Bundled JetBrains Mono font, with fallback when a configured font is not monospaced.
- Flat mode line with a 1px border.

### Changed

- Binary moved to `crates/mx`; the repository root is a virtual workspace.

## [0.2.0] - 2026-10-17

### Added

- Iced window with buffer view, block cursor, mode line (`L`/`C`) and echo area.
- Emacs motion and editing keys: `C-f`/`C-b`/`C-n`/`C-p`, `C-a`/`C-e`, `M-f`/`M-b`, `M-<`/`M->`, arrows, Home/End, `RET`, `C-d`, `DEL`.
- `C-g` to quit a key sequence, `C-x C-c` to exit, prefix echo (`C-x-`) and undefined-key messages.
- Viewport recentering, tab expansion and caret notation for control characters.
- Cross-platform key translation, including macOS Option as Meta.

## [0.1.0] - 2026-10-17

### Added

- Cargo workspace with centralized dependencies and lints, stable toolchain, and a pre-commit gate running fmt, clippy and tests.
