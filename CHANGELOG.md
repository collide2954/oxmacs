# Changelog

Notable changes to oxmacs, called mx before 0.12. The format follows [Keep a Changelog](https://keepachangelog.com/en/1.1.0/) and versions follow [Semantic Versioning](https://semver.org/).

## [Unreleased]

### Fixed

- Checking an Ox file never touches the outside world: file, process, network and HTTP calls, `print` and workers are refused while checking, and a module that needs them to load is skipped. Checks run beside the editor instead of holding it up, stop after 2 seconds with a warning where they stopped, and load imported files again when they change on disk.
- Ox code that runs too long no longer freezes the editor: views, highlighters and take-over frames stop after 250 ms, and init files, plugin loads and the REPL after 10 seconds, with an error in the log.
- Ox no longer brings the editor down: printing an atom or promise that holds itself shows `#<atom ...>`, deeply nested values print and travel through channels and workers without overflowing the stack (values nested more than 256 levels deep are refused with an error), error messages show at most 200 characters of a value, a native that panics raises an error instead, and images are limited to 32,768 pixels a side and 134 megapixels.
- Saving through a symlink writes the file it points to and keeps the link; a file with hard links, one another program holds open, or one in a folder that can't take new files is written in place; and on Unix saving keeps the owner. A file opened again through a symlink or in other letter case reuses its buffer.
- Answering a revert, reread, save-over or kill question acts on the buffer it asked about, even if another window was chosen meanwhile, instead of whichever buffer had moved into its place.
- Ox reads files that start with a byte order mark, as Windows tools often save them, instead of failing on the first form.
- `ox --help` lists the subcommands and `ox --version` prints the version, instead of looking for a file called `--help`.
- `ui/column` and `ui/row` take `:spacing` after their children as well as before, so the calendar's week view draws again.
- A plugin you are writing starts the first time you load one of its files with `C-c C-k`, without restarting oxmacs, and `C-x C-e` in a plugin's files evaluates in that plugin's module, so its own definitions are there.
- Ox highlighting is complete: calls, namespaced names such as `ui/text`, fields such as `entry.title`, quoted symbols, `@` and quasiquotes, docstrings and command summaries, and `{…}` inside strings each get their face, and strings and docstrings that span lines no longer turn the lines after them into code.
- Plugins that start with oxmacs load just after the window opens, so boot stays at about 5 ms with the calendar's reminders on.
- Typing is fast again: a key takes about half a millisecond in an ordinary file, down from 13, because oxmacs no longer looks for a missing language server on every keystroke; it looks again after 30 seconds or when you install one.
- Typing in very large files no longer waits for the whole file to be parsed again after each key, down from 655 ms to 6 ms a key in a 20,000-line Rust file. Syntax highlighting catches up a quarter of a second after you stop typing.
- Ox timers no longer wake oxmacs ten times a second: while plugins only sleep, the window waits for the next timer.
- Errors in code passed through an Ox macro, such as the body of a `defcommand`, point at that code instead of the macro call.
- Pressing `g` repeatedly in dired no longer makes the cursor jump to the start of the line and back; plugin buffers keep point where it was when they redraw, and marks made while a listing refreshes are kept.
- `swap!` no longer loses updates when tasks switch in the middle of it.
- Tracebacks of errors in Ox tasks no longer end with a frame at `?:0:0`.
- `C-x C-e` inside an unfinished Ox list evaluates the expression before point instead of failing on the unfinished list.
- Git status keeps the path you opened it from instead of the one Git resolves through symlinks, so visiting a file from it doesn't open a second buffer for a file that is already open, as happened on macOS.

### Removed

- The design notes under `docs/`. The README and the Ox manual on `C-h i` document oxmacs and Ox.
- `M-x oxmacs-doctor`. `M-x list-tools` and `M-x list-languages` cover what it checked.
- The network connections core kept for the old IRC applet, unused since `ox/net` replaced them; oxmacs-core no longer depends on rustls and webpki-roots.
- `config.toml`. oxmacs no longer reads it, and the toml and toml_edit crates are gone; settings move to `(settings {…})` in `~/.oxmacs/init.ox`. The old file is left where it was.
- What was left of the applet system: the applet and GUI applet interfaces, command-line subcommands, applet effects and events, applet tables in `config.toml` with their settings in `M-x customize`, and the one-time moves of old weather, IRC, Atlas and dired settings into those tables. Plugins use the same building blocks under plugin names. Tables in `config.toml` are left alone.
- The unfinished routines parser, superseded by Ox.
- The applets and their Cargo features: dired, markdown and weather, which were built by default, and animated-splash, atlas, database, demo, extra-themes, git, glow, irc, pdf, python and scripting. They return as Ox plugins.

### Security

- Updated rustls-webpki and webpki-roots, which check the certificates of IRC servers and downloads, for RUSTSEC-2026-0381 and RUSTSEC-2026-0382.

### Changed

- Closing the window asks about unsaved files, as `C-x C-c` does. `auto-save` now keeps recovery copies of every file instead of saving Rust files in place; the new `auto-save-visited` saves files in place when idle.
- `await` gives back any value that isn't a promise, so code can await whatever a function returns.
- `M-x list-processes` is a bundled Ox plugin. `s` and `S` sort, `/` filters, `TAB` shows a process's command, folder, status and environment, `k` asks it to end and kills it three seconds later if it hasn't, `K` kills it at once, and a sparkline shows its CPU over the last minute.
- Settings are Ox: `(settings {theme: :adwaita-dark tab-width: 4})` in `~/.oxmacs/init.ox` sets them, and plugins take theirs from `use-plugin`'s `:config`. `custom.ox` and `init.ox` run before the window opens, so the theme, font and window mode apply from the first frame, and saving either runs both again from fresh settings. `M-x find-init-file` replaces `find-config-file`.
- `M-x customize` is a bundled Ox plugin and lists plugin settings too. `C-x C-s` writes `~/.oxmacs/custom.ox`, which runs before `init.ox`, so `init.ox` wins; a setting `init.ox` overrides says so.
- oxmacs is open source under the Apache License 2.0.
- Ox and its `ox` command share the oxmacs version number.
- Ox code runs two to three times faster: function lookups are cached, calls and arithmetic do less work, comparisons and hashing no longer allocate, and `map`, `filter`, `keep`, `take`, `drop` and `distinct` build their results without copying. A frame of the matrix screensaver takes about half as long.
- The Game of Life computes a generation five times faster on a random grid and three and a half times faster once the acorn has grown: cells are whole numbers counted by sorting.
- Ox runs `+`, `-`, `*`, `<`, `<=`, `>`, `>=`, `=` and `nth` on two values as single instructions, so loops that count, compare and index run two to three times faster. Redefining or shadowing these names still works.
- `M-x list-tools` is a compact table: one line per tool, grouped by what uses it (R, Python, PDF), with the version that runs and where it comes from, and the latest release; `TAB` opens a tool's summary, the full path it runs from and its source.
- `RET` leaves the line it ends alone in languages that keep the previous line's indentation, such as TOML and Markdown, instead of re-indenting it to match the line above.
- Weather settings now live in a `[weather]` table: `location`, `units` and `mode-line`. oxmacs moves `weather-location`, `weather-units` and `mode-line-weather` there the first time it starts, keeping comments, and says so.
- Weather is a default applet; `cargo build -p oxmacs --no-default-features` leaves it out.
- IRC settings now live in an `[irc]` table: `[[irc-server]]` becomes `[[irc.server]]` and `[irc-bouncer]` becomes `[irc.bouncer]`, moved the same way the first time oxmacs starts.
- IRC is no longer part of every build; it is the opt-in `irc` applet below.
- Atlas vaults now live in an `[atlas]` table: `[[atlas-vault]]` becomes `[[atlas.vault]]`, moved the same way the first time oxmacs starts.
- Atlas is no longer part of every build; it is the opt-in `atlas` applet below.
- Markdown is a default applet: its mode, highlighting, list and heading keys and the rendered preview live in their own crate, and `--no-default-features` leaves them out.
- Dired is a default applet: listings, marks and file operations live in their own crate, and `--no-default-features` leaves them out. Without it, opening a directory opens `C-x C-f` there.
- `delete-to-trash` now lives in a `[dired]` table, moved the same way the first time oxmacs starts.
- Start-screen lines from applets sit above Save and Quit.
- `M-x` and other completion lists respond faster.
- `rust-format-buffer` and `r-format-buffer` are now one command, `format-buffer`, still on `C-c C-f` in Rust and R buffers.

### Added

- Unsaved files get recovery copies in the oxmacs cache folder, never beside the files themselves, and opening a file whose copy is newer offers to recover it; saving, reverting or killing the buffer removes the copy. A crash writes a log there too.
- `ui/image` shows an `ox/image` image in a plugin view, and `open-view` takes `:beside` and `:select` like `open-buffer`.
- Plugins can list the processes oxmacs runs with `processes`, follow them with `on-processes-change`, end them with `stop-process` and `kill-process`, and sample their usage with `watch-process-usage`. `open-buffer`'s `:select true` moves to a buffer it opens beside.
- `ox/image`, images in Ox: `image/open` reads PNG, JPEG, WebP, GIF, BMP, TIFF, ICO, QOI and SVG, turned upright by their EXIF orientation, `image/probe` reads a file's size and format without decoding it, `image/save` writes any of them but SVG with a JPEG `:quality`, and `image/new` makes a blank image. Geometry covers resizing with five filters, fitting, filling, thumbnails, crops, padding, canvases, rotation by any angle, flips, trimming and affine and perspective warps, and `image/pixel` reads one pixel. Color work follows CSS filters: brightness, contrast, gamma, exposure, hue, saturation, vibrance, grayscale, sepia, invert, tint, white balance, levels, curves, posterize, threshold with Otsu, color replacement, and quantizing to a palette or a median-cut count with dithering, spread over every core with rayon. Filters cover Gaussian, box and median blurs that keep transparent edges clean, sharpening and unsharp masks, Sobel and Canny edge maps, embossing, pixelating, repeatable noise, vignettes and custom convolution kernels. Drawing works on a private copy inside `image/edit`, or on any image as a new one: antialiased rectangles with rounded corners, ellipses, circles, polygons, lines and polylines with fills, strokes, dashes, opacity and blend modes, single pixels, and pasting images with all sixteen CSS blend modes, opacity, scale and masks; `image/mask` and `image/tile` round it out. Built on tiny-skia. Colors are hex strings, vectors, CSS names or theme faces. Slow work runs off the editor's thread and returns promises, and image functions wait for promises they are given, so `(-> (image/open path) image/width)` needs no `await`. Built on the image, resvg, png, imageproc, kamadak-exif and ab_glyph crates.
- Fedora 44 and 45 packages, built by CI. A version tag publishes them to a dnf repository on GitHub Pages and attaches them to the release; every push to master publishes to a separate nightly repository. The package installs `oxmacs`, `ox` and a desktop entry, and on Linux the window carries the app id `io.github.collide2954.oxmacs`, so desktops match it to the entry.
- A hands-on tutorial and the Ox manual, in the bundled `manual` plugin. `C-h t` opens your own copy of the tutorial where you left it: moving, editing, files, windows, search, a first Ox command and your configuration. `C-h i` opens the manual, an Info-style reader with a guide to the language, extending oxmacs, writing plugins and the `ox` command, and a reference of every library module and the editor API built from their documentation. `RET` follows links, `l` and `r` go back and forward, `n` and `p` turn pages, `s` searches, `i` looks up a name and `x` runs an example in the REPL. Every example in the guide is tested.
- A `markdown` plugin. `M-x markdown-preview` shows a Markdown buffer rendered beside it and wrapped to the window, with headings, lists, `code`, highlighted Ox examples and links that `RET` follows, and refreshes when the file is saved. Other plugins render Markdown with its `(markdown render)` module. Plugins can also ask for the reference of the Ox library and the editor API with `ox-reference`, highlight Ox code with `ox-highlight` and run code in the REPL with `run-in-repl`.
- A remote file browser instead of TRAMP, the bundled `remote` plugin. `M-x remote` lists the hosts in `~/.ssh/config` and the ones you save, and opens a dired-like browser on each over ssh, sftp or ftp, or through any command that opens a shell, such as `docker exec` or `wsl`. Files open as local copies that go back when saved, `C` and `U` copy files either way, `!` runs a command in the folder shown, `/` searches the files below it and `s` opens a shell there. One connection stays open per host, and logging in is left to ssh and its keys. `(ox file)` gained `file/read-base64` and `file/write-base64` for binary files.
- Conway's Game of Life, the bundled `life` plugin, off until `(use-plugin life)`: `M-x life` draws an acorn growing, with `SPC` to pause, `n` to step, `r` to start again and settings for the pattern and speed. Bundled plugins can be opt-in with `:opt-in true` in their manifest.
- A calendar, the bundled `calendar` plugin. `M-x calendar` shows an agenda of the coming two weeks, `m` a month and `w` a week as a grid of hours. Events are `.ics` files, one per event, in folders under `~/.oxmacs/calendar`, with time zones, repeats (`RRULE` with exceptions), reminders in the echo area and on the desktop, invitations you can accept, decline or answer maybe with a reply file, and CalDAV sync whose password comes from the vault.
- `ui/canvas` takes `:relative true` for drawings that fill the width they get, and `time/zone` names the local time zone.
- An IRCv3 client in the style of ERC, the bundled `irc` plugin. `M-x irc-setup` adds a network, keeping its SASL password in the vault, and `M-x irc` connects. Messages line up beside a nick column and wrap under themselves, with colored nicks, mIRC formatting, links, history from the server, replies, reactions, read markers, mention highlights and desktop notifications, a nick list on `C-c C-n`, `C-c C-SPC` for the next buffer with news, joins and parts only for people who spoke lately, multiline pastes after asking, and searchable logs. It has no bouncer of its own.
- Plugin buffers can have an input line, as in a chat: `open-buffer` takes `:input` and `:show`, and `append-lines`, `take-input`, `set-input`, `input-text`, `input-point`, `current-column` and `line-text` are new. Plugin settings can be `:list`s, which `M-x customize` shows and leaves to `init.ox`.
- `(ox time)` works with time zones and the calendar: `time/instant` reads moments and civil times, `time/local` takes a zone, `time/add` adds days or months across daylight saving changes, `time/format` writes moments with strftime patterns, and `time/days-in-month`. `file/append` adds to files.
- `C-a` on the input line of a REPL, shell or chat stops after the prompt.
- A password vault, the bundled `vault` plugin. `M-x vault` lists your logins in `*Vault*`; the vault is one file, `~/.oxmacs/vault.json`, sealed with XChaCha20-Poly1305 under a key Argon2id derives from your master password, and it locks after 10 minutes unused or on `M-x vault-lock`. `c` copies a password, which skips the kill ring and leaves the clipboard after 30 seconds, `t` copies the current one-time code, `RET` shows an entry with its code counting down, and `M-x vault-generate` makes passwords, passphrases and PINs. `M-x vault-import` brings in KeePass databases, Bitwarden and 1Password CSV exports and the pass store, and other plugins ask it for their logins. New crate: keepass.
- In path prompts, typing a new root after a separator starts the path over, as in Emacs: `//` goes to the root, `~/` home and, on Windows, `D:` that drive.
- Ox modules for the coming plugins: `(ox net)` connects over TCP or TLS and streams lines through a channel, `process/start` runs a program with its output and errors as channels and input you write to, `(ox crypto)` has Argon2id keys, XChaCha20-Poly1305 sealing, TOTP codes, random bytes, SHA-256 and base64, `http/request` sends any method with headers, a body and a login, and `(ox csv)` reads comma-separated text. Plugins can also copy a secret without it entering the kill ring, show desktop notifications and ask whether the window has focus. New crates: argon2 and chacha20poly1305.
- Language servers. Opening a file starts its server when it is installed (rust-analyzer, pyright, taplo, marksman, clangd, gopls or typescript-language-server), in the project's root, with its state in the mode line. Diagnostics as you type, completion, the signature at point, `C-c C-d` docs, `M-.` definitions, `M-?` references in `*xref*`, `<f2>` rename, `C-c .` code actions, formatting, and inlay hints at the end of lines. `M-x list-tools` lists the servers and offers to install missing ones, asking first; `M-x lsp-restart` and `M-x lsp-log` help when one misbehaves; `language-servers` and `inlay-hints` turn them off.
- `ox lsp`, a language server for Ox in other editors such as Helix and Neovim: checker findings as you type, completion, hover docs, signature help, go to definition, references, rename, formatting and document symbols. It uses the `lsp-server` crate.
- `ox fmt` formats Ox code in the Clojure style, keeping your line breaks, and `ox fmt --check` fails in CI when a file needs it. In oxmacs, `TAB` and `RET` indent Ox the same way, `C-c C-f` formats the buffer, and saving an `.ox` file formats it.
- In Ox buffers and the Ox REPL, the echo area shows the signature of the call at point with the first sentence of its doc, and `C-c C-d` opens `*ox doc*` for the name at point: what it is, where it is defined, its signature, its doc and the tests named after it as examples.
- Every function, macro and special form that Ox and the editor provide has docs.
- Completion of Ox names from the running image in Ox buffers and the REPL: `C-M-i`, `TAB` after a name, and a quiet popup once three characters of a name are typed.
- `M-.` goes to where an Ox name was defined, in your files, plugins or the prelude, and `M-,` comes back; `M-?` lists its usages across open Ox buffers, the current folder and the oxmacs home.
- Interactive Ox in the style of CIDER. `C-x C-e` evaluates the form before point and `C-M-x` the top-level form around it, showing `=> value` beside the form until you edit the buffer; `C-c C-b` evaluates the buffer and `C-c C-k` saves and loads the file. Evaluation runs between keystrokes, so `C-c C-c` stops a runaway loop. Errors show `!! message` beside the form, and errors from calls open `*ox stacktrace*`, where `RET` visits a frame's source and `q` closes it.
- Ox files are checked half a second after you stop typing, with `ox check`'s findings underlined, echoed at point and counted in the mode line.
- Docstrings in Ox: a string first in a `def` or `defmacro` body with more after it describes the name, as in `(def (area w h) "Width times height." (* w h))`.
- `ox check` finds mistakes in Ox code without running it: undefined names, suggesting a close one; calls with the wrong number of arguments or unknown keyword arguments; unused bindings and imports, unless named with a leading `_`; and malformed forms. It prints like rustc, or with `--format json` and `--format github` for CI, fails on errors, and on warnings too with `--strict`. Plugins and init.ox are checked with the editor's functions known.
- Profiling for Ox code: `M-x ox-profiler-start`, `M-x ox-profiler-stop` and `M-x ox-profiler-report` show where the time went, grouped by plugin, as a call tree or as functions by self time, with the slowest editor ticks; `TAB` folds, `f` switches tables and `RET` visits a function. `ox profile FILE` prints the same report, and `(profile f)` returns it as data.
- `ox perf bench` benchmarks Ox code with its time and allocations, and `ox perf sample`, on Windows, samples the VM's native stack to show where its time goes.
- Screensavers, a bundled plugin that is off until you set `(use-plugin screensaver :config {idle: 300})`: after that many seconds without input, while oxmacs has focus, a starfield or matrix rain fills the window area, keeping the mode line and minibuffer, until the first key or click, which does nothing else. `kind` picks `:starfield`, `:matrix` or `:random`, and `M-x screensaver-preview` shows either at any time.
- Ox can draw rectangles, circles, lines and text in theme or hex colors, in a view with `canvas` or over every window with `take-over`, and can run code after a stretch without input with `on-idle`. `prompt` takes `:choices` to complete from names with notes.
- Ox tests: `(test "name" (expect …))` blocks sit next to the code they test, are skipped when a file or plugin loads, and run with `ox test PATH…`, which prints each result and explains a failed `expect` with the value of each argument and where it failed.
- Ox gains `(atomically …)`, which runs its body without letting another task in, and `pr-str`.
- A plugin manifest can name its file modes with `:modes`, so opening such a file wakes the plugin; a `COMMIT_EDITMSG` opened from a terminal `git commit` gets the Git commit mode.
- Ox gains `take`, `drop`, `take-while` and `drop-while` (which read generators lazily), `iterate`, `partition`, `chunk`, `flatten`, `find`, `frequencies`, `group-by`, `zip`, `interleave`, `repeat`, `get-in`, `assoc-in`, `update-in`, `merge`, `merge-with`, `select-keys`, `juxt`, `memoize`, `keyword`, `name`, `symbol`, `type`, `some->` and `as->`; `string/words`, `string/reverse` and `string/truncate`; raw strings, `#r"…"` and `#r#"…"#`; regex literals, `#"…"`; and the `ox regex` module with `matches?`, `find`, `find-all`, `replace` and `split`.
- Git shows the diff of the file at point beside the status, following point. `t` switches it between unified and side by side, which refits to the window, and changed words stand out. `s`, `u` and `k` work on hunks and on the region's lines, in the status and in the diff, and `RET` in the diff visits the line. `c` opens the commit transient (`-a`, `-e`, `-n`, `-s`; commit, amend, extend, reword), whose message buffer shows the staged changes beside it, flags a summary over 50 characters, and takes `C-c C-c`, `C-c C-k`, `M-p` and `M-n`.
- Plugins can define file modes with keys and highlighting (`defmode`) and transient menus (`transient`), open buffers beside the current one that follow point and width, and read and replace the current buffer's text. Ox gains `distinct` and `compare-and-set!`.
- Git is back as a bundled Ox plugin, starting with the status: `C-x g` shows the branch and its upstream with ahead and behind counts, untracked, unstaged and staged files with status letters and +/− counts, stashes and recent commits. `n` and `p` move, `TAB` folds sections and shows a file's hunks in place, `RET` visits the file at the line shown, and `g` refreshes; the status also follows `.git`, the work tree and saved files. `s` stages, `u` unstages and `k` discards files and whole sections, asking first. Discarded untracked files go to the trash unless you set `(use-plugin git :config {discard-to-trash: false})`.
- Ox can run programs with `ox process` (`process/run` and `process/env`) and read numbers with `string/to-number`, and `string/split` takes a limit. Plugin text buffers can show pills, watch another folder with `:watch` and keep their names, and plugins can follow saves with `on-saved`.
- Dired is back as a bundled Ox plugin: `C-x d` or opening a directory lists it with sizes and dates. `(` hides them, `s` sorts by name, newest or largest, `RET` and `^` move between directories, and `C-x C-j` lists the current file's directory. `m`, `d`, `u`, `t` and `U` mark and flag; `x` and `D` delete, `R` renames or moves, `C` copies, `+` makes a directory and `W` opens with the usual app. Listings follow changes on disk. Turn off the trash with `(use-plugin dired :config {delete-to-trash: false})`; the `[dired]` table in `config.toml` is no longer read.
- Plugin manifests can bind global keys, add start-screen lines and open directories, and plugins can follow disk changes, open files and ask for a path with `prompt-path`. Ox gains `sort`, `sort-by` and `compare`.
- Plugins can show text buffers with styled lines and their own keys (`open-buffer`, `set-lines`, `current-line`, `goto-line`), and ask with `(await (prompt "Rename to: " :initial name))` and `(await (yes-or-no "Delete?"))`, where `C-g` raises `Quit`. Ox commands and view buttons run as tasks, so waiting on a prompt or the network never holds up typing.
- Working on Ox in oxmacs: `.ox` files open in Ox mode, with highlighting and `;` comments. `C-x C-e` evaluates the form before point and `C-c C-b` the whole buffer, showing the result in the echo area. `M-x ox-repl` (`C-c C-z` in Ox mode) opens a REPL with `M-p` and `M-n` history. Saving `init.ox` runs it again, and saving a file of one of your plugins reloads that module, keeping `defonce` state and redrawing its views.
- Weather is back as a bundled Ox plugin: `M-x weather` shows the current conditions, the next 24 hours and the next 7 days from Open-Meteo, `g` refreshes and `q` quits, and with `mode-line` on the current weather sits beside the clock, refreshed every 30 minutes. Set it up in `~/.oxmacs/init.ox` with `(use-plugin weather :config {location: "Berlin" units: :imperial mode-line: true})`; the `[weather]` table in `config.toml` is no longer read.
- The beginnings of Ox, the extension language that will carry oxmacs's plugins (see docs/ox-design.md). Its `ox` command-line tool runs `.ox` files and opens a REPL; so far Ox has numbers that grow into bignums, strings with interpolation, `def`, `fn` with optional, rest and keyword parameters, closures, tail calls, vectors, maps and sets that keep insertion order, records, atoms, hygienic macros (`defmacro`, `defrule`), `match` with guards, destructuring, errors with `try`, `catch`, `finally`, `deferror` and tracebacks, modules with `import`, `export` and live reloading, tasks that run a slice at a time and can be interrupted or cancelled, generators, promises with `await` and `spawn`, worker isolates on other threads that talk over channels, Rust values that Ox can hold, `sleep` and `every`, the `ox time`, `ox json`, `ox http`, `ox string`, `ox file` and `ox path` modules, number helpers such as `round` and `mod`, a prelude with `map`, `filter`, `reduce`, `cond` and `->`, and a garbage collector.
- `~/.oxmacs/init.ox` runs at startup (`OXMACS_HOME` moves the folder). Ox code there can define commands with `defcommand`, bind keys with `bind-key`, show messages and mode-line text, and turn on and configure plugins with `use-plugin`. Bundled plugins are on unless turned off, and those in `~/.oxmacs/plugins` load on their first command. `(open-view "*name*" view-fn :keys {…})` shows a buffer drawn with widgets from `(ox ui)` (`column`, `row`, `text` with faces, `button`, `space`, `scrollable`), redrawn when atoms change. Errors and their tracebacks go to `*ox log*`.
- `M-x list-languages` shows each language with its file extensions and what works in it; `M-x oxmacs-doctor` now only counts them and points there.
- Keyboard macros: `F3` or `C-x (` starts recording, `F4` or `C-x )` stops, and `F4` or `C-x e` plays the keys back, with `e` playing them again; `C-x C-k r` plays them on each line of the region and `F3` while recording inserts a counter. A replay stops at the first error and undoes as one step, and the mode line says when a macro is being recorded.
- Atlas, a plain-text knowledge system in `.atlas` notes: a `key: value` header, `=` headings, tasks `[ ]` `[/]` `[?]` `[x]` `[-]` with `due:`, `on:`, `every:`, `!high` and `#tags`, `[Page]` links and `@date HH:MM-HH:MM` events, all highlighted. Loose dates such as `fri` or `+3d` become ISO dates when you leave the line or save.
- `C-c C-c` in a note cycles the task at point, stamping `done:` and moving repeating tasks on; `C-c C-o` follows a link or starts the page, and `M-,` comes back.
- Vaults are `[[atlas.vault]]` entries in config.toml, each with a name, folder and color. The first `C-c n` command offers to make one at `~/Atlas`.
- `C-c n f` finds or starts a note, `C-c n t` opens today's journal, `C-c n c` captures a line or task into it, and `C-c n b` lists the lines that link to the current note.
- `C-c n a` shows the agenda across all vaults: overdue work, today, the next seven days and waiting tasks, each tagged with its vault. `t` cycles a task, `s` and `d` set its dates, `v` filters by vault and `RET` visits.
- `C-c n m` shows a month calendar with each day's entries and the selected day's details below; `w` switches to a week laid out by hour.
- `M-x oxmacs-doctor` lists Atlas vaults and how many pages each holds.
- `M-x oxmacs-doctor` lists the opt-in build features compiled in, with their versions.
- The theme in *Settings* is picked from the same live-preview list as `M-x load-theme`.
- Applets can draw the whole window of a buffer of their own; the Markdown preview is drawn this way.
- Applets can restyle the minibuffer line while a prompt waits.
- Applets can give their languages a guess at each line's indentation, which `TAB`, `RET` and `C-M-\` follow.
- Applets can give their languages a formatter, used by `format-buffer` (on `C-c C-f` in that language's buffers) and format-on-save, and a linter that checks files on save with the same underlines, mode-line counts and `*Diagnostics*` as cargo.
- Applets can run commands in `*compilation*` and teach it the error locations their tools print.
- `M-x list-tools` says when an applet's tool runs from a setting in the applet's table, such as `[python] ruff`.
- Applets can say which parts of their language's syntax hold statements, so sending the statement at point to a console works for their languages as it does for R.
- Applets can start REPL consoles like the R console, with input history, echoed code, hidden requests and keys of their own instead of the R console's.
- Applets can run background jobs and interactive sessions of their own, listed in `M-x list-processes`.
- Applets can open the file types they claim, show their own position in the mode line and have their file watched for changes.
- `M-x list-tools` can manage tools applets add, including libraries, checked against the digest GitHub publishes for each download.
- Applets can list a directory in a buffer, ask for a path, a typed yes or no or one of several letters, hear about changes on disk and directories being opened, and move files along with the buffers visiting them. Double-clicking in an applet buffer runs what `RET` runs there, and `M-x revert-buffer` runs what `g` runs.
- Applets can add languages: the file types they open, the mode-line name, a keymap and a highlighter or a tree-sitter grammar with its queries. Rust, TOML, Markdown and R are described the same way, and Markdown code blocks are highlighted in any language the build knows.
- Applets can tint lines as added or removed, in soft shades of each theme's success and error colors.
- Applet buffers can show a directory under a name of their own and have a second folder watched for changes.
- Applets can copy text of their own to the clipboard and the kill ring.
- Applets can add commands whose names are only known at run time, replace or remove them, and look up any command by its name.
- Applets can open transient popups, Magit's key menus: `-` and a letter toggle a switch, a single key runs an action with the switches that are on, and `q` or `C-g` closes it. The panel sits above the echo area, and switches are remembered until oxmacs quits.
- Applets keep their settings in a table of their own in config.toml, such as `[demo]`. A table no applet in the build reads is kept, and `M-x oxmacs-doctor` points it out. Applet settings appear in `M-x customize` under the applet's name.

### Experimental (opt-in at build time)

- `database` 0.1.0: opens SQLite databases from a `.db`, `.sqlite`, `.sqlite3` or `.db3` file, `M-x database-open` (recent databases first), `M-x database-create` and `M-x database-scratch`, which lives in memory. The schema lists tables with their row counts, views and indexes, and `TAB` shows a table's columns with their types, keys, defaults and foreign keys. `RET` shows a table's rows beside it in a drawn grid with row numbers, zebra rows, numbers aligned right and NULL dimmed; rows load in pages on a background thread as you move, so large tables open at once. `C-n`, `C-p`, `C-f`, `C-b` and the arrows move by cell, `TAB` and `S-TAB` along the rows, `C-a` and `C-e` to the first and last column, `M-<` and `M->` to the first and last row, `C-v` and `M-v` by pages, and the wheel scrolls. Clicking picks a cell and shift-clicking selects; `C-SPC` starts a selection and `M-w` copies cells as tab-separated text. `g` reads the rows again. `s` sorts by the cursor's column, cycling up, down and off, and `S` adds it as a further sort key, as do clicking and shift-clicking a header, which shows ▴ or ▾, numbered when there are several; `=` keeps the rows with the value under the cursor, `!` the rows without it, and `f` picks a column and asks for the rest of a condition such as `LIKE '%rock%'`; a strip above the header names the table, counts its rows (`12 of 3,503 rows` when filtered) and shows each filter and sort as a pill whose × removes it, and `F` removes one or all of them. Build with `cargo build -p oxmacs --features database`.
- `scripting` 0.1.0: a minimal show-and-tell of an embedded language, Rhai. It is not a supported way to extend oxmacs and will stay experimental; applets remain the way to extend it. `M-x script-eval-buffer` and `M-x script-eval-region` run the buffer or the selection as a Rhai script and show its value in the echo area, or the error with its line and column. `.rhai` files open as Rhai, with `//` comments and highlighted keywords, strings and comments, and `C-c C-c` and `C-c C-r` there evaluate the buffer and the selection. `M-x script-console` opens `*Script*`, which evaluates each line you enter, prints its value or error and keeps variables and functions for the session, with scripts acting on the buffer you opened it from; `M-p` and `M-n` bring back earlier input. Scripts act on the current buffer with `insert(text)`, `text()`, `region()`, `point()`, `goto(position)` and `line()`, show `message(text)` in the echo area and `run(name)` any command. `ask(prompt, callback)` asks in the minibuffer and calls back with the answer. `command(name, summary, callback)` adds an `M-x` command that calls back into the script, and `bind(keys, name)` puts any command on a global key, both until oxmacs quits. `C-h f script-eval-buffer` lists these functions. Scripts reach only the editor, with no files, network or programs, and stop with an error when they loop too long, recurse too deeply or build strings or arrays that are too large. Nothing runs unless you ask. Build with `cargo build -p oxmacs --features scripting`.
- `demo` 0.1.0: an example applet, `M-x demo`, showing how applets add commands, keys, buffers, prompts, questions, effects, GUI panels, a config table (`[demo] sides`) and a doctor section. Build with `cargo build -p oxmacs --features demo`.
- `atlas` 0.1.0: the Atlas knowledge system above, with its notes, journal, capture, links, backlinks, agenda and calendar, as an applet of its own. Build with `cargo build -p oxmacs --features atlas`.
- `animated-splash` 0.1.0: the start screen draws the oxmacs logo in block letters, animated above the keys; `a` cycles a drifting plasma field with a shimmer, a color wave, the logo assembling from scattered pixels, a network of neurons whose signals light the letters as they pass, and DNA replicating behind the logo with its helicase, polymerases, primers and Okazaki fragments. Build with `cargo build -p oxmacs --features animated-splash`.
- `extra-themes` 0.1.0: 56 more color themes inspired by the ones Helix ships, among them gruvbox, catppuccin, tokyonight, rose-pine, kanagawa, nord, dracula, everforest, solarized, ayu, github, modus and doom-one, each tuned to stay readable. Build with `cargo build -p oxmacs --features extra-themes`.
- `git` 0.1.0: `C-x g` shows a Magit-style status of the repository: the branch and its upstream with commits ahead and behind, untracked, unstaged and staged files with colored status letters and line counts, stashes, and recent commits with their authors and how long ago they were made. `n` and `p` move between sections and files, `TAB` folds a section or shows a file's hunks inline, and `RET` visits the line shown. `s` stages and `u` unstages a file, a section, a hunk or the lines in the region; `k` discards changes after asking and moves untracked files to the trash. The diff of the file at point shows in a pane beside the status with line numbers on both sides; `t` there switches to side by side, and `n`, `p`, `s`, `u`, `k` and `RET` work there too. `c` opens Magit's commit menu: `c` commits, `a` amends, `e` extends HEAD with the staged changes and `w` rewords it, with switches for `--all`, `--allow-empty`, `--no-verify` and `--signoff`. The message is written in `COMMIT_EDITMSG` beside the staged diff, with long summaries marked, `M-p` and `M-n` for earlier messages, `C-c C-c` to commit and `C-c C-k` to cancel; hooks run as they do in a terminal. `l` opens Magit's log menu: `l` logs the current branch, `a` all local and remote branches and `o` another branch, drawn as a list in the style of JetBrains IDEs with a branch graph in colored lanes where merges join and free lanes are reused, newest first with branch and tag labels as pills, authors and how long ago each commit was made; clicking selects a commit and the wheel scrolls, and the first 1,000 commits load at once with more as you near the end; `n` and `p` move between commits and `g` reads the log again. The commit at point shows its full message and changed files below the list, and `RET` shows its diff beside the log, following the commit at point, with `t` there for side by side. `b` opens Magit's branch menu: `b` checks out a branch or revision with completion, `c` creates a branch at HEAD and checks it out, `n` creates one without checking it out, `m` renames a branch and `x` deletes one after asking, forcing the delete only when you confirm that it is not fully merged. `l` there shows the branches as a tree in the style of JetBrains IDEs, grouped into Local with commits ahead and behind each upstream, Remote, Recent and Tags; `/` filters them as you type, `RET` checks out the branch at point, a remote one as a local branch that tracks it, and `g` reads them again. The status refreshes when files are saved or the repository changes, and `$` there lists the last 50 Git commands it ran in `*git process*`, each with how it ended and what it printed. The mode line names the branch of the current buffer's repository, such as `main ↑2 ↓0 ●`, with commits ahead and behind its upstream and `●` while there are uncommitted changes. Build with `cargo build -p oxmacs --features git`.
- `glow` 0.1.0: the minibuffer glows softly in the theme's prompt color while something waits for an answer: `M-x`, file and applet prompts, y/n and yes/no questions, isearch and query-replace. The glow fades in and out, and is instant when animations are off. Build with `cargo build -p oxmacs --features glow`.
- `pdf` 0.1.0: opening a `.pdf` shows its pages, drawn by PDFium, which `M-x list-tools` installs. Pages scroll continuously or one at a time (`c`) and start fitted to the window's width (`W`; `H` fits whole pages, `+` and `-` zoom, `=` shows actual size). `n` and `p` turn pages, `M-g g` goes to one, `o` or `M-g i` jumps through the outline and `m` draws pages in the theme's colors. `C-s` searches the text with every match highlighted and counted, dragging selects text for `M-w`, links jump to their page or open in the browser (web and mail links only), and the viewer reloads in place when the file changes. Build with `cargo build -p oxmacs --features pdf`.
- `irc` 0.1.0: the IRC client, `M-x irc-setup` and the bouncer, as an applet of its own; `oxmacs irc-daemon` runs the bouncer when the build includes it. IRC waits for `M-x irc` unless a server has `autostart = true`, which connects it in the background at startup while the start screen stays up. Build with `cargo build -p oxmacs --features irc`.
- `python` 0.1.0: `.py` and `.pyi` files open in Python mode, highlighted by tree-sitter with docstrings and decorators set apart, with `#` comments, paired brackets and quotes, and classes and defs for `M-g i` and `C-M-a`. `TAB` indents as a best guess: one level deeper after a line ending in a colon, one shallower after `return`, `pass`, `break`, `continue` or `raise`, and otherwise like the previous line of code; pressing it again right away cycles through the other levels. `RET` starts the new line at the guessed level and leaves the line above alone, and `C-M-\` indents a region the same way. `C-c C-f` formats the buffer or the selected lines with ruff, and saving lints the project with `ruff check`, with the usual underlines, mode-line counts and `*Diagnostics*`. ruff and uv install from `M-x list-tools` or come from PATH, and `[python] ruff` and `uv` name other copies. `C-c C-r` runs the file and `C-c C-t` runs pytest for the project in `*compilation*`, where each line of a traceback visits the code it names. They use the project's `.venv`, looked for upward from the file, then `[python] interpreter`, then `python3` or `python` on PATH. `C-c C-z` shows a Python console running that interpreter over pipes, with input history and the code you send echoed after each prompt. `C-RET` sends the statement at point or the region and moves to the next statement, and `C-c C-c` sends the class or def around point, or the paragraph; code goes the way the console reads it, dedented, without the blank lines inside blocks and with a blank line closing each one. `C-c C-u` opens the `*uv*` dashboard for the project: its name, the Python in its `.venv`, whether `uv.lock` is up to date, and the required, locked and newer packages, checked by uv in the background; `g` reads them again. `a` adds a dependency, `d` removes and `u` upgrades the package at point, `s` syncs the `.venv` and `l` locks, each as a background `uv` job whose output shows at the bottom. The Python versions uv knows are listed too, installed or not; `i` installs and `p` pins the one at point. Build with `cargo build -p oxmacs --features python`.

## [0.13.0] - 2026-10-19

### Added

- `C-v` and `M-v`, or Page Down and Page Up, scroll by a screen.
- `window-mode` opens the window windowed, maximized or fullscreen.
- `M-x find-config-file` opens config.toml; saving it applies the settings at once.
- `mode-line-style` picks a classic, modern, minimal, powerline or boxed mode line.
- `M-x` shows each command's key and what it does, and `C-h f` explains a command in *Help*.
- `M-x weather` shows the forecast for `weather-location` from Open-Meteo, and `mode-line-weather` puts it beside the clock.
- `M-x irc`: an IRCv3 client for the `[[irc-server]]` entries in config.toml, with a buffer per server, channel and private chat, a member list, slash commands, nick completion, `C-c C-b` to jump to busy chats, and SASL passwords asked for, read from the environment or kept in the OS keychain.
- `M-x irc-setup` adds an IRC server from a short form: Libera.Chat, OFTC, EFnet or any other server, your nick, an optional account, where the password is kept, channels, and whether the bouncer keeps you connected. `C-c C-c` writes it to config.toml and offers to connect; `M-x irc` offers it when no servers exist.
- IRC can outlive the editor: `[irc-bouncer] mode = "local"` keeps a built-in bouncer running in the background, and `mode = "remote"` uses one started with `oxmacs irc-daemon` on another machine. Messages you missed are replayed when you come back.
- R mode for `.R` and `.r` files: highlighting, tidyverse-style indentation with hanging arguments and pipes, `#` comments, paired quotes and backticks, motion by function, `M-g i` with RStudio sections, `M--` for ` <- ` and `C-S-m` for ` |> `.
- `M-x run-r` opens an R console beside the script, with input after the prompt and `M-p`/`M-n` history. R is found on its own, or set `r-program`.
- R scripts send code to R: `C-RET` sends the statement around point or the selection and moves on, `C-c C-c` sends the function or paragraph, and `C-c C-z` switches between a script and its session. Sent code shows in the console as R reads it, and R starts when needed.
- R completion: `TAB` or `C-M-i` asks the running session, and a popup also appears after three typed characters.
- `C-c C-v` shows R help in *R help*, as does `?topic` in the console.
- R plots appear in a window below the console, redrawn at its size: `n` and `p` browse them, `s` saves, `d` deletes, `z` opens a large copy, `v` shows them all, and `k` pins one in its own window.
- Several R sessions can run at once with `M-x r-new-session`, each named after its folder; a script sends to the session it last used.
- `C-c C-e` lists the session's objects; `RET` opens a data frame as a table whose header stays put, or anything else with `str()`.
- `M-x list-processes` lists the processes and connections oxmacs runs, with their CPU and memory; `RET` visits one and `k` stops it.
- R formatting with air: `C-c C-f` formats the buffer, or only the selected lines, and `format-on-save` covers R too. The `air-program` setting picks the program; otherwise oxmacs's own copy, then the one on your PATH.
- While R is idle, the echo area shows the arguments of the R call around point, such as `lm(formula, data, subset, ...)`.
- `M-.` jumps to an R definition in open buffers, the project's R files, or the running session (a sourced file, or the function's source in `*R source: name*`); `M-,` goes back.
- `M-x list-tools` lists the tools oxmacs can install itself, air for now, with the installed and latest versions. `i` installs, `u` updates and `d` removes; downloads are checked against their published SHA-256. It only goes online when opened or on `g`.

### Changed

- `M-x load-theme` starts on the theme in use and previews the selected theme across the whole editor; `C-g` puts the old one back.
- Switching themes fades 20% faster.
- `M-x oxmacs-doctor` opens at once and fills in its tools as they answer; `g` keeps the last results while it checks again.
- R starts about 200 ms sooner.

### Fixed

- Markdown previews scroll with the keyboard and can be selected with a click.
- Private IRC notices show in the server buffer instead of opening chats, and IRC formatting codes no longer show as text.
- Wide characters such as CJK and emoji wrap inside their column.

## [0.12.0] - 2026-10-18

### Added

- Windows: `C-x 2` and `C-x 3` split the selected window, `C-x o` and `C-c <arrow>` move between windows, and `C-x 0` and `C-x 1` close them. Each window keeps its own point and scroll and has its own mode line, dimmed when inactive.
- Clicking selects a window, and the wheel scrolls the window under the mouse. Drag the borders between windows, or use `C-x +` to balance them and `C-x ^`, `C-x }` and `C-x {` to resize.
- Markdown preview: `C-c C-c p` shows the rendered document beside its source and updates as you type; links open in the browser. Press it again to close the preview.
- `M-x compile` runs a command, `cargo build` by default, in a `*compilation*` window that follows its output. `g` reruns it and `C-c C-k` stops it. In Rust buffers, `C-c C-c` then `C-k`, `C-b`, `C-t`, `C-r` or `C-l` runs cargo check, build, test, run or clippy.
- `M-g n` and `M-g p` visit each compiler error and warning in turn. In `*compilation*`, `RET` or a click visits the one at point and `n`/`p` show the next or previous without leaving.
- Lints: saving a Rust file runs `cargo check`, or clippy with `rust-lint = "clippy"`, and underlines errors in red and warnings in yellow. The message at point shows in the echo area and the mode line counts them. `C-c !` lists the project's problems in `*Diagnostics*`, and `M-g n`/`M-g p` step through whichever of the build or lint ran last.
- Modified Rust buffers are saved after a second and a half without input so they are linted while you work; `auto-save = false` turns this off.
- Line numbers beside buffers that visit files, with the current line's number brighter. `line-numbers = "relative"` counts from the current line and `"off"` hides them; `M-x display-line-numbers-mode` toggles them in one buffer, and `line-number-separator = true` draws a line beside them. Clicking a number selects its line, and dragging over numbers selects lines.
- Settings: `M-x customize` lists every setting with its value. `RET` or a click changes one at once, `C-x C-s` writes the changes to `config.toml` keeping its comments, and `g` reverts.

### Changed

- The project is now called oxmacs: the binary, window title, `*oxmacs*` splash, `about-oxmacs`, `kill-oxmacs`, `save-buffers-kill-oxmacs` and `oxmacs-doctor`. Settings now live in the `oxmacs` folder of the platform config directory.
- Animations are about a third quicker.
- `load-theme` and `toggle-theme` count as setting changes: `*Settings*` shows them unsaved and `C-x C-s` there keeps them.
- `*oxmacs-doctor*` lists lint support and clippy.

### Fixed

- Long mode lines are cut off instead of running into the clock.

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
