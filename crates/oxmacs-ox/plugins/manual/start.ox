(module (export page))

(def page
  {id: "start"
   title: "Getting started"
   text: #r#"# Getting started

Ox is the language you extend oxmacs in: a small Lisp with immutable data, string interpolation and lightweight tasks. Everything in this manual runs inside oxmacs.

## Run your first code

Put point on the example below and press `x`. It runs in the REPL beside this window.

```
(str "Hello, " "Ox")
```

`M-x ox-repl` opens the REPL yourself. It evaluates what you type when you press `RET`, and `M-p` and `M-n` recall earlier input.

## Work in a file

Open a file ending in `.ox` with `C-x C-f`. Ox buffers have these keys:

- `C-x C-e` evaluates the form before point and shows the result beside it.
- `C-M-x` evaluates the top-level form around point.
- `C-c C-b` evaluates the whole buffer; `C-c C-k` saves and loads it.
- `C-c C-d` shows the documentation of the name at point, `M-.` jumps to its definition, `M-,` comes back and `M-?` lists its uses.
- `TAB` indents, or completes the name you started typing.
- `C-c C-z` opens the REPL, and `C-c C-c` stops an evaluation that runs too long.

oxmacs checks the file once you pause typing, marking unknown names and wrong numbers of arguments, and formats it when you save.

When code you evaluate fails, the error shows beside the form and in `*ox log*`. An error raised inside other calls also opens `*ox stacktrace*`, where `RET` on a frame visits its source.

## Configure oxmacs

`~/.oxmacs/init.ox` runs when oxmacs starts; set `OXMACS_HOME` to keep that folder elsewhere. `M-x find-init-file` opens it, and saving it applies it at once.

```
(settings {tab-width: 4})
(bind-key "C-c d" 'dired)
```

`M-x customize` changes the same settings in a form and saves them to `~/.oxmacs/custom.ox`, which runs before `init.ox`, so `init.ox` wins. When `init.ox` fails, oxmacs keeps what ran before the error and shows `*ox log*`.

## Next

- [The language](language): values, names and functions.
- [Modules and libraries](modules): imports, and plugins as the standard library.
- [Extending oxmacs](editor): commands, keys and hooks.
- [Buffers and views](ui): showing your own text and widgets.
- [Writing a plugin](plugins): packaging what you built.
"#})
