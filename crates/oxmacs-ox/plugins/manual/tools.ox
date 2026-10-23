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
ox lsp                 serve Ox to other editors over LSP
```

## Checking

`ox check` reads files and reports unknown names, wrong numbers of arguments, unused variables and malformed forms, with the line and column of each. In a plugin folder it knows the editor API too. `--format json` and `--format github` suit tools and CI.

## Formatting

There are no style settings: `ox fmt` keeps your line breaks and fixes indentation and spacing. oxmacs formats `.ox` files the same way when you save them.

## Testing

`ox test` runs every `test` block it finds and prints each failure with the values that made it fail. It exits with a failure status when a test fails, for CI.

## Other editors

`ox lsp` gives VS Code, Helix and Neovim diagnostics, completion, signatures, documentation on hover, definitions, references, renaming, an outline and formatting for Ox.
"#})
