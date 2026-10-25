(module (export page))

(def page
  {id: "tools"
   title: "The ox command"
   text: #r#"# The ox command

`ox` runs Ox outside the editor and holds Ox's tools, each a subcommand.

```text
ox                     open a REPL
ox FILE                run a file
ox check PATH…         find mistakes without running anything
ox fmt PATH…           format files in the one Ox style
ox fmt --check PATH…   list files that are not formatted
ox test PATH…          run the test blocks in files or folders
ox profile FILE        run a file and show where the time went
ox perf bench FILE     benchmark the functions a file returns
ox perf sample FILE NAME  sample where one benchmark spends its time
ox lsp                 serve Ox to other editors over LSP
```

Outside the editor there is no editor API. `ox FILE`, `ox test` and the REPL have the core library and the `ox/...` modules, and find a file's imports in its folder; in a plugin folder they also find the other plugins' modules.

## Checking

`ox check` reads files and folders and reports unknown names, wrong numbers of arguments, unused variables and malformed forms, with the line and column of each. It knows the editor API in `init.ox` and in a plugin folder, along with the plugin's own modules. oxmacs runs the same checks as you type.

- `--strict` fails on warnings as well as errors.
- `--root DIR` adds a folder to find imports in.
- `--format json` and `--format github` suit tools and CI.

A name that starts with `_`, such as `_line`, is unused on purpose and draws no warning.

## Formatting

There are no style settings: `ox fmt` keeps your line breaks and fixes indentation and spacing. oxmacs formats `.ox` files the same way when you save them.

## Testing

`ox test` runs every `test` block in the files and folders it is given, or in the current folder, and prints `ok` or `FAIL` for each, with the values that made a failing `expect` fail:

```text
ok    greeting names the person
FAIL  greeting is polite
      words.ox:9:3
      (= (greeting "Bo") "Hi, Bo")
        (greeting "Bo") => "Hello, Bo!"
2 tests, 1 failed
```

A test fails when its body raises, and `expect` raises when its check is false. Each file runs in a fresh isolate without the editor and runs only its own tests; a file that calls the editor while it loads is listed as `skip`, so keep the logic you test in modules that don't need it. `ox test` exits with a failure status when a test fails, for CI.

## Profiling

`ox profile FILE` runs a file and prints a call tree of where the time went, with the calls and the place of each function. In oxmacs, `M-x ox-profiler-start` and `M-x ox-profiler-stop` do the same for the editor's own Ox code, and `profile` profiles one function, giving its report as a map:

```
(string? (get (profile (fn [] (reduce + (range 100000)))) :report))
```

## Benchmarks

`ox perf bench FILE` runs a file whose last form is a map of names to functions, calls each over and over, and prints its best time and how much it allocated. A `[count function]` pair also shows the time for each of count items. Name benchmarks after the file to run only those.

```text
(def (fib n) (if (< n 2) n (+ (fib (- n 1)) (fib (- n 2)))))

{fib: (fn [] (fib 15))
 sum: [1000 (fn [] (reduce + (range 1000)))]}
```

`ox perf sample FILE NAME` samples where Ox's Rust code spends one benchmark's time, for work on Ox itself.

## Other editors

`ox lsp` gives VS Code, Helix and Neovim diagnostics, completion, signatures, documentation on hover, definitions, references, renaming, an outline and formatting for Ox.
"#})
