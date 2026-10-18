# Changelog

Notable changes to mx. The format follows [Keep a Changelog](https://keepachangelog.com/en/1.1.0/) and versions follow [Semantic Versioning](https://semver.org/).

## [Unreleased]

### Added

- `read-only-mode` (`C-x C-q`) toggles a buffer read-only; the mode line shows `%%` and edits are refused. Files without write permission open read-only.
- `dired` (`C-x d`) lists a directory in a read-only buffer: size, modification time and name, directories first, a `..` line on top; the prompt offers only directories.

## [0.7.0] - 2026-10-18

### Added

- `find-file` (`C-x C-f`) with a vertical directory list: typing filters, `TAB`/`RET` descend into directories, `DEL` after a separator goes up, `M-RET` uses the exact input; `~` abbreviates home.
- Files open in their own buffers; LF/CRLF and UTF-8 BOMs are preserved, and invalid UTF-8 is refused.
- Command line `mx [+LINE[:COLUMN]] FILE...` opens each file, jumping to the position given before it.
- Mode line shows `**` for unsaved changes, `--` otherwise, and `CRLF` for Windows line endings.
- `save-buffer` (`C-x C-s`) writes atomically, keeping permissions and the original line endings.
- `write-file` (`C-x C-w`) saves under a new name, asking before overwriting another file; `C-x C-s` in a buffer without a file asks where to write it.
- `save-some-buffers` (`C-x s`) and the `C-x C-c` quit guard ask per modified file: `y`, `n`, `!` (save the rest) or `q`.
- `switch-to-buffer` (`C-x b`) lists buffers most recent first with their file paths; `RET` on empty input returns to the previous buffer, an unknown name creates a buffer, and each buffer keeps its scroll position.
- `kill-buffer` (`C-x k`) defaults to the current buffer and asks before discarding unsaved file changes; killing the last buffer leaves an empty `*scratch*`.
- Buffers for files sharing a name show their directory, like `main.rs<src>`.
- Files changed by other programs are noticed right away (and when the window regains focus): unmodified buffers reload, modified ones keep their edits with a warning. `on-disk-change = "revert" | "ask" | "ignore"` in `config.toml` picks the behavior.
- `C-x C-s` asks before overwriting a file that changed on disk since it was read or saved.
- `revert-buffer` rereads the file, asking first only when there are unsaved edits; reverting can be undone.

## [0.6.0] - 2026-10-18

### Added

- Mouse: click moves point, drag selects, double-click selects a word, triple-click a line, shift-click extends; I-beam pointer over text.
- Mouse wheel scrolls 3 lines per notch (trackpads scroll smoothly) and keeps point in view.
- On Linux, selections set PRIMARY and middle-click pastes it at the mouse.

## [0.5.0] - 2026-10-18

### Added

- System clipboard `copy` (`M-w`, `s-c`), `cut` (`C-w`, `s-x`) and `paste` (`C-y`, `s-v`); copy and cut act on the whole line without a selection, and pasting replaces the selection.
- `paste-from-history` (`M-y`) picks from the last 50 copies in the minibuffer list.
- `delete-line` (`C-k`), `delete-word` (`M-d`, `C-<delete>`) and `backward-delete-word` (`M-DEL`, `C-DEL`); they never touch the clipboard.
- Typing, `RET` and paste replace the selection; `DEL`, `C-d` and `<delete>` delete it.
- `undo` (`C-/`, `C-_`, `C-x u`, `s-z`) and `redo` (`C-?`, `C-M-_`, `s-S-z`); typing groups into steps of 20 characters.

## [0.4.0] - 2026-10-18

### Added

- Mark and selection: `C-SPC`/`C-@` (twice deactivates), `C-x C-x`, `C-x h`/`s-a`; `C-g` deactivates.
- Shift-selection with any shifted motion (`S-<right>`, `C-S-f`, `M-S-f`, `S-<end>`...).
- Selection highlighted with the theme's region face, extending to the edge across newlines.
- Cursor glides on jumps and fade-blinks while idle (stops after 10 blinks); `animations = false` turns both off.
- `cursor-blink-interval` setting (default 400ms, 0 disables) with a crisper 60ms fade.
- Smooth scrolling; long jumps glide in from one screen away.
- Theme changes crossfade and the `M-x` list fades in.
- Animations turn off when the OS asks for reduced motion.
- Minibuffer with `M-x` to run commands by name; `RET` runs, `C-g` aborts, `[No match]` on unknown names.
- Vertical `M-x` completion: space-separated substring matching, recent commands first, key hints, `C-n`/`C-p` to select, `TAB` to insert.
- Hollow cursor in the buffer while the minibuffer is active.
- `ESC ESC` quits like `C-g`, including mid-sequence and in the minibuffer.
- `ESC` acts as a Meta prefix (`ESC f` = `M-f`).
- `C-m`, `C-i` and `C-[` are `RET`, `TAB` and `ESC`; `C-j` inserts a newline (runs the command in the minibuffer).

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
