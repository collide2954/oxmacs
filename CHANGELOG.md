# Changelog

Notable changes to oxmacs, called mx before 0.12. The format follows [Keep a Changelog](https://keepachangelog.com/en/1.1.0/) and versions follow [Semantic Versioning](https://semver.org/).

## [Unreleased]

### Added

- `(ox synth)`, a new built-in module, makes sound from any Ox code. A sound is a graph built from functions: `synth/sine`, `synth/saw`, `synth/square`, `synth/triangle` and `synth/noise`, samples played at the note's pitch by `synth/sampler` or scattered into a cloud by `synth/grains`, shaped by `synth/envelope`, `synth/lowpass` and the other filters, `synth/vowel`, `synth/drive`, `synth/fold`, `synth/crush` and `synth/pan`, and passed through `synth/delay`, `synth/reverb`, `synth/chorus` and `synth/compress`, which can duck under another track, with `synth/control` for values that change while it plays and `synth/lfo` for slow movement. A song is tracks of patterns written bar by bar, such as `(synth/notes "a4 . c5 e5 | d5 _ _ c5")`, `synth/chords` and `synth/beats`, each track's voices mixed through its own effects, with a tempo and automations. Samples load from WAV, FLAC, MP3 and Ogg Vorbis files or are rendered from any sound or song, and are sliced, reversed, resampled, faded, normalized, joined, layered and run through effects; `synth/save` writes WAV files. It is built on the new `ox-synth` crate. In oxmacs, `synth/play` plays a sound or sample, `synth/loop` loops a song and swaps in a new one at the next bar while it plays, `synth/set` and `synth/automate` move controls, and `synth/position`, `synth/controls`, `synth/scope` and `synth/level` tell what is playing.
- `bellow` 0.1.0, opt-in: make music in Ox. `M-x bellow-new` writes a small song to start from and `M-x bellow-play` plays the song in the current file and shows `*bellow*` beside it. Evaluating a changed song with `C-x C-e` or `C-c C-b` swaps it in at the next bar while the music plays. The workbench shows the bar and tempo, the sound's wave and level, and a slider for every `(synth/control ...)` the sounds use, marked when an automation moves it; sliders, `synth/set` and `synth/automate` move the same controls. `M-x bellow-stop` stops it.
- `(ui/slider value low high)` draws a slider in plugin views that calls `:on-change` with the new value while it is dragged, with an optional `:step` and `:width`.
- `pdf` 0.1.0, opt-in: PDF files open as pages drawn by PDFium, which `M-x list-tools` installs (or the `library` setting names). Pages scroll past one after another, or one at a time with `c` (`continuous`), fit the window's width with `W` or whole pages with `H`, grow and shrink with `+` and `-`, and show at actual size with `=` (`zoom`). `n` and `p` turn pages, `SPC`, `DEL`, `C-n`, `C-p` and the mouse wheel scroll, `M-<` and `M->` go to the first and last page, `M-g g` to a page by number and `o` to a heading of the outline. `s` searches the whole document, marking every match and saying how many there are, and `C-s` and `C-r` step through them. Dragging over text selects it for `M-w`, links go to their page or open web addresses outside, `m` draws pages in the theme's colors (`theme-colors`), the mode line shows the page and scale, and a document that changes on disk is read again in place.
- `(ox pdf)` reads PDFs with PDFium: `pdf/library` finds the library and offers it in `M-x list-tools`, `pdf/open` reads a document's pages, title, author and outline, and `pdf/search` finds where text is on the pages. `(ui/pdf path)` draws the pages with a zoom, one page or all, the theme's colors, colored marks, moves to a page or by lines and screens, and tells Ox the page shown, the text dragged over and the links clicked.
- `open-view` takes `:watch`, a folder whose changes on disk reach `on-disk-change` while the view is open.
- `epub` 0.1.0, opt-in: EPUB books open as books, set in pages of real type in an installed serif (`typeface`, also sans or monospace), two pages side by side when the window is wide enough (`spread`), with the book's title and the chapter's at the top and page numbers at the bottom. `SPC`, `DEL`, the arrows and the mouse wheel turn pages, `]` and `[` move between chapters, `o` jumps from the table of contents, `s` searches the whole book and lists every match while `C-s` and `C-r` step through them, `+` and `-` change the size, and links inside the book work. Dragging over text selects it: `h` highlights it, `N` writes a note on it, `M-w` copies it and `H` lists the book's highlights and notes. Each book remembers its page, `M-x epub-recent` reopens one read lately, and `M-x epub-library` shows a folder of books as covers with how far you have read.
- `rodeo` 0.1.0, opt-in: 2D games written in Ox play in a window beside their code and change while they run. `M-x rodeo-new` writes a small platformer to start from and `M-x rodeo-play` plays the current file: it runs in the user module, `start` is called once, `tick` 60 times a second and `draw` every frame, so redefining one with `C-x C-e` or saving the file changes the running game while `defonce` keeps its state. The game is drawn at its own size, 320 by 180 unless it calls `rodeo/screen`, scaled by whole numbers with black bars. While its window is selected every key goes to the game except sequences starting with `C-x`. An error pauses the game with its message until you fix it and resume; `M-x rodeo-pause`, `rodeo-step`, `rodeo-slow`, `rodeo-restart` and `rodeo-stop` control it. Games draw with `(import (ox rodeo))`: `rodeo/sprite` with rotation, scale, flips, tint, flash and outline, `rodeo/rect`, `rodeo/circle`, `rodeo/line`, `rodeo/text` and `rodeo/fade`; read keys and named actions with `rodeo/held?`, `rodeo/pressed?` and `rodeo/released?`; and use the built-in `(ox geometry)`, `(ox ease)`, `(ox random)`, `(ox camera)`, `(ox collide)` and `(ox tiled)` for points and rects, tweens, seeded random numbers, cameras, collision and Tiled maps. `play-game`, `pause-game`, `resume-game`, `step-game`, `restart-game`, `stop-game` and `game-speed` control a game from any Ox code.
- New built-in modules, usable in any Ox code: `(ox geometry)` for plain `[x y]` points and `[x y w h]` rectangles (adding, scaling, lengths, angles, turning, edges, centers, overlaps, unions and clamping), `(ox ease)` for easing curves from linear to bounce, `(ox camera)` for cameras as plain maps that follow, clamp, zoom and shake, `(ox collide)` for overlaps, boxes that slide along walls and tiles, and spatial grids, and `(ox tiled)` to load Tiled maps as plain data. `(ox random)` gives random numbers from the system or, with `:from`, from a seeded generator kept in an atom so replays and rewinds repeat them: `random/float`, `random/range`, `random/int`, `random/chance?`, `random/pick`, `random/shuffle` and `random/noise`.
- `rodeo/black-hole` draws a black hole ray traced in Rust: every ray follows its exact path past a hole that does not spin, so the sky and an accretion disk bend around the shadow and photon ring, the far side of the disk shows over the top, the side coming toward you is brighter and bluer, and light from deep in the well is redshifted. Ox sets the camera's tilt, turn, distance and field of view, the disk, a star that collapses into the hole, and a procedural sky of stars, a Milky Way band and nebulae, and can turn lensing and each frequency shift on or off. Rows are traced in parallel and the sky is kept between frames while the camera holds still, so a turning disk plays live.
- The `ox-rodeo` crate holds the native core of the game engine: draw lists, textures, grid and Aseprite atlases, a fixed-timestep clock and particles. Game sound lives with the game natives in oxmacs, so the crate links into the `ox` tools without ALSA.

- `(ui/pages (ui/document blocks))` sets text in pages like a book: paragraphs, headings, quotes, list items, preformatted lines, images and rules made of runs with italics, bold, code and links, in a serif, sans or monospace typeface, broken between whole lines with headings kept with what follows, one page or two side by side, with a running header and page numbers, colored marks, text selected by dragging, clickable links and page turns from the mouse wheel. It tells Ox where each page starts, so a plugin can turn pages and remember its place.
- `rodeo/polygon` draws a filled or outlined shape through a vector of points.
- `(ox lexer)` highlights code a line at a time from a token regex, blocks that run across lines and word lists: `lexer/spec` makes a lexer once and `lexer/lex` gives a line's spans and the state the next line starts from. The bundled Python, Go, C, C++, TypeScript, JavaScript and JSON modes use it.
- `(ox zip)` reads zip archives, such as EPUB books: `zip/entries` lists what is inside, and `zip/read` and `zip/read-base64` read an entry as text or bytes. `image/decode` makes an image from base64 bytes. Built on the zip crate.
- `xml/parse` reads XHTML with `:html true`: whitespace between elements stays and HTML entities such as `&nbsp;` and `&mdash;` are understood.
- Plugins open the files they claim with `:opens-files ["epub"]` in their `plugin.ox` and `(on-open-file "epub" f)`, instead of the file opening as text.
- `zen` 0.1.0, opt-in: `M-x zen` writes without distractions and comes back the same way. The text sits in one centered column (`width`, 80 characters), wrapped between words (`wrap-words`); the other windows, the mode lines, line numbers, tab bar and side window go away and return afterwards (`hide-chrome`); every paragraph but the one you are in fades (`dim`); the line you type on stays in the middle of the window (`typewriter`); and the frame fills the screen (`fullscreen`). Each part is on by default and has its own setting.
- Plugins change how the windows look with `set-presentation`, which centers a text column, hides the mode lines, line numbers, tab bar and side window, fades all but the current paragraph, keeps point's line in the middle and fills the screen, and wrap a buffer's long lines between words with `set-word-wrap`.
- `explorer` 0.1.0, on by default: a file tree in a side window, shown, moved to and hidden with `C-x C-d`. It needs no setup: it opens at the project of the current file (its git root, or the nearest folder holding a `.projectile`, `Cargo.toml`, `package.json`, `pyproject.toml`, `go.mod` or `plugin.ox`), opens the folders down to that file and marks it, and follows each file you switch to. Typing filters the tree to the files whose names match (`DEL` takes a letter back, `C-g` clears). Git changes color the names and mark them M, A, U, D or !, with a dot on the folders holding them, and error and warning counts from language servers and linters show beside files and add up on folders. `RET` or a click opens a file in the other windows or a folder in place, `SPC` shows a file without leaving the tree, `C-c C-n` and `C-c C-d` make a file or a folder, `C-c C-r` or `<f2>` renames or moves, `C-c C-c` copies, `C-c C-k` or `<delete>` deletes to the trash, dragging a file onto a folder moves it there, `C-c C-h` shows hidden and ignored files, and dragging the divider beside the tree resizes it. `C-x o` and `C-x 1` leave it alone, and `C-x 0` in it closes it.
- Plugins can show a buffer in a side window along the left or right of the frame with `show-side-window`, hear clicks, drags from line to line and typing in their buffers with `open-buffer`'s `:on-click`, `:on-drop` and `:on-type`, and read error and warning counts per file with `(diagnostic-counts)` and `on-diagnostics-change`.
- `tabs` 0.1.0, opt-in: workspace tabs across the top of the frame, each keeping its own windows with their buffers, points and scrolling. `C-x t 2` opens a tab, `C-x t 0` closes it and `C-x t u` brings it back, `C-x t o` or `C-TAB` and `C-x t O` or `C-S-TAB` go to the next and previous tab, `C-x t RET` goes to one by name, `C-x t r` names one, `C-x t m` moves it right, `C-x t n` duplicates it, and `C-x t f` and `C-x t b` visit a file or a buffer in a new tab. A tab is called after the buffer it shows until it is named. The bar appears with the second tab (`show: :always` keeps it up), takes the theme's colors with the current tab opening into the windows below, and its tabs are clicked to switch, closed with their × or a middle click, added with + and dragged to reorder.
- Plugins keep window layouts with `(store-windows key)`, bring them back with `(recall-windows key)`, drop them with `(forget-windows key)`, hear the windows change with `(on-window-change key f)`, and show tabs across the top of the frame with `set-tab-bar`.
- The modes 0.14.0 removed come back as one opt-in plugin per language, each with highlighting, comments, indentation and its language server: `python` (basedpyright, pyright or pylsp), `go` (gopls), `c` and `cpp` (clangd), `typescript` with TSX and `javascript` (typescript-language-server) and `json`. A file whose language plugin is off opens as plain text, and the echo area says once a session which plugin to turn on. `.md` files open in Markdown mode again: the `markdown` plugin, now on by default, holds the mode, marksman and the preview.
- `M-x r-markdown-preview` shows an R Markdown or Quarto document rendered beside it with the `markdown` plugin's preview, which the `r` plugin now requires.
- `define-language-server` takes `:programs`, servers to try in turn, such as basedpyright, pyright and pylsp, and `:runs true` when `M-x list-tools` can install a server by running its `:install` command.
- `.editorconfig` files choose tabs or spaces (`indent_style`), the indent size and the tab width for the files they cover, ahead of the `indent-tabs` and `tab-width` settings, and a mode can default to tabs with `defmode`'s `:indent {tabs: true}`.
- The `indent-tabs` setting indents with tabs, then spaces for what is left, instead of spaces only.
- `defmode` takes `:indent {width: 4 after: #"[:{]\s*$" dedent: #"^\s*}"}`: `RET` and `TAB` indent one width past a line that opens a block and back on one that closes it. In modes without it, `RET` and `TAB` keep the indentation of the line above instead of starting at the margin and inserting a tab.
- A `defmode` highlighter that takes a third argument gets the state it returned for the line before, by returning `{spans: [...] state: x}`, so highlighting can carry strings, comments and blocks across lines; after an edit, only the lines from the first changed one are highlighted again.
- `C-c C-t` (`M-x ox-test`) in Ox buffers runs the buffer's tests, unsaved edits included, as `ox test` would, and lists how each went in `*ox tests*` beside it.
- `oxmacs --help` and `oxmacs --version` print to the terminal they ran from, Windows included, and `oxmacs -q` or `--no-init` starts without running `custom.ox` and `init.ox`. Other options are refused instead of opening a buffer named after them, and `--` still makes what follows file names.
- Plugins move the buffers visiting a file or folder they renamed with `(move-visited-files from to)`.
- Plugins list places with `show-places`, the same list *xref* and *Diagnostics* use: maps of `:file`, `:line` and `:column` with an optional `:label`, `:text` or in-memory `:source`, grouped by file or kept in order, visited by `RET` or a click or handed to `:on-visit`, and refreshed by `g` through `:on-refresh`.
- Plugins decorate text with `annotate`: shaded in a face, underlined as an error or warning, or with a note after the line, kept until the next edit, until `clear-annotations` or fading over a time, and moving with the text around them.
- `on-buffer-close` calls a handler with each buffer's handle after it closes, plugin buffers too, so a plugin can let go of what it kept for it.
- The downloads and the Fedora package include `THIRD-PARTY-LICENSES.html`, the licenses of the crates oxmacs and ox are built from, beside the JetBrains Mono font's license and the highlight queries' notices.
- `run-job` runs a program in its own buffer, the kind `M-x compile` uses, with `:cwd`, `:env`, a `:title`, `:input` for its standard input, and `:on-line` and `:on-exit` handlers.
- `console-start` takes `:prompter`, a command sent at the start and after each input to a program such as a shell without a terminal, which prints no prompt of its own.
- `(plugin-folder 'name)` gives a plugin its own folder for files, and with `:cache true` one in the system's cache; `path/safe-name` turns any text into a file name that works on every system.
- `path/normalize`, `path/relative` and `path/inside?` work out `.` and `..`, write a path from inside a folder, and tell whether a path lies in a folder, from the text alone and ignoring letter case on Windows.
- `file/list` takes `:sort` (`:name`, `:newest` or `:largest`), `file/sorted` puts other lists of files in the same order, and `file/size-text` writes a size the way `ls -h` does. `delete-path` and `move-path` delete and move files the way the editor does, following `delete-to-trash` and asking before replacing a file.
- `process/stop` asks a process started with `process/start` to end, and a few seconds later stops it and every program it started if it hasn't.
- `M-x list-processes` lists language servers beside builds, lints and consoles, and a process you stop shows as stopping until it ends; `(processes)` gives its `:status` as `:stopping` then.
- Programs started with `process/start` show in `M-x list-processes` as Ox processes, and end when oxmacs quits. The map `process/start` gives carries their `:id` there, for `stop-process` and `kill-process`.
- `open-view` and `open-buffer` take `:file`, the file the buffer shows: it is named after the file as file buffers are, opening the file again comes back to it instead of asking the plugin, and its changes on disk reach `on-disk-change`. Both return the buffer.
- Plugins keep state with each of their buffers with `(set-buffer-value! key value)` and read it with `(buffer-value key)`; it goes away when the buffer closes, and views are built again when it changes.
- Lines in plugin buffers can hold links: a piece `[text face :link target]` is followed by a click or by the new `follow-link` command, which calls the buffer's `:on-link` with target, and `next-link` and `previous-link` move between links, wrapping around.
- `open-buffer` and `open-view` take `:label`, the kind of buffer, such as `"remote folders"`; buffers of one kind share their keys, and `C-h f` names the kind where a command is bound.
- A line given to `open-buffer`, `set-lines` or `append-lines` as a map with fields of the plugin's own is an item: `line-item` gives the map of the line at point or of any line, `goto-item` moves to the first line whose item has the fields it is given, and `next-item` steps between items.
- `ox migrate PATH…` rewrites Ox files written for an older Ox to match it, changing only names a file doesn't define itself, and `ox migrate --check` lists the files it would change. Run it over `init.ox`, `custom.ox` and your plugins after upgrading.

### Changed

- Rust, TOML and Ox are plugins like every other language. `rust`, opt-in, holds Rust's mode, rust-analyzer, `M-x rust-check`, `rust-build`, `rust-test`, `rust-run` and `rust-clippy` on `C-c C-c C-k`, `C-b`, `C-t`, `C-r` and `C-l`, the lint run on save, chosen with its `lint` setting (`:check`, `:clippy` or `:off`) in place of the editor's `rust-lint`, and the patterns that make cargo's errors and panics clickable. `toml`, opt-in, holds TOML's mode and taplo, and `ox-lang`, on by default, holds Ox's mode; a plugin can no longer be named `ox`, the name of Ox's built-in modules. The `r` plugin declares R's mode itself, with `C-c C-f` formatting.
- `M-g i` (`imenu`) lists Markdown headings, R Markdown headings and chunks, a PDF's outline and an EPUB book's table of contents, through `:items` on `defmode` and `open-view`: a function giving maps of a `:label`, an optional `:note` and the `:line` to jump to or a `:go` function. `o` in PDFs and books opens the same list, and `pdf-outline`, `epub-contents` and `r-markdown-outline` are gone.
- Markdown is read one way everywhere: one line grammar in the `markdown` plugin, `(markdown grammar)`, finds fences, headings of levels 1 to 6 with their titles, quotes, bullets and inline code, strong text and links, and the highlighting, the preview and R Markdown's outline all use it. The preview now shows headings of every level, quotes, `*`, `+` and numbered bullets and `~~~` fences, which it used to leave as plain text.
- `M-x list-languages` works out each column from what the editor has for the language, such as its grammar, highlighter, indenter, formatter, linter and language server, instead of flags typed by hand. The Fold column is gone, as oxmacs has no folding, and Preview shows only when a language declares one.
- Plugins list the tools `M-x list-tools` installs with `:tools` in their manifest, each with its GitHub repository, archives per system, checksum and the setting that points at your own copy, and they show only while their plugin is on. air comes from the `r` plugin, whose `air-program` setting replaces the editor's, and PDFium from the `pdf` plugin, whose `library` setting now also shows in `M-x list-tools`.
- Plugins and init.ox name faces one way everywhere, the same names tree-sitter queries use, and can now also use `:lifetime`, `:search-match`, `:search-current` and `:paren-match`.
- `open-buffer`, `set-lines`, `append-lines`, `set-header` and `set-overlay` read lines and pieces the same way, and `set-lines` and `append-lines` take a buffer handle as well as a name. Overlay rows are written like other lines, `{line: pieces opacity: 0.5}` instead of `{pieces: ...}`, and a key is drawn with `[text face :keycap]` instead of the face `:keycap`. A `nil` piece is left out instead of showing as `nil`.
- Popups, transient menus, the headers kept above plugin buffers and the key overlay share one raised background, and dimmed text in the mode line, tab bar, completions and menus is one shade, a little easier to read than before.
- The headings of `M-x remote`, `list-processes`, `customize`, `vault`, `calendar`, the Ox profiler, R's package and environment lists, IRC channel lists and dired stay above their lines while they scroll.
- `RET`, `TAB` and `S-TAB` in the Ox manual, Markdown previews and R help are the shared `follow-link`, `next-link` and `previous-link`, and clicking a link follows it; `ox-manual-follow`, `ox-manual-next-link`, `ox-manual-previous-link`, `markdown-follow-link`, `r-help-follow`, `r-help-next-link` and `r-help-previous-link` are gone.
- Tools installed from `M-x list-tools` are unpacked inside oxmacs instead of by the system's `tar`, taking out only the tool's own file.
- Plugins keep their own files in `~/.oxmacs/data`, one folder each: the vault, calendars, EPUB places, IRC logs, R history and your copy of the tutorial move there the first time each is used, and nothing is deleted. `remote`'s copies of remote files move to the system's cache. The manual's start page lists where oxmacs keeps everything.
- With `OXMACS_HOME` set, installed tools, recovery copies and crash logs move under that folder too, in `tools` and `cache`, so two homes no longer share them; without it they stay in the system's data and cache folders.
- One `delete-to-trash` editor setting, on by default, decides whether deleting moves files to the system's trash, for `dired` 0.3.0, `explorer` 0.2.0 and discarding in `git` 0.4.0; their own `delete-to-trash` and `discard-to-trash` settings are gone. Moving a file onto one that exists asks the same question everywhere and moves the buffers visiting it along.
- Folders list one way in `dired`, the explorer and `remote`: folders first, then by name ignoring case with hidden files last, or newest or largest first in `dired`. Sizes read one way too, like `ls -h` (512B, 1.5K, 12M), in those and in `M-x list-processes`.
- `remote` 0.3.0: `!` runs a command in a shell of its own and shows it in a run buffer, where `g` runs it again and `C-c C-k` stops it, and `/` searches in a shell of its own too, so neither can disturb the connection kept open for browsing and saving. `s` opens the shell as a console, with `M-p` and `M-n` history, and `M-x remote-disconnect` closes the kept connection to the host here.
- R jobs, Shiny apps and R Markdown rendering (`r` 0.20.0) run in the same kind of buffer as `M-x compile`: `g` runs them again and `C-c C-k` stops them, in place of the `r` plugin's own `k`, `r-job-stop` and `r-job-again`. They show in `M-x list-processes` linked to their buffer, and they end with Finished, Exited abnormally with code N, or Killed.
- `flash` is now `annotate` with `:lasts`, the milliseconds it takes to fade, in place of `:duration`; `zap` 0.2.1 uses it.
- Every special buffer works one way: double-clicking a line in a list does what RET does there, as in plugin buffers, and `g` (or `M-x revert-buffer`) runs each buffer's own refresh, such as checking again in *Diagnostics*.
- `irc` 0.10.0, `remote` 0.2.0 and `r` 0.19.0 show IRC log searches, remote searches and R test results as lists of places: `RET` or a click visits, `SPC` shows, and `g` runs the R tests again. `irc-search-visit`, `remote-search-visit` and `r-test-visit` are gone; `RET` does what they did.
- *ox usages* and *ox stacktrace* are lists of places too, showing each place's line, and Ox docs (`C-c C-d`) open in *doc*.
- *xref* and *Diagnostics* are one kind of list of places: a click or `RET` visits a place, `SPC` shows it while staying in the list, `n` and `p` step and `g` refreshes. `location-visit`, `location-next` and `location-previous` are now `place-visit`, `place-next` and `place-previous`, and `goto-diagnostic`, `show-diagnostic` and `recheck-diagnostics` are `place-visit`, `place-show` and `places-refresh`.
- `math/random`, `math/random-int` and `math/random-pick` are now `random/float`, `random/int` and `random/pick` in `(ox random)`, and `(ox math)` gains `math/degrees` and `math/radians`.
- `ox profile` and the `profiler` plugin show the same rows: calls above 0.5% of the busy time, the 40 functions with the most self time and the 10 slowest ticks. The map `profile` and `profiler-stop` return holds those rows, each with its `:total-share` and `:self-share` of the busy time.
- `ox` reads every subcommand's arguments the same way: an option it doesn't know is refused with the subcommand's usage, `--` makes what follows paths, and a path that doesn't exist is an error instead of passing. `ox test` and `C-c C-t` share one runner and report: a file that needs the editor counts as skipped, the summary says how many were skipped, and failures name their file, line and column.
- `vault-read-kdbx` is now `keepass/read` in `(ox keepass)`, and like every native that reads files it doesn't run while code is only being checked.
- `ox check`, `ox test`, `ox lsp`, `ox perf` and `C-c C-t` know rodeo's game modules and the editor's own modules, `(ox ui)`, `(ox pdf)` and `(ox rodeo)`, in every file, so a game checks clean outside oxmacs instead of failing with `No module named ox/rodeo`. The rest of the editor API is known in editor code: `init.ox`, `custom.ox`, plugin folders and files in the oxmacs home.
- Ox files open in oxmacs are checked the way `ox check` checks them, so a script outside the oxmacs home that calls an editor function such as `message` is flagged instead of passing in the editor and failing under `ox`.
- Natives take keyword arguments the way Ox functions do: after the positional ones, with a keyword they don't know refused as `x has no keyword parameter :y`, which `ox check` and the editor's checking now catch before the code runs, and `nil` meaning the default. `ui/column` and `ui/row` take `:spacing` after their children.
- R Markdown prose is styled as Markdown's is, with quotes, list markers, `code`, **strong** text and links as well as headings.
- Markdown previews leave out the YAML front matter between `---` lines at the top of a document.
- oxmacs no longer builds image codecs it never uses, such as the AVIF encoder and OpenEXR, which iced's image support pulled in; it builds faster and its executables are smaller.
- `(settings 'weather {units: :imperial})` changes a plugin's settings, as `(settings {...})` changes the editor's, and `M-x customize` writes `custom.ox` that way; `configure-plugin` is gone. `settings-schema` lists the plugins' settings after the editor's, each with its `:owner`, `:kind`, `:doc`, `:value`, `:saved` and `:custom`, and `plugin-settings`, `saved-settings` and `custom-settings` are gone.
- A plugin's settings take the kinds the editor's settings use: `:text`, `:integer`, `:number`, `:toggle`, `:list`, `:data` for any value, or a vector of choices. `:string`, `:int` and `:bool` are refused with the name to use instead.
- `(setting :key)` reads a plugin's own setting in any of its files, not only its entry file, and `:startup-when true` starts a plugin with oxmacs whenever it is on, as `tabs` and `keycast` now do.
- Events have one set of names: `set-tab-bar` calls `:on-click` with the tab clicked and `:on-drop` with the tab dragged and where it landed, as `open-buffer` does for lines, and `ui/button` and `ui/image` call `:on-click` instead of `:on-press`. `:on-move` is only for point moving and `:on-select` only for selected text.
- A plugin's `:modes` entries name their mode with `title:`, as `defmode` does, instead of `name:`. The manifest is the one place for the extensions, file names, comment and keys of a mode it declares: `defmode` in the plugin gives only its highlighting and indentation and refuses the rest, and `git` keeps its commit keys in `:mode-keys`.
- A plugin's `:commands` names its commands, as `[hello]`, and each summary comes from the command's `defcommand`, read at startup without running the plugin; a listed command no `defcommand` defines is reported in `*ox log*`.
- Projects are found one way. `(project-root)` is the nearest folder with a `.git`, `.hg`, `.svn` or `.projectile` below your home folder, and the explorer opens there. `(language-root)` is the nearest folder a language marks as a project, falling back to the project root: the cargo workspace for Rust, the folder with a `DESCRIPTION`, `renv.lock` or `.Rproj` for R, and the files a mode lists in its `roots`. Language servers, `M-x compile`, `cargo check` and R start there, so a Rust workspace runs one rust-analyzer instead of one per crate. `define-language-server` no longer takes `:roots`; `defmode` and a plugin's `:modes` take `roots` instead.
- A plugin's `:modes` entry can borrow a built-in grammar with `grammar: "rust"`, `"toml"`, `"r"` or `"ox"`, getting its highlighting, indentation, outline and brackets without a `defmode`, and can say how to find its project with `locate`, a command that prints a file in it.
- Plugins declare build commands, a linter and error patterns in their manifest: `:builds` makes commands such as `rust-check`, `:lint` runs a program on save with arguments that can come from the plugin's settings and reads its output by a named format, and `:error-patterns` are regexes with `file`, `line` and `column` groups that make build output clickable. While a linter runs, the mode line names its command, such as `cargo check`.
- `vault` 0.3.0: other plugins log in with `vault/login`, which gives an entry's `:user` and `:password` and is an error when there is no such entry, as `calendar`, `remote` and `irc` now do; `vault/secret` is gone.
- Completion, call signatures, `M-.`, `M-?` and `C-c C-d` work one way in every language: the buffer's mode answers when it can, and its language server otherwise. Completion pops up on its own after three characters of a name, counting the characters a language allows in names, such as `-` and `/` in Ox and `.` in R, and never inside strings or comments. `lsp-find-definition`, `lsp-find-references` and `lsp-describe` are now `find-definition`, `find-references` and `describe-at-point`, and Ox's own `ox-find-definition`, `ox-usages`, `ox-doc` and `ox-complete` are gone in their favor.
- `define-mode` and `console-start` take `:complete`, `:signature`, `:definition`, `:references` and `:docs`, which answer `C-M-i` and completion while typing, the call signature, `M-.`, `M-?` and `C-c C-d` in the mode's buffers or the console before any language server does. Each, like `:items`, runs as a task of its own and may await, and gives its answer, `nil` to leave it to the language server, or `true` when it answers itself. The r plugin answers this way in R buffers and consoles, so `r-complete` and `r-find-definition` are gone in favor of `C-M-i` and `M-.`.
- The editor settings `completion-delay` and `signature-delay` set how long typing rests before completion pops up on its own and before a call's arguments show, 0.25 and 0.4 seconds by default, for every language. The r plugin's settings of those names are gone.

### Removed

- `with`, `defrule` and `case`, which each repeated another form: `assoc` changes a record's fields, `defrules` takes a single rule, and `match` compares with numbers, strings, keywords and quoted symbols as `case` did. `ox migrate` rewrites them.

### Fixed

- An edit in a buffer whose mode highlights with an Ox `:highlight` function highlights again only the lines whose state it changes, instead of every line to the end of the buffer. Ox buffers are highlighted the same way, from the start of the file, so a string or block comment opened more than 300 lines above still colors the lines below it.
- The Rust edition used for formatting is read from `Cargo.toml` as TOML, so a commented-out `edition` line or a comment after it no longer confuses it.
- Changing zap's `flash`, screensaver's `idle`, calendar's `reminders` or weather's `mode-line` takes effect at once instead of at the next start, and a plugin setting changed in `M-x customize` reaches `tabs` and `zen` at once.
- Opening plugin buffers no longer adds a set of keys for each one that lasts until oxmacs quits, so `C-h f remote-visit` no longer lists every remote folder ever browsed, and reopening a buffer reuses its keys.
- On Windows, language servers installed with npm start: oxmacs runs their `.cmd` script directly and never picks npm's extensionless shell script, and every program lookup tries the extensions in `PATHEXT`.
- Sending a lot of text to a busy process, such as an R session or a language server, no longer freezes the editor: input is written in the background.
- Stopping or killing a build, a console or a language server stops the programs it started too, and on Windows they all end when oxmacs quits. Stopping waits a few seconds after asking, as before, and on Windows it now ends the whole tree instead of only the first program.
- A language server's log keeps going past output that isn't valid UTF-8.
- Saving a file, `file/write`, `ox fmt` and `image/save` all write one way: a file that exists keeps its permissions, owner and links, and now also its access list, creation time and other streams on Windows, such as the mark that it came from the internet, and its extended attributes on Unix, such as ACLs, the SELinux label and macOS quarantine. A new file gets the usual permissions instead of being readable only by you.
- Recovery copies of unsaved buffers are readable only by you.
- `path/expand` leaves `~bob/notes` as it is instead of turning it into a bob folder in your own home, and `~` expands to the home folder without a trailing separator.
- The explorer works when it is rooted at a drive, such as `C:\`, and compares folders the same way everywhere (`explorer` 0.1.4).
- `file/read` leaves out a leading byte order mark.
- `file/list` reads each entry's details once instead of twice, which is much faster on Windows in large folders.
- `image/save` that can't encode an image, such as an ICO over 256 pixels, leaves the file already there as it was instead of emptying it, and `ox fmt` replaces each file whole.
- A remote command with an unclosed quote, such as `echo 'oops`, no longer hangs the `remote` plugin's connection, and with it every later save of a remote file.
- `print` and `println` show in *ox repl* when it is open and in *ox log* otherwise. They wrote to a standard output the Windows release doesn't have, so their text was lost.
- Ox code waiting on a file, a program or the network goes on as soon as the work is done, instead of at the next poll up to a tenth of a second later, so chains of awaits, such as the explorer opening a project, no longer pay that delay at every step. Problems the background Ox checker finds also show as soon as it finishes.
- `process/start` writes its `:input` and then closes the program's input, as documented; it used to ignore it. Its `:lines` and `:errors` keep streaming past output that isn't valid UTF-8, so a remote shell or R output no longer stops there.
- `process/which` and `process/run` find `.cmd` programs on Windows, trying the extensions in `PATHEXT`. `process/kill` stops the programs a process started too, and on Windows processes started from Ox end when oxmacs quits.
- In Ox buffers, completion, `C-c C-d` and the signature hint know definitions typed in the buffer before it's evaluated, as `ox lsp` does; both use one shared way to describe and complete names.
- `M-.` and `M-?` in Ox buffers follow what a name refers to, as `ox lsp` does: usages of a parameter stay inside its function, a global's usages leave out locals of the same name, and `M-.` on a local goes to where it's bound, without evaluating the buffer first.
- `ox lsp` renames and finds references by what a name refers to: renaming a global no longer touches a parameter or a local with the same name, or another module's global, and `g/area` follows `area` into the module `g` names. Going to a definition lands on the defined name.
- A new file in folders that don't exist yet belongs to its project as soon as it's opened, so its project's checks and language server count it even where the temporary or home folder is reached through a link or a short name.
- `C-M-f`, `C-M-b`, `C-M-u`, `C-M-d` and bracket matching skip brackets inside strings, comments and character literals in languages without a tree-sitter grammar, such as Ox and the modes plugins define, using the language's own highlighting.
- Ox highlighting, the signature hint, `C-x C-e`, completion, names at point and `ox fmt` read Ox with the reader's own tokens, so raw strings with quotes inside, regexes with braces, block comments, commas and strings inside `{…}` holes no longer confuse them, and code inside a hole is colored as code.
- A prompt opened by a click or by Ox, such as clicking describe-command on the start screen, shows its completions as the same command's keys do, instead of only counting them.
- After C-x C-w, or renaming or moving a file from the explorer or dired, its language server is told the file closed at its old place and opened at the new one, so edits, diagnostics and M-. follow it; the buffer also takes the language and editorconfig style of its new name.
- Language server edits that insert several pieces of text at the same place keep the server's order, instead of coming out reversed.
- Language server edits, renames, references and diagnostics reach a file's buffer when the server spells its path differently, such as a lowercase drive letter on Windows, instead of being dropped or opening it again; recovered changes, saves after a prompt and renames from the explorer or dired match the same way.
- Opening a file from a filtered explorer tree clears the filter, so the next search starts afresh instead of adding to the old one.
- A new tab opened while the explorer or another side window is selected shows the main window's buffer, not the side window's.
- The epub library shows how far you have read each book even when its folder is written with other separators than the books' own paths, as `C:/Books` on Windows.
- A file that isn't a readable EPUB says why it couldn't be opened without an Ox error from the mode line.
- An R background job says it finished only after the last of its error output is shown, instead of sometimes printing that output after "Finished".
- Text no longer spills past the bottom of a window over its mode line and the echo area where oxmacs draws without a GPU, as under WSL: a window draws only the rows that fit in it instead of relying on the renderer to cut off the rest.
- `set-lines` with `:line` moves every window showing the buffer there, not only the selected one.
- Hooks such as `on-window-change` hear what an Ox command changed as soon as it finishes, instead of at the next key press.
- `(expect event.all-day)` checks the field, as `(expect (= event.all-day true))` does, instead of failing with `. is not defined`.
- `file/write` and `file/write-base64` write a new file that then takes the old one's place, so a crash or full disk no longer leaves `custom.ox`, calendar events or other files a plugin writes half written. Files with other hard links are still written in place, and symlinks keep pointing at their targets.
- Saving a file with hard links on Windows writes it in place, as on Linux and macOS, so its other names keep showing the same text.
- `M-x list-tools` lists only the language servers of languages oxmacs knows, instead of offering to install servers for languages it has no mode for, which could never start.
- Renaming or moving a file or folder with `R` in dired moves the buffers visiting it along, as dired did before it became a plugin, instead of leaving them on the old path, where saving brought the old file back.
- A command run with `!` in the remote file browser that reads its input, such as `cat`, gets none and finishes, instead of hanging the host's connection and swallowing the commands after it.
- Files read over sftp each pass through a file of their own, removed once read, so two reads at once no longer overwrite each other.
- The dnf repository on GitHub Pages is served: Pages was never turned on, so `oxmacs.repo` and `oxmacs-nightly.repo` were not found. Each publish now replaces the site with a single commit instead of keeping every old RPM in the repository's history.
- Releases carry the Fedora RPMs, as the README says; 0.14.0's were attached by hand.
- `M-x remote-add-host` saves the hosts in `custom.ox`, as `M-x irc-setup` saves networks, instead of adding another `use-plugin` form to the end of `init.ox` each time.
- The explorer reads git status without rewriting `.git/index` and without walking the folders git ignores, so it opens and refreshes several times faster in large repositories, and it runs no git at all outside a repository.
- Opening a remote file no longer writes its local copy over the file at the same path on this machine: copies live in the oxmacs home's `remote` folder, one folder per host, with the characters Windows refuses in names replaced.
- `file/copy` refuses to copy a file onto itself or onto another of its hard links, which emptied it, and a folder into itself, which never finished, and it copies links as links instead of following them. `file/remove` removes a link to a folder on Windows instead of failing, and leaves the folder it points to alone.
- Language server edits for text that changed after they were asked for, from formatting, renames, code actions or the server itself, are dropped with a message instead of landing in the wrong places. Formatting on save through a language server now formats first and writes once, and still saves when the server fails or stops.
- The `C-h t` tutorial teaches the cut and paste keys oxmacs has (`C-w`, `M-w`, `C-y` and `M-y`, with `C-k` deleting the rest of the line), and its configuration step defines the command it binds, so the `init.ox` it leaves no longer fails at every start.
- The explorer keeps up with every change on disk: changes that arrive while it reads the folders bring another read once they settle instead of being dropped, so a burst of new files all show, and changes made while it is hidden show when it comes back.
- A PDF or EPUB closed with `C-x k` opens again instead of switching to a buffer that no longer exists, and two documents with the same file name each get a buffer of their own, named after their folders as files are, such as `paper.pdf<drafts>`, instead of the second showing the first. Opening a document that is already open goes back to its buffer.
- `rest` on a vector and `first`, `second`, `last` and `empty?` no longer copy the whole collection, and `count` and `nth` walk lists without copying them, so a loop that takes a vector or list apart item by item runs in linear time instead of quadratic. Errors from `map`, `filter`, `reduce`, `take` and the other sequence functions name the function called instead of `vec`.

### Security

- `net/send` refuses a line holding a carriage return or line feed instead of sending it to the server as two lines, and IRC sends text with a lone carriage return as separate lines after asking, so pasted text can't slip a second command to the server.

## [0.14.0] - 2026-10-25

### Added

- Ox, the language oxmacs is configured and extended in, and its `ox` command, which runs `.ox` files, opens a REPL and shares oxmacs's version number. Ox has numbers that grow into bignums, strings with interpolation, raw strings (`#r"…"`), regex literals (`#"…"`) and nested `#| … |#` comments, `def` with docstrings, `fn` with optional, rest and keyword parameters, closures, tail calls, vectors, maps and sets that keep insertion order, records, atoms, hygienic macros (`defmacro`, `defrule`), `match` with guards, destructuring, errors with `try`, `catch`, `finally`, `deferror` and tracebacks, modules with `import`, `export` and live reloading, tasks that run a slice at a time and can be interrupted or cancelled, generators, promises with `await` and `spawn`, `(atomically …)`, worker isolates on other threads that talk over channels, Rust values that Ox can hold, `sleep` and `every`, and a garbage collector.
- Ox has loops: `(loop [i 0] … (again (inc i)))` goes round in constant space, `for` builds a vector and `each` runs code for every item, both with nested bindings, `:when` and `:let`, and `while` and `until` repeat on changing state. `case` picks a clause by constant: `(case key [:up 1] [(:left :right) 2] [else 3])`.
- Ox's core library is built into Ox in Rust: `map`, `filter`, `reduce`, `sort-by`, `group-by`, `frequencies`, `partition`, `zip`, `get-in`, `update-in`, `merge-with`, `memoize`, `juxt`, `swap!`, `pr-str` and the rest, `take`, `drop`, `take-while` and `drop-while`, which read generators lazily, and macros such as `when`, `cond`, `->`, `some->` and `as->`. Functions passed to them can still `await`, `sleep` and `yield`.
- Ox's library modules: `(ox string)`, `(ox regex)`, `(ox json)`, `(ox file)`, which reads and writes binary files as base64, `(ox path)`, `(ox http)`, whose `http/request` sends any method with headers, a body and a login, `(ox csv)`, `(ox process)`, which runs programs with `process/run`, or with `process/start` with their output and errors as channels and input you write to, and finds them with `process/which`, `(ox net)`, which connects over TCP or TLS and streams lines through a channel, and `(ox crypto)`, with Argon2id keys, XChaCha20-Poly1305 sealing, TOTP codes, random bytes, SHA-256 and base64.
- `(ox math)` has `sqrt`, `pow`, `exp`, `log`, `log10`, `sin`, `cos`, `tan`, `atan2`, `pi`, `e`, `clamp`, `lerp`, and random numbers with `random`, `random-int` and `random-pick`. `pow` stays exact for whole numbers, so `(math/pow 2 100)` is the whole number, and a square root or logarithm out of range raises an error instead of giving NaN.
- `(ox time)` works with time zones and the calendar: `time/instant` reads moments and civil times, `time/local` takes a zone and `time/zone` names the local one, `time/add` adds days or months across daylight saving changes, `time/format` writes moments with strftime patterns, and `time/days-in-month`, `time/day-number`, `time/date-of` and `time/weekday-number` do date arithmetic.
- `(ox xml)` reads XML into maps of `:tag`, `:attrs` and `:children` with `xml/parse`, finds elements along paths such as `"channel/item"` with `xml/find` and `xml/find-all`, reads `xml/text` and `xml/attr`, and writes the same maps back as XML with `xml/write`.
- `ox/image`, images in Ox: `image/open` reads PNG, JPEG, WebP, GIF, BMP, TIFF, ICO, QOI and SVG, turned upright by their EXIF orientation, `image/probe` reads a file's size and format without decoding it, `image/save` writes any of them but SVG with a JPEG `:quality`, and `image/new` makes a blank image. Geometry covers resizing with five filters, fitting, filling, thumbnails, crops, padding, canvases, rotation by any angle, flips, trimming and affine and perspective warps, and `image/pixel` reads one pixel. Color work follows CSS filters: brightness, contrast, gamma, exposure, hue, saturation, vibrance, grayscale, sepia, invert, tint, white balance, levels, curves, posterize, threshold with Otsu, color replacement, and quantizing to a palette or a median-cut count with dithering, spread over every core with rayon. Filters cover Gaussian, box and median blurs that keep transparent edges clean, sharpening and unsharp masks, Sobel and Canny edge maps, embossing, pixelating, repeatable noise, vignettes and custom convolution kernels. Drawing works on a private copy inside `image/edit`, or on any image as a new one: antialiased rectangles with rounded corners, ellipses, circles, polygons, lines and polylines with fills, strokes, dashes, opacity and blend modes, single pixels, and pasting images with all sixteen CSS blend modes, opacity, scale and masks; `image/mask` and `image/tile` round it out. Colors are hex strings, vectors, CSS names or theme faces. Slow work runs off the editor's thread and returns promises, and image functions wait for promises they are given, so `(-> (image/open path) image/width)` needs no `await`. Built on the image, imageproc, resvg and tiny-skia crates.
- Macros say how their calls look with metadata, `(defmacro ^{indent: 1 defines: 0} defwidget [name & body] …)`, which `ox fmt`, Ox highlighting and indentation in oxmacs, go-to-definition and `ox lsp` read from Ox's own macros, the editor's, the file's and those of the modules it imports.
- A module's own definitions, and names it imports with `:only`, take the place of built-in macros of the same name. `ox check` warns when a definition shadows a built-in, unless it is marked `(def ^:shadow (drop …) …)` or the module leaves the built-in out with `(import (ox :exclude [drop]))`; `ox/drop` still reaches it. `^:shadow` and `^{…}` are Ox's metadata, read before the name in `def`, `defonce` and `defmacro`.
- `ox check` finds mistakes in Ox code without running it: undefined names, suggesting a close one; calls with the wrong number of arguments or unknown keyword arguments; unused bindings and imports, unless named with a leading `_`; malformed forms; and pattern strings that a regex literal or raw string would write without escapes. It prints like rustc, or with `--format json` and `--format github` for CI, fails on errors, and on warnings too with `--strict`.
- `ox fmt` formats Ox code in the Clojure style, keeping your line breaks and comments, and `ox fmt --check` fails in CI when a file needs it. In oxmacs, `TAB` and `RET` indent Ox the same way, `C-c C-f` formats the buffer, and saving an `.ox` file formats it.
- `ox lsp`, a language server for Ox in other editors such as Helix and Neovim: checker findings as you type, completion, hover docs, signature help, go to definition, references, rename, formatting and document symbols. It uses the `lsp-server` crate.
- Ox tests: `(test "name" (expect …))` blocks sit next to the code they test, are skipped when a file or plugin loads, and run with `ox test PATH…`, plugin folders included, which prints each result and explains a failed `expect` with the value of each argument and where it failed.
- Profiling for Ox code: `M-x ox-profiler-start`, `M-x ox-profiler-stop` and `M-x ox-profiler-report` show where the time went, grouped by plugin, as a call tree or as functions by self time, with the slowest editor ticks; `TAB` folds, `f` switches tables and `RET` visits a function. `ox profile FILE` prints the same report, and `(profile f)` returns it as data.
- `ox perf bench` benchmarks Ox code with its time and allocations, and `ox perf sample`, on Windows, samples the VM's native stack to show where its time goes.
- `~/.oxmacs/init.ox` runs at startup, before the window opens (`OXMACS_HOME` moves the folder). Ox code there can define commands with `defcommand`, bind keys with `bind-key`, show messages and mode-line text, and turn on and configure plugins with `use-plugin`. Errors and their tracebacks go to `*ox log*`. Ox code that runs too long is stopped with an error in the log: views, highlighters and take-over frames after 250 ms, and init files, plugin loads and the REPL after 10 seconds.
- Plugins, written in Ox. Bundled plugins ship with oxmacs, and those in `~/.oxmacs/plugins` load on their first command or once `use-plugin` turns them on. A plugin's `plugin.ox` gives its `:version` as a semantic version and may require other plugins and oxmacs itself, as `:requires {markdown: "^0.2" ox: ">=0.14"}`: required plugins start first, one that is off lends its modules without starting, and a plugin imports another's modules only when it requires it. A plugin whose manifest or requirements are wrong stays off and says why in `*ox log*`.
- The README and the Ox manual list the bundled plugins, with their versions, whether they are on by default and what they do, and Ox's built-in modules. The calendar, git, irc, keycast, life, markdown, r, screensaver, vault, weather and zap plugins are off until `(use-plugin name)`.
- Working on Ox in oxmacs: `.ox` files open in Ox mode, with highlighting and `;` comments. `M-x ox-repl` (`C-c C-z` in Ox mode) opens a REPL with `M-p` and `M-n` history. Saving `init.ox` runs it again, and saving a file of one of your plugins reloads that module, keeping `defonce` state and redrawing its views; a plugin you are writing starts the first time you load one of its files with `C-c C-k`.
- Interactive Ox in the style of CIDER. `C-x C-e` evaluates the form before point and `C-M-x` the top-level form around it, showing `=> value` beside the form until you edit the buffer; `C-c C-b` evaluates the buffer and `C-c C-k` saves and loads the file. Evaluation runs between keystrokes, so `C-c C-c` stops a runaway loop. Errors show `!! message` beside the form, and errors from calls open `*ox stacktrace*`, where `RET` visits a frame's source and `q` closes it.
- Ox files are checked half a second after you stop typing, with the editor's API known, and `ox check`'s findings are underlined, echoed at point and counted in the mode line. Checks run beside the editor, never touch files, processes or the network, and stop after 2 seconds.
- In Ox buffers and the Ox REPL, the echo area shows the signature of the call at point with the first sentence of its doc once point rests, and `C-c C-d` opens `*ox doc*` for the name at point: what it is, where it is defined, its signature, its doc and the tests named after it as examples.
- Every function, macro and special form that Ox and the editor provide has docs.
- Completion of Ox names from the running image in Ox buffers and the REPL: `C-M-i`, `TAB` after a name, and a quiet popup once three characters of a name are typed.
- `M-.` goes to where an Ox name was defined, in your files, plugins or the prelude, and `M-,` comes back; `M-?` lists its usages across open Ox buffers, the current folder and the oxmacs home.
- `(open-view "*name*" view-fn :keys {…})` shows a buffer drawn with widgets from `(ox ui)` (`column`, `row`, `text` with faces, `button`, `space`, `scrollable`, `image`), redrawn when atoms change. Views hear their size in pixels through `:on-resize`, can open unseen with `:show false`, and take `:beside` and `:select` like `open-buffer`; `ui/image` shows an `ox/image` image and takes `:on-press` and `:selected`.
- Plugins can show text buffers with styled lines and their own keys (`open-buffer`, `set-lines`, `current-line`, `goto-line`), and ask with `(await (prompt "Rename to: " :initial name))` and `(await (yes-or-no "Delete?"))`, where `C-g` raises `Quit`. Ox commands and view buttons run as tasks, so waiting on a prompt or the network never holds up typing.
- Plugin manifests can bind global keys, add start-screen lines, open directories and name file modes with `:modes`, so opening such a file wakes the plugin; a `COMMIT_EDITMSG` opened from a terminal `git commit` gets the Git commit mode. Plugins can follow disk changes and saves with `on-saved`, open files and ask for a path with `prompt-path`, whose `:initial` can start with a file name.
- Plugin text buffers can show pills, watch another folder with `:watch`, keep their names, keep lines above their text while it scrolls with `set-header`, and have an input line, as in a chat: `open-buffer` takes `:input` and `:show`, and `append-lines`, `take-input`, `set-input`, `input-text`, `input-point`, `current-column` and `line-text` work with it. Plugin settings can be `:list`s, which `M-x customize` shows and leaves to `init.ox`.
- Plugins can define file modes with keys and highlighting (`defmode`) and transient menus (`transient`), and open buffers beside the current one that follow point and width.
- Plugins can draw rectangles, circles, lines and text in theme or hex colors, in a view with `canvas`, which takes `:relative true` to fill the width it gets, or over every window with `take-over`, and can run code after a stretch without input with `on-idle`. `prompt` takes `:choices` to complete from names with notes.
- Plugins can run consoles with `console-start`, which hears the program's marked output and prompts, and talk to them with `console-send` and `console-request`. They can read and move point, insert text, read ranges and the region, find the statement or function around a place, list and read buffers, show a buffer below or beside another with `display-buffer`, offer completions with `complete-at`, remember where a jump started for `M-,`, tell whether the last key typed text with `just-typed?` or a prompt is open with `prompting?`, show text in a buffer of a mode with `show-text`, and bind keys in the buffers of a mode with the manifest's `:mode-keys`.
- `console-start` takes `:errors`, patterns of lines that mean the code just sent failed: such a line reports that code to `on-evaluate` again as failed, so zap flashes R code that raised an error in the error color.
- Plugins get buffer handles: `(current-buffer)` returns one that stays with its buffer when the name changes and says `That buffer was killed` once it is gone, `(with-buffer b ...)` works in another buffer without touching the windows, each task with its own current buffer, and natives that take a buffer name take a handle too.
- Plugins edit buffers by character offset: `point`, `set-point`, `insert`, `delete`, `text-between`, `buffer-size` and `region`, `line-start`, `line-end` and `line-of` to work with lines, and markers that move with the text around them. Read-only buffers refuse edits.
- Plugins hear buffers: `on-buffer-open` for each file or other buffer people type in, `on-buffer-change` with the edits each command makes, `on-mode-enter` when a buffer enters a mode, and `on-before-save`, which saving waits for, so a handler can format a file before it is written; an error in one cancels the save.
- Plugins can list the processes oxmacs runs with `processes`, follow them with `on-processes-change`, end them with `stop-process` and `kill-process`, and sample their usage with `watch-process-usage`.
- Plugins float text over the windows with `set-overlay`: rows of text, theme faces and keycaps, each with its own opacity, at the bottom right or larger at the bottom center.
- Plugins light up text with `flash`, hear code run with `on-evaluate`, and report code they ran, or that failed, to zap with `zap`.
- Plugins hear the keys that run commands: `on-key-command` gets the keys pressed, the command's name and `:repeat`, leaving out typing, with a command run from `M-x` under the keys of `M-x`.
- Plugins can copy a secret without it entering the kill ring, show desktop notifications, ask whether the window has focus, ask for the reference of the Ox library and the editor API with `ox-reference`, highlight Ox code with `ox-highlight` and run code in the REPL with `run-in-repl`.
- Keyboard macros: `F3` or `C-x (` starts recording, `F4` or `C-x )` stops, and `F4` or `C-x e` plays the keys back, with `e` playing them again; `C-x C-k r` plays them on each line of the region and `F3` while recording inserts a counter. A replay waits for Ox commands to finish, stops at the first error and undoes as one step, and the mode line says when a macro is being recorded.
- Language servers. Opening a Rust or TOML file starts rust-analyzer or taplo when it is installed, in the project's root, with its state in the mode line, and plugins start servers for their languages with `define-language-server`. Diagnostics as you type, completion, the signature at point, `C-c C-d` docs, `M-.` definitions, `M-?` references in `*xref*`, `<f2>` rename, `C-c .` code actions, formatting, and inlay hints at the end of lines. `M-x list-tools` lists the servers and offers to install missing ones, asking first; `M-x lsp-restart` and `M-x lsp-log` help when one misbehaves; `language-servers` and `inlay-hints` turn them off.
- Unsaved files get recovery copies in the oxmacs cache folder, never beside the files themselves, and opening a file whose copy is newer offers to recover it; saving, reverting or killing the buffer removes the copy. A crash writes a log there too.
- In path prompts, typing a new root after a separator starts the path over, as in Emacs: `//` goes to the root, `~/` home and, on Windows, `D:` that drive.
- `C-a` on the input line of a REPL, shell or chat stops after the prompt.
- `M-x list-languages` shows each language with its file extensions and what works in it.
- A hands-on tutorial and the Ox manual, in the bundled `manual` plugin. `C-h t` opens your own copy of the tutorial where you left it: moving, editing, files, windows, search, a first Ox command and your configuration. `C-h i` opens the manual, an Info-style reader with a guide to the language, modules and libraries, extending oxmacs, buffers and views, writing plugins and the `ox` command, and a reference of every library module and the editor API built from their documentation. `RET` follows links, `l` and `r` go back and forward, `n` and `p` turn pages, `s` searches, `i` looks up a name and `x` runs an example in the REPL. Every example in the guide is tested.
- A remote file browser instead of TRAMP, the bundled `remote` plugin. `M-x remote` lists the hosts in `~/.ssh/config` and the ones you save, and opens a dired-like browser on each over ssh, sftp or ftp, or through any command that opens a shell, such as `docker exec` or `wsl`. Files open as local copies that go back when saved, `C` and `U` copy files either way, `!` runs a command in the folder shown, `/` searches the files below it and `s` opens a shell there. One connection stays open per host, and logging in is left to ssh and its keys.
- A password vault, the `vault` plugin. `M-x vault` lists your logins in `*Vault*`; the vault is one file, `~/.oxmacs/vault.json`, sealed with XChaCha20-Poly1305 under a key Argon2id derives from your master password, and it locks after 10 minutes unused or on `M-x vault-lock`. `c` copies a password, which skips the kill ring and leaves the clipboard after 30 seconds, `t` copies the current one-time code, `RET` shows an entry with its code counting down, and `M-x vault-generate` makes passwords, passphrases and PINs. `M-x vault-import` brings in KeePass databases, Bitwarden and 1Password CSV exports and the pass store, and other plugins ask it for their logins. New crates: argon2, chacha20poly1305 and keepass.
- A calendar, the `calendar` plugin. `M-x calendar` shows an agenda of the coming two weeks, `m` a month and `w` a week as a grid of hours. Events are `.ics` files, one per event, in folders under `~/.oxmacs/calendar`, with time zones, repeats (`RRULE` with exceptions), reminders in the echo area and on the desktop, invitations you can accept, decline or answer maybe with a reply file, and CalDAV sync whose password comes from the vault.
- A new IRCv3 client in the style of ERC, the `irc` plugin, in place of the built-in one. `M-x irc-setup` adds a network to `custom.ox`, keeping its SASL password in the vault, and `M-x irc` connects. Messages line up beside a nick column and wrap under themselves, with colored nicks, mIRC formatting, links, history from the server, replies, reactions, read markers, mention highlights and desktop notifications, a nick list on `C-c C-n`, `C-c C-SPC` for the next buffer with news, joins and parts only for people who spoke lately, multiline pastes after asking, and searchable logs. It has no bouncer of its own.
- IRC reconnects after a lost connection with backoff, after 1, 2, 5, 15 and 30 seconds and then every minute, keeps trying while the server can't be reached, says so in every buffer of that network and rejoins the channels you were in. `/reconnect` connects again at once and `/quit` stops trying.
- IRC shows server errors and IRCv3 `FAIL`, `WARN` and `NOTE` replies in the buffer you are in, or in the channel they are about, says when a nick you asked for is taken, and tells you about invitations, with `C-c C-j` offering the channel you were invited to.
- IRC networks take `:monitor`, a list of nicks to watch with IRCv3 `MONITOR`: the server buffer, and a private chat with them, say when they come online or go offline, with a desktop notification when they arrive while you are elsewhere.
- IRC `/whois` answers show in the buffer you asked from as plain sentences, such as who someone is, their channels, account, server, idle time and away message, and `/whois` alone in a private chat asks about the other person. Away replies to your messages show in that chat.
- An IRC channel list: `/list` or `M-x irc-list` shows a network's channels by size with their topics, `/` filters them by name or topic, `RET` joins the one on the line and `g` lists them again.
- `TAB` in IRC completes slash commands at the start of the input and the channels you are in, as well as nicks. IRC follows the capabilities a server adds or drops while you are connected, with IRCv3 `CAP NEW` and `CAP DEL`, and asks for `cap-notify` and `userhost-in-names`.
- Git, the `git` plugin, in the style of Magit. `C-x g` shows the branch and its upstream with ahead and behind counts, untracked, unstaged and staged files with status letters and +/− counts, stashes and recent commits. `n` and `p` move, `TAB` folds sections and shows a file's hunks in place, `RET` visits the file at the line shown, and `g` refreshes; the status also follows `.git`, the work tree and saved files. `s` stages, `u` unstages and `k` discards files and whole sections, asking first. Discarded untracked files go to the trash unless you set `(use-plugin git :config {discard-to-trash: false})`.
- Git shows the diff of the file at point beside the status, following point. `t` switches it between unified and side by side, which refits to the window, and changed words stand out. `s`, `u` and `k` work on hunks and on the region's lines, in the status and in the diff, and `RET` in the diff visits the line. `c` opens the commit transient (`-a`, `-e`, `-n`, `-s`; commit, amend, extend, reword), whose message buffer shows the staged changes beside it, flags a summary over 50 characters, and takes `C-c C-c`, `C-c C-k`, `M-p` and `M-n`.
- The `zap` plugin flashes the code you run, character for character, fading out over half a second: Ox forms and files evaluated with `C-x C-e`, `C-M-x`, `C-c C-b` and `C-c C-k`, the Ox REPL's input, R statements, functions, regions and R Markdown chunks, and code any plugin sends to a console from the buffer that holds it. Code that ran flashes in the marked face and code that failed in the flagged face; its `duration`, `face` and `error-face` settings change them.
- The `keycast` plugin shows the keys you press and the commands they run, for demo videos: keycaps and the command name in a pill at the bottom right, larger at the bottom center, or in the mode line, with the last three stacked, older ones fainter, repeats of a command counted as `×4`, and each gone after about two seconds. Typing never shows.
- Screensavers, the `screensaver` plugin, set with `(use-plugin screensaver :config {idle: 300})`: after that many seconds without input, while oxmacs has focus, a starfield or matrix rain fills the window area, keeping the mode line and minibuffer, until the first key or click, which does nothing else. `kind` picks `:starfield`, `:matrix` or `:random`, and `M-x screensaver-preview` shows either at any time.
- Conway's Game of Life, the `life` plugin: `M-x life` draws an acorn growing, with `SPC` to pause, `n` to step, `r` to start again and settings for the pattern and speed.
- R Markdown and Quarto: `.Rmd` and `.qmd` files open in R Markdown mode, with headings, chunk fences, chunk options and front matter set apart. `C-c C-c` or `C-S-RET` runs the R chunk at point, `C-RET` sends the statement in it and steps, `C-c C-a` runs every chunk and `C-c C-r` those above point; `M-n` and `M-p` move between chunks, `C-c C-i` inserts one and `M-g i` jumps to a heading or chunk. `M-;` comments with `#` inside R chunks and with `<!-- -->` in the prose around them. `C-c C-k` or `C-S-k` saves and renders the document with rmarkdown, or Quarto when rmarkdown or pandoc is missing, in an R process of its own as a background job showing its output beside, and opens the result; the r plugin's `render-on-save` renders on every save.
- Debugging R: when R stops in `browser()` or a function flagged with `C-c C-t d` (`C-c C-t u` clears it), its `Browse[1]>` prompt takes input like any other, the script opens at the line R is on, and RStudio's `<f10>`, `S-<f4>`, `S-<f6>`, `S-<f5>` and `S-<f8>` step, step in, finish, continue and stop, from the script or the console. Plugin consoles know prompts without markers from `:prompts` patterns, report lines matching `:watch` patterns, and tell the prompt they wait at with `console-prompt`.
- R jobs and apps: `M-x r-run-job` runs the saved script with Rscript in the background, apart from the session, with its output in `*R job: name*`, where `k` stops it and `g` runs it again. `M-x r-run-app` runs the Shiny app of the folder or project the same way, reloading as its files change, and opens it once it listens. htmlwidgets and other pages R sends to its viewer open in the system's browser.
- R scripts get R's languageserver package when it is installed: diagnostics from lintr, hover, references and the rest of the language server features, even before a session starts. Without a session, `C-M-i` and `TAB` complete and `M-.` finds definitions through it; with one, the session answers as before. The r plugin's `language-server` setting turns it off.
- R package work: in a project with a DESCRIPTION, `C-c C-w l` loads it, `d` documents it, `c` checks it, `i` installs it and `b` builds it with devtools, or with pkgload, roxygen2 and pkgbuild when devtools is missing, and RStudio's `C-S-l`, `C-S-d`, `C-S-e`, `C-S-b` and `C-S-t` do the same. `C-c C-w t` runs the tests with testthat and lists them by file in `*R tests*`, where `RET` visits a failure, and `M-x r-packages` lists the installed packages, `RET` attaching or detaching one and `h` showing its help index. `M-x r-renv-status`, `r-renv-snapshot`, `r-renv-restore` and `r-renv-init` run renv on the project.
- Each R project gets a session of its own: scripts under a folder with a DESCRIPTION, renv.lock, .git or .Rproj file send their code to that project's session, which starts in the project's folder, so its `.Rprofile` and renv load.
- `M-x r-layout` arranges the windows like RStudio: the script above its console on the left, the environment above the plots, or the help when there are no plots, on the right; `M-x r-layout-restore` puts back the windows as they were. Plugins read the windows as data with `window-layout` and lay them out with `arrange-windows`.
- R consoles remember their input across sessions: what you type is kept in `r-history` in the oxmacs home folder, and `M-p` brings back the last thousand inputs in a new session. Plugins read and set a console's input history with `console-history` and `set-console-history`.
- `C-c C-o` in R scripts and consoles shows the structure of the R object at point in the echo area.
- R tables sort by the column at point with `s` (up, down, then as they were), keep the rows where an R expression holds with `/`, page through a thousand rows at a time with `]` and `[`, and describe the column at point with `i`: its mean, spread and range, or its commonest values, and how many are missing.
- R help links the names under See Also: `RET` follows one, `TAB` and `S-TAB` move between them, `l` and `r` go back and forward through the pages seen, `x` runs the example at point in the console and `e` runs them all.
- The r plugin's `completion-delay` and `signature-delay` settings set how long typing rests before R completion pops up on its own and before a call's arguments show, 0.25 and 0.4 seconds by default.
- Fedora 44 and 45 packages, built by CI. A version tag publishes them to a dnf repository on GitHub Pages and attaches them to the release; every push to master publishes to a separate nightly repository, and only when every check passes. The package installs `oxmacs`, `ox` and a desktop entry, and on Linux the window carries the app id `io.github.collide2954.oxmacs`, so desktops match it to the entry.
- Every CI run keeps release builds of `oxmacs` and `ox` for Linux, macOS and Windows, and the Fedora RPMs, as downloads on its page; version tags attach them all to the release.

### Changed

- Settings are Ox: `(settings {theme: :adwaita-dark tab-width: 4})` in `~/.oxmacs/init.ox` sets them, and plugins take theirs from `use-plugin`'s `:config`. `custom.ox` and `init.ox` run before the window opens, so the theme, font and window mode apply from the first frame, and saving either runs both again from fresh settings. `M-x find-init-file` replaces `find-config-file`.
- `M-x customize` is a bundled Ox plugin and lists plugin settings too. `C-x C-s` writes `~/.oxmacs/custom.ox`, which runs before `init.ox`, so `init.ox` wins; a setting `init.ox` overrides says so.
- Every language has a mode command named after it: `ox-mode`, `rust-mode`, `toml-mode` and `r-mode`, and the modes plugins define, such as `git-commit-mode` and `r-markdown-mode`.
- `*scratch*` starts in Ox mode, so `C-x C-e` evaluates code there right away; `text-mode` turns it back into plain text.
- R support is the bundled `r` plugin, off until `(use-plugin r)` turns it on. Its consoles, sessions, sending keys, help, R keys, plots window with its gallery and pins, environment listing, table views, completion, signatures and definitions work as before, plots now drawn in a plugin view, `M-x run-r` finds R the same way, and the `r-program` setting is now the plugin's `program`: `(use-plugin r :config {program: "Rterm"})`. Syntax, indentation and air formatting stay built in.
- Dired is a bundled Ox plugin, with the same listings, sorting, marks and file operations on the same keys, and listings follow changes on disk. Turn off the trash with `(use-plugin dired :config {delete-to-trash: false})` instead of `delete-to-trash` in `config.toml`.
- Weather is the `weather` plugin, off until it is set up with `(use-plugin weather :config {location: "Berlin" units: :imperial mode-line: true})`, which replaces `weather-location`, `weather-units` and `mode-line-weather`. `M-x weather` shows the current conditions, the next 24 hours and the next 7 days from Open-Meteo, `g` refreshes and `q` quits, and with `mode-line` on the current weather sits beside the clock, refreshed every 30 minutes.
- The Markdown preview is the `markdown` plugin's `M-x markdown-preview` instead of `C-c C-c p`: the rendered buffer beside it, wrapped to the window, with headings, lists, `code`, highlighted Ox examples and links that `RET` follows, refreshed when the file is saved. Other plugins render Markdown with its `(markdown render)` module.
- `M-x list-processes` is a bundled Ox plugin. `s` and `S` sort, `/` filters, `TAB` shows a process's command, folder, status and environment, `k` asks it to end and kills it three seconds later if it hasn't, `K` kills it at once, and a sparkline shows its CPU over the last minute.
- `M-x list-tools` is a compact table: one line per tool, grouped by what uses it, with the version that runs and where it comes from, and the latest release; `TAB` opens a tool's summary, the full path it runs from and its source.
- Closing the window asks about unsaved files, as `C-x C-c` does. `auto-save` now keeps recovery copies of every file instead of saving Rust files in place; the new `auto-save-visited` saves files in place when idle.
- `rust-format-buffer` and `r-format-buffer` are now one command, `format-buffer`, still on `C-c C-f` in Rust and R buffers.
- `RET` leaves the line it ends alone in languages that keep the previous line's indentation, such as TOML, instead of re-indenting it to match the line above.
- `M-x` and other completion lists respond faster.
- oxmacs is open source under the Apache License 2.0.

### Removed

- `config.toml`. oxmacs no longer reads it, and the toml and toml_edit crates are gone; settings move to `(settings {…})` in `~/.oxmacs/init.ox`. The old file is left where it was.
- Markdown mode: `.md` files open as plain text, and the `markdown` plugin keeps the preview.
- The built-in IRC client, its bouncer and `oxmacs irc-daemon`, and passwords in the OS keychain; the `irc` plugin above replaces the client and keeps passwords in the vault.
- `M-x oxmacs-doctor`. `M-x list-tools` and `M-x list-languages` cover what it checked.

### Fixed

- After C-x C-w, or renaming or moving a file from the explorer or dired, its language server is told the file closed at its old place and opened at the new one, so edits, diagnostics and M-. follow it; the buffer also takes the language and editorconfig style of its new name.
- Language server edits that insert several pieces of text at the same place keep the server's order, instead of coming out reversed.
- Language server edits, renames, references and diagnostics reach a file's buffer when the server spells its path differently, such as a lowercase drive letter on Windows, instead of being dropped or opening it again; recovered changes, saves after a prompt and renames from the explorer or dired match the same way.
- Typing in very large files no longer waits for the whole file to be parsed again after each key, down from 655 ms to 6 ms a key in a 20,000-line Rust file. Syntax highlighting catches up a quarter of a second after you stop typing.
- Saving through a symlink writes the file it points to and keeps the link; a file with hard links, one another program holds open, or one in a folder that can't take new files is written in place; and on Unix saving keeps the owner. A file opened again through a symlink or in other letter case reuses its buffer.
- Answering a revert, reread, save-over or kill question acts on the buffer it asked about, even if another window was chosen meanwhile, instead of whichever buffer had moved into its place.
- Sending R code several times while R is still starting goes to that one session in the order it was sent, instead of starting a session for each.

### Security

- Updated rustls-webpki and webpki-roots, which check the certificates of IRC servers and downloads, for RUSTSEC-2026-0381 and RUSTSEC-2026-0382.

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

- After C-x C-w, or renaming or moving a file from the explorer or dired, its language server is told the file closed at its old place and opened at the new one, so edits, diagnostics and M-. follow it; the buffer also takes the language and editorconfig style of its new name.
- Language server edits that insert several pieces of text at the same place keep the server's order, instead of coming out reversed.
- Language server edits, renames, references and diagnostics reach a file's buffer when the server spells its path differently, such as a lowercase drive letter on Windows, instead of being dropped or opening it again; recovered changes, saves after a prompt and renames from the explorer or dired match the same way.
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

- After C-x C-w, or renaming or moving a file from the explorer or dired, its language server is told the file closed at its old place and opened at the new one, so edits, diagnostics and M-. follow it; the buffer also takes the language and editorconfig style of its new name.
- Language server edits that insert several pieces of text at the same place keep the server's order, instead of coming out reversed.
- Language server edits, renames, references and diagnostics reach a file's buffer when the server spells its path differently, such as a lowercase drive letter on Windows, instead of being dropped or opening it again; recovered changes, saves after a prompt and renames from the explorer or dired match the same way.
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

- After C-x C-w, or renaming or moving a file from the explorer or dired, its language server is told the file closed at its old place and opened at the new one, so edits, diagnostics and M-. follow it; the buffer also takes the language and editorconfig style of its new name.
- Language server edits that insert several pieces of text at the same place keep the server's order, instead of coming out reversed.
- Language server edits, renames, references and diagnostics reach a file's buffer when the server spells its path differently, such as a lowercase drive letter on Windows, instead of being dropped or opening it again; recovered changes, saves after a prompt and renames from the explorer or dired match the same way.
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
