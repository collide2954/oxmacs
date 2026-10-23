# Ox tooling

Tools for writing Ox: a checker, interactive development in oxmacs in the style of CIDER, a formatter, a language server for other editors, and an LSP client so oxmacs can use other languages' servers. Settled with the user on 2026-10-22.

## Decisions

- **Order.** Analysis first; each phase builds on the one before.
- **Language servers.** Both: `ox lsp` for Ox first, then an LSP client in oxmacs for other languages.
- **Interactive tools.** All of them: evaluation with inline results, live docs, completion and `M-.`, an inspector and macro expansion, and tests at point with a stepping debugger.
- **Checking.** Names, arity and unused code first, then gradual types.
- **Integrations.** `ox fmt`, a CI-friendly `ox check` and `ox test`, and a tree-sitter grammar.
- **Docstrings.** A string first in a body with more forms is the doc: `(def (area w h) "Width times height." (* w h))` and `(def limit "Most retries." 5)`. Tests named after a function are its examples.
- **One command.** Every tool is a subcommand of `ox`: `ox check`, `ox lsp`, `ox fmt`, `ox test` and `ox profile`.
- **Severity.** Undefined names, wrong argument counts and unknown keyword arguments are errors; unused bindings and imports are warnings, silenced by a leading `_`. `ox check` fails on errors, and with `--strict` on warnings too.

## Phases

1. **Checker core.** Undefined names with suggestions, wrong argument counts and keyword arguments, unused bindings and imports, malformed forms; `ox check` with exit codes, `--format json` and `--format github`, and `--strict`; plugins and init.ox checked with the editor's natives known.
2. **Docstrings and evaluation.** Docstrings; `C-M-x` and `C-x C-e` showing `=> value` beside the form; `C-c C-k` loads the file; errors open `*ox stacktrace*`, whose frames jump to source; `C-c C-c` interrupts; checker findings underlined in Ox buffers.
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
