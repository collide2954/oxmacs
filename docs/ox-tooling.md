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
- **Completion.** Names come from the running image: the module's own definitions, the prelude, special forms, `alias/name` for every import with an alias, and the names of `:only` imports. `C-M-i` completes in Ox buffers and the REPL; `TAB` completes the name before point and otherwise indents, until Ox has real indentation; and three typed characters of a name offer completions quietly, as in R, but not in strings or comments. A single answer is inserted; several open the popup. Locals aren't offered yet; the checker's walk could add them.

- **`M-.` and `M-,`.** `M-.` goes to where the name at point was defined, as the running image recorded it: a file opens at the line, and code without a file, such as the prelude and bundled plugins, opens read-only from its source. `M-,` comes back. Natives and special forms say so and point at `C-c C-d`.
- **Usages.** `M-?` lists every place the name at point is used in `*ox usages*`, where `RET` visits one, `n` and `p` move and `q` closes. It reads the code, so strings and comments don't count but `alias/name` does, and searches open Ox buffers, with their unsaved text, then the `.ox` files under the current file's folder and the oxmacs home. It matches by name, so a local with the same name in another module counts too; the checker's walk could tell them apart later.

Phase 3 slices:

1. **Describing names,** docs for the whole library, the signature at point and `C-c C-d`. Done.
2. **Completion** from the running image. Done.
3. **`M-.` and `M-,`** into the library and plugins, and usages. Done.

Phase 3 is done.

## `ox fmt`

Settled with the user on 2026-10-23.

- **Style.** The Clojure style guide, keeping the author's line breaks. Bodies of `def`, `defmacro`, `fn`, `let`, `when`, `if`, `do`, `try`, `catch`, `match`, `cond`, `module`, `test` and the like, and of any form whose name starts with `def`, `with-`, `when-` or `if-`, indent 2 from their opening bracket. A call's arguments line up under its first argument, or indent 1 when no argument shares the line with the function. Items of vectors, maps, sets and data lists line up under the first item.
- **Tidying.** No spaces just inside brackets, one space where two forms touch, closing brackets on their own line move up to the line before unless it ends in a comment, at most one blank line in a row, no trailing whitespace, and one newline at the end. Runs of spaces between forms on a line stay, so hand-aligned tables survive.
- **Safety.** Strings, interpolations inside them, raw strings, regexes and comments are never touched. The formatter reads its own output and refuses any result that reads differently from the input, and it leaves code with syntax errors alone.
- **Where it runs.** `ox fmt PATH…` formats files and folders in place; `ox fmt --check` lists files that need it and fails, for CI. In oxmacs, `TAB` and `RET` indent with the same rules, `TAB` completes once the line is indented, `C-c C-f` formats the buffer, and every save of an `.ox` file formats it first, like gofmt.

Phase 4 is done.

## `ox lsp`

Built on 2026-10-23 while the user was away.

- **What it does.** `ox lsp` speaks the Language Server Protocol over stdio. It publishes `ox check`'s findings as you type, and answers completion, hover (signature and doc), signature help, go to definition, references, rename, formatting with `ox fmt`, and document symbols.
- **Where answers come from.** The open file's own top-level definitions are read statically (`ox::names::definitions`), so they work before anything runs; everything else comes from a checking isolate, with the editor's natives for plugin files and init.ox exactly as `ox check` decides. Definitions in other open files are found too, and library definitions with a file on disk.
- **Shared code.** The call at point, the symbol at point, usages and top-level definitions live in `ox::names`, used by both oxmacs and the server.
- **Crates.** `crates/ox-lsp` on `lsp-server`, the protocol scaffold rust-analyzer uses; responses are plain JSON, so it doesn't depend on `lsp-types`. `ox-cli` passes in how to pick and build isolates, so the server itself only depends on `ox`.

Setting it up elsewhere:

- **Helix** (`languages.toml`):

  ```toml
  [language-server.ox]
  command = "ox"
  args = ["lsp"]

  [[language]]
  name = "ox"
  scope = "source.ox"
  file-types = ["ox"]
  comment-token = ";"
  language-servers = ["ox"]
  ```

- **Neovim** (0.11):

  ```lua
  vim.filetype.add({ extension = { ox = "ox" } })
  vim.lsp.config("ox", { cmd = { "ox", "lsp" }, filetypes = { "ox" }, root_markers = { "init.ox", ".git" } })
  vim.lsp.enable("ox")
  ```

- **VS Code** needs a small extension to start a server for a new language; that waits for the tree-sitter grammar in phase 9, which also gives the other editors highlighting.

Phase 5 is done.

## The LSP client

Settled with the user on 2026-10-23 and built the same night (phase 10, moved ahead of 6 to 9 because it is one of the editor's fundamentals).

- **Starting.** Opening a file whose language has a server starts it, if it is installed, in the project root: the nearest folder with one of the server's marker files, such as `Cargo.toml`, else the Git root, else the file's folder. One server runs per server and root. The mode line names it, with `…` while it starts and the server's progress, such as `rust-analyzer Indexing (40%)`. The `language-servers` setting turns this off.
- **Missing servers.** The first file that would use a missing server names it once in the echo area. `M-x list-tools` lists every server under "Language servers" with where it was found or how to get it; `i` on one offers to run its install command, such as `rustup component add rust-analyzer`, and runs it in `*compilation*` only after a yes.
- **Servers.** rust-analyzer, basedpyright or pyright or pylsp, taplo, marksman, clangd, gopls and typescript-language-server. Rust code adds others with `Editor::add_language_servers`.
- **Features.** Diagnostics underline as you type and replace cargo check for Rust files the server covers; completion pops up after three letters or a trigger character such as `.`, and `C-M-i` asks; the signature of the call at point shows in the echo area; `C-c C-d` shows the docs in `*doc*`; `M-.` goes to a definition and `M-,` comes back; `M-?` lists references in `*xref*`, where `RET` visits one and `n` and `p` step; `<f2>` renames everywhere, leaving files unsaved; `C-c .` offers the server's code actions; `C-c C-f` formats with the server when oxmacs has no formatter of its own for the language, and so does saving when `format-on-save` is on; inlay hints show faintly at the end of their line, because putting them inside the line would move every column the editor counts (`inlay-hints` turns them off).
- **Protocol.** The client is in the core (`editor/lsp`), with the process and framing in `external/lsp.rs`; servers run on their own threads and their messages arrive like other events. Positions are UTF-32 when the server agrees, else UTF-16. Documents are sent whole on each change. `M-x lsp-restart` restarts the buffer's server and `M-x lsp-log` shows `*language servers*`, the servers' log and error output. `ox-cli`'s tests drive the client against `ox lsp` end to end.
- **Later.** Incremental document sync for very large files, inlay hints inside the line, and servers from Ox (`(language-server …)` in init.ox).
