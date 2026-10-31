(module (export page))

(def page
  {id: "plugins"
   title: "Writing a plugin"
   text: """
# Writing a plugin

A plugin is a folder under `~/.oxmacs/plugins` with a manifest and Ox modules. It costs nothing until it is used: at startup oxmacs reads its files but runs none of them, and the code loads the first time one of its commands, keys or modes is used.

## Plugins are the standard library

Ox has no standard library of its own. The built-in functions and the `ox/...` modules, such as `ox/string` and `ox/json`, are primitives: Rust code plugged straight into Ox through the bridge, as the editor API is. Everything else is a plugin.

So plugins do two jobs. They extend the editor, and they are where shared Ox code lives. Some bundled plugins, such as dired and git, are on from the start; others, such as life, wait until `use-plugin` turns them on. A plugin can import the modules of another plugin it requires, as this manual does with `(import (markdown render))`, so a plugin you write can be a library as well as a feature. See [Modules and libraries](modules).

## Make one in four steps

1. Create the folder `~/.oxmacs/plugins/hello`, named after the plugin.

2. Add `plugin.ox`, the manifest. It names the commands that wake the plugin and the file that defines them:

```text
(plugin hello
  :version "0.1.0"
  :summary "Greetings"
  :commands [hello]
  :settings {name: {type: :text default: "world" doc: "Who to greet"}}
  :entry "hello.ox")
```

3. Add `hello.ox`, the entry:

```text
(defcommand hello "Say hello"
  (message $"Hello, {(setting :name)}!"))
```

4. Press `C-c C-k` in `hello.ox`. The plugin starts, and `M-x hello` works. Plugins in your folder stay off until you turn them on, so to have it in every session, add this to `init.ox`:

```text
(use-plugin hello :config {name: "Ana"})
```

## Develop it live

Saving a loaded plugin's file evaluates it again in place, so a change takes effect at once; `plugin.ox` itself is read again only when oxmacs starts. Values made with `defonce` keep their state across reloads; use it for atoms. `C-x C-e` evaluates a form inside the plugin's own module, and errors show beside the form and in `*ox log*`. When the plugin loads, oxmacs checks that its entry defined every command the manifest lists.

## Manifests

- `:version` follows semantic versioning, and `:summary` says what the plugin does.
- `:requires` names the plugins it uses with the versions it works with, as `{markdown: "^0.2" vault: ">=0.2, <1"}` (`"*"` takes any version), and `ox:` the oxmacs version, as `{ox: ">=0.14"}`. If one is missing or the wrong version, the plugin stays off and `*ox log*` says why.
- `:entry` names the entry file, the plugin's name with `.ox` unless given.
- `:commands` names the plugin's commands, as `[hello]`. They exist from startup with the summaries their `defcommand`s give, and load the plugin when first run.
- `:keys` binds global keys, such as `{"C-c h" hello}`; your `init.ox` can rebind them.
- `:mode-keys` binds keys in the buffers of a mode, such as `{"R" {"C-c h" hello}}`, and the first press loads the plugin.
- `:wakes-for` names modes, such as `["R"]`, whose buffers load the plugin as soon as one is open.
- `:settings` declares settings with a `:type` (`:text`, `:integer`, `:number`, `:toggle`, `:list`, `:data` for any value, or a vector of keywords to choose from), a `:default` and a `:doc`. `M-x customize` shows them, `use-plugin :config` and `(settings 'name {...})` set them, and `(setting :name)` reads them anywhere in the plugin.
- `:start-screen` adds commands to the start screen, as `{hello: "Say hello"}`.
- `:modes` declares file modes that wake the plugin, as `[{title: "Notes" extensions: ["note"] comment: "#" roots: ["notebook.toml"]}]`, where `roots` names the files that mark a project of the mode for `language-root` and its language server, and `locate`, a command such as `["cargo" "locate-project" "--workspace" "--message-format" "plain"]`, prints a file in the project when markers alone can't tell. `grammar: "rust"` lends a mode the highlighting, indentation, outline and brackets of a built-in grammar, `rust`, `toml`, `r` or `ox`, so it needs no `defmode`; otherwise the entry defines it with `defmode`, giving their highlighting and indentation, while their extensions, file names, comment, roots and keys stay in the manifest.
- `:startup-when :name` starts the plugin with oxmacs when its setting `name` is true, as the weather plugin does for its mode line, and `:startup-when true` starts it whenever it is on, as tabs does.
- `:opens-directories true` makes the plugin the way directories open, as dired is.
- `:opens-files ["epub"]` makes the plugin the way files with those extensions open, as epub does for books and pdf for documents; its entry registers the opener with `(on-open-file "epub" f)`.
- `:opt-in true` keeps a bundled plugin off until `use-plugin` turns it on.
- `:builds` names build commands, as `{check: "cargo check" test: "cargo test"}`, which become commands named after the plugin, such as `rust-check`, run in the project of the current buffer.
- `:lint` runs a linter when a file of the plugin's modes is saved, as `{program: "cargo" args: [:lint "--message-format=json"] root: :language format: :cargo}`. In `:args`, `:file` is the saved file and any other keyword is the value of the plugin's setting of that name, and `:off` or `nil` there turns linting off; `:root` is `:language` (the default), `:project` or `:folder`, and `:format` names the reader of the output, `:cargo`.
- `:error-patterns` makes locations in build output clickable, as `[#"--> (?P<file>.+):(?P<line>\d+):(?P<column>\d+)"]`: each regex names the groups `file` and `line`, and may name `column`.
- `:tools` lists programs or libraries `M-x list-tools` installs from GitHub releases while the plugin is on, as `[{name: "air" repository: "posit-dev/air" summary: "R formatter" group: "R" tag-prefix: "v" checksum: :sidecar setting: :air-program archives: {linux: {x86_64: "air-x86_64-unknown-linux-gnu.tar.gz"}}}]`. `archives` maps each system (`windows`, `macos`, `linux`) and architecture (`x86_64`, `aarch64`) to its file; `checksum` is `:sidecar`, a `.sha256` file beside each archive, or `:digest`, the digest GitHub publishes; `form: :library` is for a shared library, which is never looked for on PATH; and `setting` names the plugin's setting that points at a copy of your own instead.

`(use-plugin hello :disabled true)` turns a plugin off, and a plugin in your folder with the name of a bundled one replaces it. `(plugin-setting 'weather :units)` reads another plugin's setting, `(oxmacs-home)` gives the folder that holds `init.ox`, and `(plugin-folder 'hello)` gives the plugin a folder of its own for its files, as the tutorial keeps your place in the manual's.

## Modules

The entry file is the module named after the plugin. Other files are `hello/name` modules, imported as `(import (hello name))`. `setting` works in all of them. Keep pure logic, such as parsing and drawing, in those modules and the commands in the entry, so the logic can be tested without the editor.

Another plugin imports `(hello name)` by requiring hello in its manifest, as `:requires {hello: "^0.1"}`. Plugins it requires start before it does, and one that is off, such as an opt-in plugin nobody turned on, lends its modules without starting, so its commands stay off. A library with no commands of its own needs only a manifest, and its entry file can be empty:

```text
(plugin strings
  :version "0.1.0"
  :summary "String helpers")
```

## Test it

`ox check ~/.oxmacs/plugins/hello` reports mistakes such as unknown names and wrong arities before you run anything; in a plugin folder it knows the editor API and the plugin's own modules. `ox test ~/.oxmacs/plugins/hello` runs the `test` blocks of the plugin's modules, which can import each other; a file that needs the editor to load, such as the entry with its `defcommand`s, is skipped, so keep what you test in the other modules. See [the ox command](tools).

## The bundled plugins

oxmacs ships these plugins. Those on by default load the first time you use them; opt-in ones wait for `(use-plugin name)` in `init.ox`.

```table
Plugin       Version  Default  What it does
bellow       0.1.0    opt-in   Make music in Ox: songs that loop while you change them, and a workbench of sliders for their controls
c            0.1.0    opt-in   C with highlighting, indentation and the clangd language server
calendar     0.2.9    opt-in   Calendars in .ics files, with CalDAV sync, reminders and invitations
cpp          0.1.0    opt-in   C++ with highlighting, indentation and the clangd language server
customize    0.1.6    on       Change the editor's and plugins' settings in a form
dired        0.3.4    on       List directories and manage their files
epub         0.2.6    opt-in   Read EPUB books in pages, with contents, search, highlights, notes and a library
explorer     0.2.4    on       A file tree beside the windows that follows the file you're in
git          0.4.5    opt-in   Git status and staging in the style of Magit
go           0.1.0    opt-in   Go with highlighting, indentation with tabs and the gopls language server
irc          0.10.7   opt-in   An IRCv3 client in the style of ERC
javascript   0.1.0    opt-in   JavaScript with highlighting, indentation and typescript-language-server
json         0.1.0    opt-in   JSON and JSON with comments, with highlighting and indentation
keycast      0.2.4    opt-in   Shows the keys you press and the commands they run, for demos
life         0.2.5    opt-in   Conway's Game of Life on a canvas
manual       0.5.92   on       The oxmacs tutorial and the Ox manual
markdown     0.2.10   on       Markdown with highlighting, a rendered preview and the marksman language server
ox-lang      0.1.0    on       Ox, the language oxmacs is extended in, with highlighting, indentation, ox fmt and the REPL
pdf          0.2.5    opt-in   Read PDF documents drawn by PDFium, with search, an outline, links and copying
processes    0.1.6    on       List the processes oxmacs runs, with their usage
profiler     0.1.4    on       Find where the Ox code in oxmacs spends its time
python       0.1.0    opt-in   Python with highlighting, indentation and the basedpyright, pyright or pylsp language server
r            0.21.0   opt-in   R consoles, plots, data and help in the style of ESS and RStudio
remote       0.4.0    on       Browse and edit files on other machines over ssh, sftp and ftp
rodeo        0.1.2    opt-in   Play 2D games written in Ox beside their code, and change them while they run
rust         0.1.0    opt-in   Rust with highlighting, cargo builds and lints, and the rust-analyzer language server
screensaver  0.2.4    opt-in   A starfield or matrix rain after a while without input
tabs         0.1.4    opt-in   Workspace tabs across the top, each keeping its own windows
toml         0.1.0    opt-in   TOML with highlighting, indentation and the taplo language server
typescript   0.1.0    opt-in   TypeScript and TSX with highlighting, indentation and typescript-language-server
vault        0.3.1    opt-in   An encrypted password vault with one-time codes and imports
weather      0.3.4    opt-in   Forecasts from Open-Meteo
wsl          0.1.1    opt-in   WSL distributions as places to work: their files open as your own, their tools run inside them, and *WSL* starts, stops and opens them
zap          0.2.2    opt-in   Flash the code you evaluate or send to a console, in the error color when it fails
zen          0.1.2    opt-in   Distraction-free writing in one centered column
```

## Learn from the bundled plugins

dired, git, weather, irc, calendar, vault, remote, markdown and this manual are all plugins written this way. Their sources are in the oxmacs repository under `crates/oxmacs-script/plugins`.

"""})
