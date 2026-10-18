# Changelog

Notable changes to oxmacs, called mx before 0.12. The format follows [Keep a Changelog](https://keepachangelog.com/en/1.1.0/) and versions follow [Semantic Versioning](https://semver.org/).

## [Unreleased]

### Added

- Windows: `C-x 2` and `C-x 3` split the selected window, `C-x o` and `C-c <arrow>` move between windows, and `C-x 0` and `C-x 1` close them. Each window keeps its own point and scroll and has its own mode line, dimmed when inactive.
- Clicking selects a window, and the wheel scrolls the window under the mouse. Drag the borders between windows, or use `C-x +` to balance them and `C-x ^`, `C-x }` and `C-x {` to resize.
- Markdown preview: `C-c C-c p` shows the rendered document beside its source and updates as you type; links open in the browser. Press it again to close the preview.
- `M-x compile` runs a command, `cargo build` by default, in a `*compilation*` window that follows its output. `g` reruns it and `C-c C-k` stops it. In Rust buffers, `C-c C-c` then `C-k`, `C-b`, `C-t`, `C-r` or `C-l` runs cargo check, build, test, run or clippy.
- `M-g n` and `M-g p` visit each compiler error and warning in turn. In `*compilation*`, `RET` or a click visits the one at point and `n`/`p` show the next or previous without leaving.
- Line numbers beside buffers that visit files, with the current line's number brighter. `line-numbers = "relative"` counts from the current line and `"off"` hides them; `M-x display-line-numbers-mode` toggles them in one buffer. Clicking a number selects its line, and dragging over numbers selects lines.
- Settings: `M-x customize` lists every setting with its value. `RET` or a click changes one at once, `C-x C-s` writes the changes to `config.toml` keeping its comments, and `g` reverts.

### Changed

- The project is now called oxmacs: the binary, window title, `*oxmacs*` splash, `about-oxmacs`, `kill-oxmacs`, `save-buffers-kill-oxmacs` and `oxmacs-doctor`. Settings now live in the `oxmacs` folder of the platform config directory.
- Animations are about a third quicker.

## [0.11.0] - 2026-10-18

### Added

- Major modes: `.rs` files open in rust-mode, `.toml` files and `Cargo.lock` in toml-mode, and `.md` files in markdown-mode. The mode line shows the mode, and `rust-mode`, `toml-mode`, `markdown-mode` and `text-mode` switch it.
- Syntax highlighting through tree-sitter, in colors from GNOME's Adwaita schemes. Markdown highlights inline markup, and fenced Rust and TOML code in their own colors.
- Indentation: `TAB` indents the line or selection, `RET` indents the new line, closing brackets re-indent, and `C-M-\` indents the region. TOML and Markdown keep the previous line's level and indent inside arrays. In plain text, `TAB` inserts a tab.
- Comments: `C-x C-;` and `s-/` toggle comments on the line or selected lines, and `M-;` adds a comment or moves to one.
- Brackets: the bracket matching the one at point is highlighted. In Rust and TOML, brackets and quotes pair as you type, step over their closers and wrap the selection, `DEL` removes an empty pair, and `RET` between braces opens a block.
- `rust-format-buffer` (`C-c C-f`) runs rustfmt with the project's edition; `format-on-save = true` formats Rust files when saving.
- Structural motion: `C-M-a`/`C-M-e` move by item, TOML table or Markdown section, `C-M-f`/`C-M-b` move over balanced expressions, and `C-M-u`/`C-M-d` move out of and into brackets.
- `imenu` (`M-g i`) jumps to a function, type, impl, TOML table or Markdown heading.
- Markdown: `M-RET` continues lists, and `M-<left>`/`M-<right>` promote and demote headings and list items.
- `mx-doctor` reports the environment, config, font, each language's features and the tools mx uses.
- A clock at the right of the mode line. `clock = false` hides it, and `clock-format` takes a strftime format such as `"%a %H:%M"`.

## [0.10.0] - 2026-10-18

### Added

- Incremental search: `C-s`/`C-r` search as you type, and pressing them again moves to the next or previous match, wrapping around after a "Failing" stop. `RET` stays, `C-g` goes back, `DEL` steps back, and any other key ends the search and runs. All-lowercase queries ignore case.
- Searches highlight every match and show a `3/17` count. `C-w` adds the next word, `C-y` pastes into the query, and `C-s C-s` repeats the last search.
- Regexp search with `C-M-s`/`C-M-r` (Rust regex syntax); `M-r` switches a search between regexp and literal.
- `query-replace` (`M-%`) and `query-replace-regexp` (`C-M-%`, with `\1` groups) work from point or within the selection. `y`/`SPC` replaces, `n`/`DEL` skips, `!` replaces the rest, `.` replaces one and stops, `^` goes back, and `q`/`RET` stops. One `C-/` undoes the whole replace, and `M-%` during a search replaces its query. Lowercase queries keep each match's case, so `Foo` becomes `Bar`.

### Changed

- The `*mx*` splash lists `C-s` for search.

## [0.9.0] - 2026-10-18

### Added

- `kill-mx` exits at once without offering to save, like Emacs's `kill-emacs`; `C-x C-c` still asks.
- `about-mx` shows the read-only `*mx*` splash: version, tagline and a cheatsheet of essential keys taken from the keymap. `n`/`p`/`TAB` move between keys, `RET` or a click runs one, `q` buries it. Plain `mx` opens on it; `splash = false` in `config.toml` turns it off.
- `load-theme` picks a theme from a list (or `system` to follow the OS light/dark setting again), and `toggle-theme` flips between a theme's light and dark variants; both last for the session.

### Changed

- Long lines wrap onto continuation rows marked with `\`, like Emacs; `C-n`/`C-p`, the mouse wheel and clicks work by screen rows.
- The cursor glides only on real jumps; it snaps on typing, one-line and one-character moves, key repeat, mouse clicks and scrolling, so it never trails behind. While gliding it is drawn behind the text, so no letters vanish under it.
- The cursor covers the whole glyph under it, two cells wide over wide characters and `^M`-style controls, with the glyph shown inverted.

### Fixed

- Scrolling with the mouse wheel no longer drags point to the ends of short lines; it keeps its column across scrolls and into the next `C-n`/`C-p`.
- Vertical motion keeps the screen column across lines with tabs and wide characters.

## [0.8.0] - 2026-10-18

### Added

- `dired` (`C-x d`) lists a directory in a read-only buffer: size, modification time and name, directories first, a `..` line on top; the prompt offers only directories.
- Directories open in dired from `C-x C-f` (`RET` on a bare directory or a directory path), from `mx DIR`, and with `dired-jump` (`C-x C-j`), which lands on the current file (or goes up from a listing).
- In dired, `n`/`SPC`/`C-n` and `p`/`C-p` move between names; `RET`/`f`/`e` (or a double-click) visit the file at point or move the listing into the directory; `^` (or `RET` on `..`) goes up with point on the directory you left. One listing buffer is reused while navigating, and `M-x` offers dired commands only in listings.
- In dired, `g` (or `revert-buffer`) rereads the directory, `s` cycles name / newest / largest order (shown in the header), `(` hides details and `q` buries the listing; point stays on its file.
- Dired listings refresh by themselves when their directory changes (and when the window regains focus), keeping marks and point.
- Dired marks: `m` marks, `d` flags for deletion, `u` unmarks, `DEL` unmarks backward, `U` clears every mark and `t` toggles marks; marks survive refreshes.
- Dired deletion: `x` deletes flagged files and `D` the marked files (or the one at point), after a y/n question naming them. Files go to the OS trash; `delete-to-trash = false` in `config.toml` deletes them for good, after you type `yes`.
- Dired `R` renames or moves and `C` copies the marked files (or the one at point) through the path prompt; a directory target receives them, copies are recursive and keep modification times, each conflict asks `y`, `n`, `!` or `q`, and buffers visiting renamed files follow them.
- Dired `+` creates a directory (and any missing parents) and moves point to it.
- Dired `W` opens the marked files (or the one at point) with the system's default application.
- Dired colors: the header, directory names and symlinks have their own colors, and marked and flagged lines are tinted.
- `read-only-mode` (`C-x C-q`) toggles a buffer read-only; the mode line shows `%%` and edits are refused. Files without write permission open read-only.

### Changed

- `RET` on a bare directory in `C-x C-f` opens it in dired instead of showing `[Directory]`.

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
