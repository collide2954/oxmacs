(module (export page))

(def page
  {id: "modules"
   title: "Modules and libraries"
   text: #r##"# Modules and libraries

## Files are modules

Each `.ox` file is a module. Its first form, `(module …)`, imports other modules and can list what it exports. A file without one imports nothing and shares everything.

```text
(module
  (import (ox string)
          (ox json :as js)
          (notes parse :only [headline]))
  (export summary))
```

- `(ox string)` brings in the `ox/string` module, whose names are called as `string/split`: the module's last word, a slash and the name.
- `:as js` calls a module by another name, as in `js/read`.
- `:only [headline]` also lets you call those names without the prefix.
- `(export summary)` keeps the module's other names private: anyone else who calls them gets a "not defined" error.

A module runs once, the first time something imports it, and everything that imports it shares it. Two modules that import each other are an import cycle, which is an error.

## Your names come first

A module's own definitions take the place of built-in names, macros included: a module that defines `each` calls its own. `ox check` warns about it, in case it was an accident. Mark a definition that means it with `^:shadow`, or leave built-ins out of the whole module with `:exclude` on `(ox)`, the built-in names themselves. `ox/drop` still reaches the built-in one.

```text
(module (import (ox :exclude [count])))

(def (count shelf) (ox/count shelf.books))
(def ^:shadow (drop column seconds) …)
```

## Where modules are found

`(import (notes parse))` names the module `notes/parse`. The `ox` command finds it as `notes/parse.ox` in the folder of the file it runs. In oxmacs, a plugin's files are modules named after the plugin: `parse.ox` in the notes plugin is `notes/parse`, and the entry file is `notes` itself.

In the REPL, or with `C-x C-e`, evaluate a `module` form to import into what you are working on:

```
(module (import (ox string)))
(string/upper "ox")
```

## The standard library is plugins

Ox has three layers, and only the last is written in Ox:

- Built-in names, such as `map`, `str` and `spawn`, are always there. The [Core library](ref:ox) lists them.
- The `ox/...` modules, such as `ox/string`, `ox/file` and `ox/http`, are Rust plugged straight into Ox through the bridge, and need only an import. The [Editor API](ref:oxmacs) is plugged in the same way.
- Everything else is a plugin.

So plugins do two jobs: they add features to the editor, and they are Ox's standard library, where shared Ox code lives. Some bundled plugins are on from the start; others, such as life, wait until `use-plugin` turns them on. Any plugin's modules can be imported by another plugin that requires it, so a plugin can be a library as well as a feature, or only a library. This manual renders its pages with the markdown plugin's renderer:

```
(module (import (markdown render)))
(get (render/render "# Notes\n\n- one\n- two" 40) :lines)
```

A plugin that imports another plugin's modules names that plugin in the `:requires` of its manifest, which makes them importable even when that plugin is off; see [Writing a plugin](plugins).

## Built-in modules

Ox itself brings these modules, each a plug into the Rust bridge. Import them as in `(import (ox file))`; the prelude needs no import.

```table
Module      What it gives
prelude     Numbers, collections, text, records, errors, pattern matching, tasks, channels and promises, there without an import
ox/string   Text: searching, splitting, joining, trimming, padding and changing case
ox/regex    Regular expressions: matching, finding, replacing and splitting
ox/json     Reading and writing JSON
ox/csv      Reading CSV into rows
ox/xml      Reading XML into maps, finding elements along paths, and writing XML
ox/math     Roots, powers, logarithms, angles, clamping and random numbers
ox/time     Dates and times: the clock, parsing, formatting, time zones and durations
ox/path     Taking paths apart and joining them, the home folder and ~ expansion
ox/file     Reading, writing, listing, copying, renaming and removing files and folders
ox/process  Running programs, talking to them while they run, and the environment
ox/net      TCP connections, with or without TLS
ox/http     HTTP requests and URL encoding
ox/crypto   Hashes, encryption with a password, random numbers, base64 and one-time codes
ox/image    Opening, editing, drawing on and saving images, with filters and color adjustments
ox/ui       Widgets for plugin views: text, rows, columns, buttons, images and canvases; in oxmacs only
```

## Workers

A module is also how work moves to another thread. `worker` loads a module in a fresh isolate on a thread of its own, calls one of its functions and returns a promise of the result:

```text
(await (worker 'stats/math 'crunch (range 1000000)))
```

Arguments and results must be plain data: numbers, strings, keywords, symbols, records and collections of them, and channels. Functions, atoms and promises can't cross. Workers find modules the way the `ox` command does; in oxmacs a worker can't load a plugin's modules, so use workers in code that `ox` runs.
"##})
