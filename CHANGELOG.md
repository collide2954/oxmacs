# Changelog

Notable changes to oxmacs, called mx before 0.12. The format follows [Keep a Changelog](https://keepachangelog.com/en/1.1.0/) and versions follow [Semantic Versioning](https://semver.org/).

## [Unreleased]

### Added

- `pdf` 0.1.0, opt-in: PDF files open as pages drawn by PDFium, which `M-x list-tools` installs (or the `library` setting names). Pages scroll past one after another, or one at a time with `c` (`continuous`), fit the window's width with `W` or whole pages with `H`, grow and shrink with `+` and `-`, and show at actual size with `=` (`zoom`). `n` and `p` turn pages, `SPC`, `DEL`, `C-n`, `C-p` and the mouse wheel scroll, `M-<` and `M->` go to the first and last page, `M-g g` to a page by number and `o` to a heading of the outline. `s` searches the whole document, marking every match and saying how many there are, and `C-s` and `C-r` step through them. Dragging over text selects it for `M-w`, links go to their page or open web addresses outside, `m` draws pages in the theme's colors (`theme-colors`), the mode line shows the page and scale, and a document that changes on disk is read again in place.
- `(ox pdf)` reads PDFs with PDFium: `pdf/library` finds the library and offers it in `M-x list-tools`, `pdf/open` reads a document's pages, title, author and outline, and `pdf/search` finds where text is on the pages. `(ui/pdf path)` draws the pages with a zoom, one page or all, the theme's colors, colored marks, moves to a page or by lines and screens, and tells Ox the page shown, the text dragged over and the links clicked.
- `open-view` takes `:watch`, a folder whose changes on disk reach `on-disk-change` while the view is open.
- `epub` 0.1.0, opt-in: EPUB books open as books, set in pages of real type in an installed serif (`typeface`, also sans or monospace), two pages side by side when the window is wide enough (`spread`), with the book's title and the chapter's at the top and page numbers at the bottom. `SPC`, `DEL`, the arrows and the mouse wheel turn pages, `]` and `[` move between chapters, `o` jumps from the table of contents, `s` searches the whole book and lists every match while `C-s` and `C-r` step through them, `+` and `-` change the size, and links inside the book work. Dragging over text selects it: `h` highlights it, `N` writes a note on it, `M-w` copies it and `H` lists the book's highlights and notes. Each book remembers its page, `M-x epub-recent` reopens one read lately, and `M-x epub-library` shows a folder of books as covers with how far you have read.
- `(ui/pages (ui/document blocks))` sets text in pages like a book: paragraphs, headings, quotes, list items, preformatted lines, images and rules made of runs with italics, bold, code and links, in a serif, sans or monospace typeface, broken between whole lines with headings kept with what follows, one page or two side by side, with a running header and page numbers, colored marks, text selected by dragging, clickable links and page turns from the mouse wheel. It tells Ox where each page starts, so a plugin can turn pages and remember its place.
- `(ox zip)` reads zip archives, such as EPUB books: `zip/entries` lists what is inside, and `zip/read` and `zip/read-base64` read an entry as text or bytes. `image/decode` makes an image from base64 bytes. Built on the zip crate.
- `xml/parse` reads XHTML with `:html true`: whitespace between elements stays and HTML entities such as `&nbsp;` and `&mdash;` are understood.
- Plugins open the files they claim with `:opens-files ["epub"]` in their `plugin.ox` and `(on-open-file "epub" f)`, instead of the file opening as text.
- `zen` 0.1.0, opt-in: `M-x zen` writes without distractions and comes back the same way. The text sits in one centered column (`width`, 80 characters), wrapped between words (`wrap-words`); the other windows, the mode lines, line numbers, tab bar and side window go away and return afterwards (`hide-chrome`); every paragraph but the one you are in fades (`dim`); the line you type on stays in the middle of the window (`typewriter`); and the frame fills the screen (`fullscreen`). Each part is on by default and has its own setting.
- Plugins change how the windows look with `set-presentation`, which centers a text column, hides the mode lines, line numbers, tab bar and side window, fades all but the current paragraph, keeps point's line in the middle and fills the screen, and wrap a buffer's long lines between words with `set-word-wrap`.
- `explorer` 0.1.0, on by default: a file tree in a side window, shown, moved to and hidden with `C-x C-d`. It needs no setup: it opens at the project of the current file (its git root, or the nearest folder holding a `.projectile`, `Cargo.toml`, `package.json`, `pyproject.toml`, `go.mod` or `plugin.ox`), opens the folders down to that file and marks it, and follows each file you switch to. Typing filters the tree to the files whose names match (`DEL` takes a letter back, `C-g` clears). Git changes color the names and mark them M, A, U, D or !, with a dot on the folders holding them, and error and warning counts from language servers and linters show beside files and add up on folders. `RET` or a click opens a file in the other windows or a folder in place, `SPC` shows a file without leaving the tree, `C-c C-n` and `C-c C-d` make a file or a folder, `C-c C-r` or `<f2>` renames or moves, `C-c C-c` copies, `C-c C-k` or `<delete>` deletes to the trash, dragging a file onto a folder moves it there, `C-c C-h` shows hidden and ignored files, and dragging the divider beside the tree resizes it. `C-x o` and `C-x 1` leave it alone, and `C-x 0` in it closes it.
- Plugins can show a buffer in a side window along the left or right of the frame with `show-side-window`, hear clicks, drags from line to line and typing in their buffers with `open-buffer`'s `:on-click`, `:on-drop` and `:on-type`, and read error and warning counts per file with `(diagnostic-counts)` and `on-diagnostics-change`.
- `tabs` 0.1.0, opt-in: workspace tabs across the top of the frame, each keeping its own windows with their buffers, points and scrolling. `C-x t 2` opens a tab, `C-x t 0` closes it and `C-x t u` brings it back, `C-x t o` or `C-TAB` and `C-x t O` or `C-S-TAB` go to the next and previous tab, `C-x t RET` goes to one by name, `C-x t r` names one, `C-x t m` moves it right, `C-x t n` duplicates it, and `C-x t f` and `C-x t b` visit a file or a buffer in a new tab. A tab is called after the buffer it shows until it is named. The bar appears with the second tab (`show: :always` keeps it up), takes the theme's colors with the current tab opening into the windows below, and its tabs are clicked to switch, closed with their × or a middle click, added with + and dragged to reorder.
- Plugins keep window layouts with `(store-windows key)`, bring them back with `(recall-windows key)`, drop them with `(forget-windows key)`, hear the windows change with `(on-window-change key f)`, and show tabs across the top of the frame with `set-tab-bar`.
- The bundled `languages` plugin, on by default, brings back the modes 0.14.0 removed: Python, Go, C, C++, TypeScript, TSX, JavaScript, JSON and Markdown, with highlighting, comments, indentation and their language servers (basedpyright, pyright or pylsp; gopls; clangd; typescript-language-server; marksman). `.md` files open in Markdown mode again, and the `markdown` plugin keeps the preview.
- `M-x r-markdown-preview` shows an R Markdown or Quarto document rendered beside it with the `markdown` plugin's preview, which the `r` plugin now requires.
- `define-language-server` takes `:programs`, servers to try in turn, such as basedpyright, pyright and pylsp, and `:runs true` when `M-x list-tools` can install a server by running its `:install` command.
- `.editorconfig` files choose tabs or spaces (`indent_style`), the indent size and the tab width for the files they cover, ahead of the `indent-tabs` and `tab-width` settings, and a mode can default to tabs with `defmode`'s `:indent {tabs: true}`.
- The `indent-tabs` setting indents with tabs, then spaces for what is left, instead of spaces only.
- `defmode` takes `:indent {width: 4 after: #"[:{]\s*$" dedent: #"^\s*}"}`: `RET` and `TAB` indent one width past a line that opens a block and back on one that closes it. In modes without it, `RET` and `TAB` keep the indentation of the line above instead of starting at the margin and inserting a tab.
- A `defmode` highlighter that takes a third argument gets the state it returned for the line before, by returning `{spans: [...] state: x}`, so highlighting can carry strings, comments and blocks across lines; after an edit, only the lines from the first changed one are highlighted again.
- `C-c C-t` (`M-x ox-test`) in Ox buffers runs the buffer's tests, unsaved edits included, as `ox test` would, and lists how each went in `*ox tests*` beside it.
- `oxmacs --help` and `oxmacs --version` print to the terminal they ran from, Windows included, and `oxmacs -q` or `--no-init` starts without running `custom.ox` and `init.ox`. Other options are refused instead of opening a buffer named after them, and `--` still makes what follows file names.
- Plugins move the buffers visiting a file or folder they renamed with `(move-visited-files from to)`.
- The downloads and the Fedora package include `THIRD-PARTY-LICENSES.html`, the licenses of the crates oxmacs and ox are built from, beside the JetBrains Mono font's license and the highlight queries' notices.

### Changed

- R Markdown prose is styled as Markdown's is, with quotes, list markers, `code`, **strong** text and links as well as headings.
- Markdown previews leave out the YAML front matter between `---` lines at the top of a document.
- oxmacs no longer builds image codecs it never uses, such as the AVIF encoder and OpenEXR, which iced's image support pulled in; it builds faster and its executables are smaller.

### Fixed

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
