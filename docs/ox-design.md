# Ox: the oxmacs extension language

Status: design, agreed in five rounds of questions on 2026-10-21. Nothing here is built yet.

The standard library is described in [ox-stdlib.md](ox-stdlib.md), and how the language is built in [ox-internals.md](ox-internals.md).

## Why

Every non-core feature of oxmacs is a Rust crate behind a cargo feature. Each change means recompiling and relinking a large workspace, and the commit gate has become the bottleneck of the whole project. Ox moves everything that is not the editor's engine into plugins written in a fast, modern Scheme dialect that loads at run time. Changing a plugin then means saving a file, not rebuilding the editor.

## Goals

- **Lean core, everything else a plugin.** The Rust core is the engine. Language modes, applets, themes, the start screen, doctor, customize and list-tools all become bundled Ox plugins, each with its own version.
- **Blazing fast, without Emacs's bottlenecks.** Emacs runs all Lisp on one thread that blocks typing, pauses for garbage collection, interprets slowly, and runs hooks synchronously on every keystroke. Ox avoids each of these by design (see "Speed").
- **Full integration.** Ox reaches everything the editor can do: buffers, keys, commands, the minibuffer, windows, processes, the mode line, and the GUI down to the canvas and text level.
- **Configuration in Ox.** `~/.oxmacs/init.ox` replaces `config.toml`.

## Decisions

| Topic | Decision |
| --- | --- |
| Engine | Our own VM in Rust: a new `ox` crate with reader, hygienic expander, bytecode compiler, register VM and GC. A Cranelift JIT tier may come later. |
| Threads | The UI isolate runs editor code in small time-boxed slices. Slow work runs in worker isolates with their own heaps, which talk by messages and read buffers as immutable rope snapshots. async/await for I/O. |
| Native code | Core primitives only. The core provides fast native building blocks; plugins are pure Ox and nothing is feature-gated. |
| Migration | Prove it, then port: the VM and editor bindings first, then weather end to end, then one applet at a time, deleting each Rust crate as its plugin lands. |
| Dialect | Ox's own dialect, Scheme-flavored and free to break compatibility where that makes it cleaner. |
| Words | Short modern words: `def`, `fn`, `let [x 1]`, `if`, `cond`, `match`, `do`, `set!`. |
| Additions | Modules with explicit imports and exports; hygienic `syntax-rules` and procedural macros; lexical scope only; `[ ]` vectors, `{ }` maps, `:keywords`, string interpolation; `match`, records, destructuring. |
| Types | Gradual with inference: annotations are optional, but the checker infers through unannotated code and reports definite errors as you type, in `ox check` and in the gate. Nil safety, generics, map shapes, unions and aliases, and exhaustive `match`. |
| Tests | Built in: inline `(test …)` blocks with an `expect` that explains failures, doc examples, property tests, snapshot tests, `ox test` and an editor runner. |
| Data | Immutable by default: persistent vectors, maps and strings. Atoms and buffers are mutable. |
| Control | Proper tail calls, async/await, generators and green tasks. No `call/cc`. |
| Errors | Exceptions with typed error records and clickable tracebacks. A failing hook is reported, never fatal. |
| GUI | Declarative, Elm-style view trees mapped onto Iced widgets; a canvas draw-list API; buffer decorations. Rust diffs and renders. |
| Config | `~/.oxmacs/init.ox` is the only config file. `M-x customize` writes `custom.ox`. `config.toml` is migrated once. |
| Folder | `~/.oxmacs` on every system (`OXMACS_HOME` moves it). Caches go in the platform cache folder. |
| Plugins | Lazy, versioned folders: `plugin.ox` names what wakes a plugin, and nothing loads before that. |
| Sources | Bundled plugins, your own in `~/.oxmacs/plugins`, and git URLs pinned in `~/.oxmacs/plugins.lock`. |
| Trust | Full access, like Emacs packages. |
| Extension | `.ox` |
| Speed | Benchmarks with budgets gate every push. |
| Superseded | The Rhai `scripting` applet and the routines plan are dropped; keyboard macros stay in core. |

## The language

```clojure
(module weather
  (import (ox editor) (ox http) (ox json) (ox ui))
  (export weather weather-refresh))

(def-record forecast day high low sky)

(def (forecast-url [city : String]) : String
  "https://api.open-meteo.com/v1/forecast?name={city}")

(def (weather [city : String])
  (let [reply (await (http-get (forecast-url city)))]
    (match reply
      [{status: 200 body: b} (show-forecast (parse-days (json/read b)))]
      [{status: s} (raise (http-error :status s))])))

(defcommand weather-refresh
  "Fetch the forecast again"
  (weather (setting 'weather.location)))
```

- **Reading.** `( )` lists, `[ ]` vectors, `{k: v}` maps, `:key` keywords, `"…{expr}…"` interpolation with `a.b` field access inside braces, and `#;` datum comments. The reader keeps exact source spans, so errors, tracebacks and highlighting point at the right characters.
- **Definitions.** `def` for values and functions, `fn` for lambdas, and `let` with vector bindings and destructuring: `(let [{name: n} city] …)`.
- **Modules.** Each file is one module with explicit imports and exports. There is no single global namespace; plugins see only what they import.
- **Macros.** Hygienic `syntax-rules`, plus procedural `defmacro` with hygiene helpers for anything harder.
- **Scope.** Lexical only. Parameters (`with-param`) replace dynamic variables, with fast lookup.
- **Types.** Annotations are optional; see "Types" below.
- **Numbers.** 64-bit integers that grow into bignums, and 64-bit floats.
- **Strings.** Immutable UTF-8; large ones are ropes. Characters are Unicode scalar values.
- **Collections.** Persistent vectors (RRB trees), maps and sets (HAMT). Updates share structure; `transient` batches edits inside one function.
- **Mutation.** `atom` cells and editor objects (buffers, windows) are the mutable things.
- **Control.** Tail calls are guaranteed. `async`/`await`, generators (`yield`) and `spawn` for green tasks.
- **Errors.** `raise` any record value; `try`/`catch` matches on its type. The REPL and echo area show tracebacks you can click.

## Runtime and speed

Each of Emacs's bottlenecks has a planned answer in Ox:

| Emacs bottleneck | Ox |
| --- | --- |
| One Lisp thread blocks typing | Editor code runs in time slices on the UI isolate; slow code runs in workers. |
| Stop-the-world GC pauses | Per-isolate generational GC: a bump-allocated nursery with copying minor collections, and incremental major marking. Immutable data needs almost no write barriers. Target: p99 pause under 1 ms. |
| Slow interpreter | A register-based bytecode VM with inline caches for module and record lookups. Type annotations unbox numbers. A Cranelift JIT tier is possible later. |
| Hooks run synchronously on every keystroke | Hooks are prioritized and can be async; a keystroke never waits on a slow hook. |
| Startup grows with packages | Plugins load lazily, and compiled bytecode is cached by source hash, so a cold start skips compiling. |
| Font-lock done with Lisp regexps | Highlighting stays native (tree-sitter, or Ox's reader for `.ox` files). Plugins add decorations as data. |

- **A stackless VM.** Ox frames live on the VM's own stack, not Rust's, so any task can pause at any instruction and resume later. That is what makes time slicing and green tasks possible without threads.
- **Scheduling.** Each frame gives the UI isolate a budget of about 4 ms for Ox work. A task that uses up its slice is paused and resumed after the frame is drawn, and input is handled first. `C-g` interrupts a runaway task.
- **Isolates.** The UI isolate owns the editor. `spawn-worker` starts a worker isolate with its own heap on a thread pool. Messages are immutable values: large ones go into a shared, frozen, reference-counted arena instead of being copied. Buffer contents cross as rope snapshots that cost nothing to share.
- **The bridge.** Native primitives are Rust functions registered through a small `#[ox::native]` macro. Strings and ropes cross without copying. Rust holds Ox values through rooted handles.

## Editor API (first version)

- **Buffers.** Open, create and kill buffers; read text through snapshots; edit through transactions that form one undo step. Point, mark, markers and selections.
- **Commands and keys.** `defcommand` (with documentation and an interactive spec), `defkeymap`, `bind-key`, prefix keys, transient-style menus.
- **Modes.** `defmode` for major modes that extend `prog` or `text`, and minor modes. Language modes provide a tree-sitter grammar or highlighter, indentation guesses, comment syntax, pairs, outline rules, formatters, linters and statement rules.
- **Minibuffer.** `prompt`, `pick` with async completion sources, `yes-or-no`, and file and buffer pickers.
- **Hooks.** `add-hook` with priority and async support: before and after save, buffer opened, mode entered, idle, and more.
- **Processes.** `spawn-process` with async line streams and consoles (REPL buffers), `*compilation*`, error patterns, and timers.
- **Settings.** `defsetting` with a type, default and documentation, which feeds `M-x customize`, `custom.ox` and doctor findings.
- **Display.** Mode-line segments, faces and themes as Ox data, `message`, and notifications.

## GUI API

```clojure
(def (view state)
  (column :spacing 8
    (text "Forecast" :size 18 :face 'heading)
    (row (for [day state.days] (day-card day)))
    (canvas :height 120
      (draw-path (temperature-curve state.days) :stroke 'accent :width 2))))
```

- **Views.** A plugin buffer or panel has a `view` function from its state to a tree of widgets: `column`, `row`, `text`, `rich-text`, `button`, `input`, `scrollable`, `stack`, `table`, `image` and `canvas`. Events become messages, and an `update` function turns a message and the state into a new state, Elm-style, like Iced itself. Rust diffs the trees and renders them, so a plugin never blocks a frame.
- **Canvas.** Draw lists of paths, fills, strokes, text runs, images and clip regions, with hit testing for clicks and drags. This is enough to port the database grid, the git log graph and the PDF pages.
- **Buffer decorations.** Faces on ranges, overlays, virtual text, gutters, pills, fringe marks and inline widgets. They are applied as data and rendered natively.
- **Placement.** Buffers, side panes, the area above the echo area, pop-ups and transient menus.

## Plugins

```clojure
; plugins/weather/plugin.ox
(plugin weather
  :version "0.2.0"
  :requires [(ox ">= 0.1") (http)]
  :commands [weather weather-refresh]
  :mode-line #t
  :settings [location units mode-line]
  :entry "weather.ox")
```

- **Wake-ups.** The manifest names what wakes the plugin: commands, file types, keys, hooks, mode-line segments, or startup. Before that, only the manifest is read.
- **Versions.** Plugins have semver versions and `:requires` constraints, checked when they load. `M-x list-plugins` shows each plugin's version, source, load state and load time.
- **Sources.** Bundled plugins ship with oxmacs, under `plugins/` beside the binary. Your own live in `~/.oxmacs/plugins`. `(use-plugin "github:someone/ox-thing" :version "^1.2")` clones a plugin and pins its commit in `~/.oxmacs/plugins.lock`.
- **Live reload.** Saving a plugin's file reloads its module, and the REPL can redefine anything at run time.

## Configuration

```clojure
; ~/.oxmacs/init.ox
(set! theme 'modus-vivendi)
(set! tab-width 4)

(use-plugin git)
(use-plugin weather :config {location: "Austin" units: 'metric})

(bind-key "C-c w" 'weather)
```

```
~/.oxmacs/
  init.ox          your configuration
  custom.ox        written by M-x customize
  plugins/         your plugins and cloned ones
  plugins.lock     pinned commits of cloned plugins
```

Caches (compiled bytecode, installed tools such as air, ruff and PDFium) live in the platform's cache folder. On first start, `config.toml` is translated into `init.ox`, with a note of what moved.

## What stays in the Rust core

- **Text.** Ropes, buffers, undo, markers, search and replace, and keyboard macros.
- **Syntax.** Tree-sitter infrastructure and bundled grammars.
- **Input.** Keymaps, command dispatch, the minibuffer and completion.
- **Display.** Windows and layout, rendering, the Iced shell, and the host for widgets and the canvas.
- **System.** Files and file watching, processes and consoles.
- **Ox.** The VM and its native primitives:
  - processes, files, paths, globs and watchers;
  - HTTP(S), sockets and TLS (for IRC);
  - JSON, TOML and Markdown parsing;
  - regex and tree-sitter parsing and queries;
  - SQLite;
  - images and PDF pages through PDFium;
  - git through the CLI;
  - the clipboard, timers and notifications.

## Migration

| Phase | Work | Done when |
| --- | --- | --- |
| 1. The `ox` crate | Reader with spans, expander, compiler, stackless VM, GC v1, core library, module loader, bytecode cache. Its own test suite of `.ox` programs, plus a benchmark harness. | Ox programs run at their benchmark budgets. Independent of the editor, so it builds fast. |
| 2. Embedding | The bridge in core: UI isolate, scheduler, editor API v1, `init.ox`, plugin manifests and lazy loading, `M-x ox-repl`, `C-x C-e`, eval-buffer, an `.ox` mode. | A plugin adds a command, key, setting and mode-line segment, and live reload works. |
| 3. Proof | Port weather (HTTP, JSON, settings, mode line, a forecast buffer), then delete `oxmacs-weather`. | Weather works the same, and is faster to change. |
| 4. GUI API | Views, canvas and decorations. Port the start screen (canvas animation) and demo. | The animated start screen runs as a plugin. |
| 5. Applets | Port themes, glow, dired, markdown, atlas, irc, git, database, pdf and python, one at a time, each deleting its crate. | No applet crates left; no cargo features. |
| 6. Built-ins | Rust, TOML and R modes (with the R console), doctor, customize and list-tools become bundled plugins. | The core is the engine alone. |
| 7. Cleanup | Remove `config.toml` support after migrating it, the Rhai applet, the routine parser and every feature flag. | One binary plus plugins. |

Work paused for Ox continues later as Ox plugins: the list-tools and doctor reorganization, database and git slices, and the Vim-style modal editing idea.

## Speed budgets

Speed budgets are checked in pre-push and fail on regression:

- startup to the first frame, with every bundled plugin installed: under 100 ms;
- keystroke to frame, with typical plugins hooked in: under 4 ms;
- GC pause at the 99th percentile: under 1 ms;
- a call from Rust into Ox and back: measured and tracked;
- loading a plugin from the bytecode cache: measured per plugin.

## Types

```clojure
(deftype Units (or :metric :imperial))
(deftype City {name: String lat: Float lon: Float zip: String?})

(def (describe [city : City] [units : Units]) : String
  (match units
    [:metric "{city.name} in °C"]
    [:imperial "{city.name} in °F"]))

(def (first-or [items : (Vector T)] [fallback : T]) : T
  (if (empty? items) fallback (first items)))
```

- **Gradual, with inference.** Annotations are optional, and untyped code always runs. The checker infers types through unannotated code from literals, locals and calls to typed functions. It reports only definite errors: a value that cannot be what its use needs.
- **Where it runs.** In `.ox` buffers as you type, with underlines; in `ox check`; and in the gate.
- **Speed.** Typed code runs faster, because annotated numbers get unboxed instructions.
- **Nil safety.** `String` never holds `nil`; `String?` may. Before a `String?` can be used as a `String`, the checker requires a nil test (`if`, `when-let`, `match`).
- **Generics.** `(Vector T)`, `(Map K V)` and generic functions such as `(fn [T] T)` are checked per use.
- **Map shapes.** `{name: String lat: Float}` is a type, so plain maps get checked fields, as records do. Extra keys are allowed unless the shape is marked closed.
- **Unions and aliases.** `(deftype Units (or :metric :imperial))` names a type. Unions can mix keywords, records and other types.
- **Exhaustive `match`.** A `match` over a union, a record type or `T?` must handle every case. The checker names the missing ones.
- **At run time.** Annotated parameters are also checked at function entry, so typed code can trust its types even when untyped code calls it.

## Testing

```clojure
(def (add a b)
  "Adds two numbers.
  (add 1 2) ;=> 3"
  (+ a b))

(test "add sums its arguments"
  (expect (= (add 1 2) 3))
  (expect (= (add -1 1) 0)))

(test "add commutes"
  (for-all [x Int, y Int]
    (expect (= (add x y) (add y x)))))

(test "the forecast view renders"
  (expect-snapshot (render (forecast-view sample))))
```

- **Inline tests.** `(test "name" …)` blocks live next to the code they test, as in Rust. Loading a plugin skips them; `ox test` and `M-x ox-test` run them.
- **`expect`.** It understands the expression it checks. A failure shows each sub-expression's value and the exact location:

  ```
  weather.ox:12:3  add sums its arguments
    (= (add 1 2) 4)
       (add 1 2) => 3
  ```

- **Doc examples.** Lines in a docstring written as `(add 1 2) ;=> 3` run as tests, so documentation cannot go stale.
- **Property tests.** `for-all` generates values for its typed variables, tries many, and shrinks a failure to the smallest example.
- **Snapshot tests.** `expect-snapshot` compares output with a stored file next to the test. When the output changes on purpose, the runner offers to update the file.
- **Runner.** `ox test` runs from the command line and in the gate. In the editor, a `*tests*` buffer lists results with clickable failures; tests can be run at point and rerun on save.

## Editor support

oxmacs's support for Ox should be the best place to write Ox. The compiler, checker and test runner run inside the editor, so nothing goes through a separate server.

- **Live development, like CIDER.** Code evaluates in the running editor:
  - `C-x C-e` shows the result inline beside the form;
  - `C-c C-k` reloads the file in place;
  - results are live values you can open and explore;
  - errors jump to source.

  A scratch worker isolate is one key away, for experiments that should not touch the editor.
- **Language intelligence.**
  - **Writing.** Completion from locals, imports, modules and types; the signature of the call at point in the echo area; hover documentation with its examples.
  - **Navigation.** Go to definition, into the standard library too; find references; rename a name across files.
  - **Checking.** Inferred types shown faintly beside bindings, and reader, type and test errors underlined as you type, with explanations.
  - **Code actions.** Add the missing import, handle `nil`, fill in the missing `match` cases, extract a function.
- **Structural editing by default.**
  - Brackets always balance.
  - Slurp, barf, raise, splice and wrap; motion by form; a selection that grows form by form; whole-form deletion.
  - Indentation follows Ox's forms.

  One key turns it off.
- **Formatting.** One canonical style, like gofmt: `ox fmt`, `C-c C-f`, and formatting on save. There are no style settings.
- **Debugging and inspection.**
  - A value inspector that walks into maps, vectors, records, closures and buffers, with large collections paged.
  - A stepping debugger: breakpoints in the gutter, step in, over and out, locals for every frame, and evaluation in a paused frame.
  - A macro expander that expands the form at point one step or fully, shown inline or beside the code.
  - Tracing of a function's arguments and results, and profiling of CPU and allocations per plugin, shown as a flame graph.
- **Tests.** The `*tests*` runner: run the test at point, see failures in the gutter and rerun on save.
- **Plugin development.**
  - `M-x ox-new-plugin` scaffolds a plugin folder with `plugin.ox`, an entry module and a test.
  - An `*ox log*` buffer lists each plugin's errors, warnings, load times and reloads, linked to source.
  - `M-x list-plugins` installs, updates, pins, disables and removes plugins, and shows their versions, sources and wake-ups.
  - `M-x ox-docs` is a searchable reference of the standard library and every loaded plugin, built from docstrings with runnable examples.
- **Other editors.** `ox lsp` is a language server built in phase 1, reusing the same reader, checker, formatter and test runner, so VS Code, Helix and Neovim users can write Ox too. It lives in its own crate, `ox-lsp`, so the `ox` crate stays free of protocol dependencies.

## Open questions, for later

- How plugins can bring tree-sitter grammars the core does not bundle (loading compiled grammar libraries, as Helix does?).
- When to add the JIT tier.
- A central plugin registry.
- How themes are written as Ox data, and whether old theme files are converted.
- Language servers: a core client that plugins configure, or a pure Ox client?
