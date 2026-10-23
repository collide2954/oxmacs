# Ox tooling

Tools for writing Ox: a checker, interactive development in oxmacs in the style of CIDER, a formatter, a language server for other editors, and an LSP client so oxmacs can use other languages' servers. Settled with the user on 2026-10-22.

## Decisions

- **Order.** Analysis first; each phase builds on the one before.
- **Language servers.** Both: `ox lsp` for Ox first, then an LSP client in oxmacs for other languages.
- **Interactive tools.** All of them: evaluation with inline results, live docs, completion and `M-.`, an inspector and macro expansion, and tests at point with a stepping debugger.
- **Checking.** Names, arity and unused code first, then gradual types.
- **Integrations.** `ox fmt`, a CI-friendly `ox check` and `ox test`, and a tree-sitter grammar.
- **Docstrings.** A string first in a body with more forms is the doc: `(def (area w h) "Width times height." (* w h))` and `(def limit "Most retries." 5)`. Tests named after a function are its examples.
- **One command.** Every tool is a subcommand of `ox`: `ox check`, `ox lsp`, `ox fmt`, `ox test`, `ox profile` and `ox perf`.
- **Severity.** Undefined names, wrong argument counts and unknown keyword arguments are errors; unused bindings and imports are warnings, silenced by a leading `_`. `ox check` fails on errors, and with `--strict` on warnings too.

## Phases

1. **Checker core.** Undefined names with suggestions, wrong argument counts and keyword arguments, unused bindings and imports, malformed forms; `ox check` with exit codes, `--format json` and `--format github`, and `--strict`; plugins and init.ox checked with the editor's natives known.
2. **Docstrings and evaluation.** Docstrings; `C-M-x` and `C-x C-e` showing `=> value` beside the form; `C-c C-k` loads the file; errors open `*ox stacktrace*`, whose frames jump to source; `C-c C-c` interrupts; checker findings underlined in Ox buffers. The library's own functions get docstrings in phase 3, when `C-c C-d` shows them.
3. **Live docs, completion and `M-.`.** The signature at point in the echo area, `C-c C-d` docs with their examples, completion from the running image, `M-.` and `M-,` into the library and plugins, and usages.
4. **`ox fmt`.** One canonical style, `C-c C-f` and formatting on save.
5. **`ox lsp`.** The `ox-lsp` crate: diagnostics, completion, hover, signature help, definitions, references, rename and formatting.
6. **Inspector and macro expansion.**
7. **Tests and the debugger.** The test at point or the file's tests with results in the gutter; a stepping debugger with breakpoints and locals.
8. **Gradual types.** Annotations, inference and type errors, with inferred types shown faintly.
9. **Tree-sitter grammar.** `tree-sitter-ox` for Helix, Neovim and GitHub.
10. **LSP client.** oxmacs talks to rust-analyzer, pyright and other servers.

## The checker

- **Where it runs.** `Isolate::check(name, source)` returns diagnostics. It reads the file, loads its imports as a run would, expands every form, and compiles each one without running it, so malformed forms report the compiler's own error. Only `defmacro` forms run, so later forms can use those macros.
- **Names.** A walk over the expanded forms keeps the scopes the compiler keeps: parameters, `let`, named functions, `match` patterns and `catch` bindings, matched by name and hygiene marks. Anything else must be defined at the top level of the file, anywhere in it, or reached through an import or the prelude. Names that macros bring in from other modules arrive already resolved.
- **Hosts.** Plugin code calls the editor's natives and imports `(ox ui)`, which only the editor defines. `ox check` checks a file in a folder with a `plugin.ox`, and `init.ox`, in an isolate with every editor native and module installed (`oxmacs_ox::checking_isolate`), and other files in a plain one, where calling `message` is a mistake. The `ox` command lives in its own crate, `ox-cli`, so it can depend on the editor's bridge.
- **Output.** Human output shows the line with the name underlined, like rustc; `--format json` prints one object per finding and `--format github` prints workflow annotations.

Phase 1 slices:

1. **Undefined names,** with suggestions, reader errors and malformed forms, through `Isolate::check`.
2. **Argument counts and keyword arguments** for calls to functions defined in the file, imported or in the prelude.
3. **Unused bindings and imports,** and `ox check` with its formats, exit codes and `--strict`.
4. **Plugins and init.ox** checked with the editor known.

Phase 1 slices 1 to 4 are done.

## Interactive development

- **Evaluation runs as tasks.** `C-x C-e`, `C-M-x`, `C-c C-b` and `C-c C-k` read the forms in the buffer and evaluate them one at a time with `Isolate::spawn_eval_with`, each as a task, so the editor stays responsive and `C-c C-c` can cancel a runaway form. A form is expanded only after the one before it finishes, so macros defined earlier in the buffer work. Forms keep their place in the file: the text before them is replaced by spaces and newlines, so errors and frames point at the right line and column.
- **Results beside the form.** The value appears faintly after the form's last line, as `=> value`, and stays until that buffer changes; long values are cut, and the whole value is in the echo area. Errors show `!! message` in the error color.
- **Keys.** `C-c C-b` evaluates the buffer, as before, rather than interrupting as in CIDER; `C-c C-c` interrupts. `C-c C-k` saves first; for init.ox and plugin files, saving reloads them.
- **Stack traces.** An error with more than one frame opens `*ox stacktrace*` beside the code: the message, the error type, and each frame's function and place. `RET` visits a frame in the other window, opening code that has no file, such as bundled plugins, read-only from its source; `n` and `p` move and `q` closes. A single frame would only repeat the note, so it opens nothing.
- **Checking while typing.** Half a second after the last key in an Ox file, the checker runs on the buffer's text in its own isolate, kept apart from the running one, and its findings replace that file's diagnostics, underlined, echoed at point and counted in the mode line. Plugin files and init.ox are checked with the editor's natives known, as `ox check` does. Each file is checked in one module that is cleared each time.

Phase 2 is done.

## Live docs, completion and `M-.`

Decided on 2026-10-22 while the user was away, taking the recommended option each time; open to change.

- **What Ox knows about a name.** `Isolate::describe(module, name)` answers with its kind (function, macro, native, record, special form or value), its signature as written, its doc and the place it was defined. Functions keep their parameter list as written, and every `def`, `defmacro`, `defrecord` and `defonce` records its place when it runs, so a redefinition moves it.
- **Docs for the library.** Natives are documented in tables written in Ox syntax, `(inc x) "Adds one to x."`, one for Ox (`crates/ox/src/library/docs.ox`) and one for the editor (`crates/oxmacs-ox/src/docs.ox`), with a `(special …)` section for special forms. The prelude's functions and macros carry docstrings. Tests fail when a native has no entry, an entry names no native, an entry's parameters disagree with the native's arity, or a library function or macro has no docstring.
- **Names resolve where the code lives.** A plugin file's names are looked up in that plugin's module; everything else in `user`.
- **The signature at point.** Moving through a call in an Ox buffer or the REPL shows its signature and the first sentence of its doc in the echo area. It only fills an empty echo area, or replaces a signature it showed, so results and errors stay.
- **`C-c C-d`.** Shows the name at point, or the call around point, in `*ox doc*`: its kind and place, signature, doc, and examples. Examples are the tests whose title is the name, or starts with the name and a space, shown as written; `q` closes it.
- **`C-x C-e`** evaluates the expression just before point, even inside an unfinished list, as Emacs does.

Phase 3 slices:

1. **Describing names,** docs for the whole library, the signature at point and `C-c C-d`. Done.
2. **Completion** from the running image.
3. **`M-.` and `M-,`** into the library and plugins, and usages.
