(module (export page))

(def page
  {id: "plugins"
   title: "Writing a plugin"
   text: #r##"# Writing a plugin

A plugin is a folder under `~/.oxmacs/plugins` with a manifest and Ox modules. It costs nothing until it is used: at startup oxmacs reads only the manifest, and the code loads the first time one of its commands, keys or modes is used.

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
  :commands {hello: "Say hello"}
  :settings {name: {type: :string default: "world" doc: "Who to greet"}}
  :entry "hello.ox")
```

3. Add `hello.ox`, the entry:

```text
(defcommand hello "Say hello"
  (message "Hello, {(setting :name)}!"))
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
- `:commands` maps each command to its summary. They exist from startup and load the plugin when first run.
- `:keys` binds global keys, such as `{"C-c h" hello}`; your `init.ox` can rebind them.
- `:mode-keys` binds keys in the buffers of a mode, such as `{"R" {"C-c h" hello}}`, and the first press loads the plugin.
- `:wakes-for` names modes, such as `["R"]`, whose buffers load the plugin as soon as one is open.
- `:settings` declares settings with a `:type` (`:string`, `:int`, `:number`, `:bool`, `:list` or a vector of keywords to choose from), a `:default` and a `:doc`. `M-x customize` shows them, `use-plugin :config` sets them and `(setting :name)` reads them in the entry file.
- `:start-screen` adds commands to the start screen, as `{hello: "Say hello"}`.
- `:modes` declares file modes that wake the plugin, as `[{name: "Notes" extensions: ["note"] comment: "#"}]`; the entry then defines them with `defmode`.
- `:startup-when :name` starts the plugin with oxmacs when its setting `name` is true, as the weather plugin does for its mode line.
- `:opens-directories true` makes the plugin the way directories open, as dired is.
- `:opens-files ["epub"]` makes the plugin the way files with those extensions open, as epub does for books and pdf for documents; its entry registers the opener with `(on-open-file "epub" f)`.
- `:opt-in true` keeps a bundled plugin off until `use-plugin` turns it on.

`(use-plugin hello :disabled true)` turns a plugin off, and a plugin in your folder with the name of a bundled one replaces it. `(plugin-setting 'weather :units)` reads another plugin's setting, and `(oxmacs-home)` gives the folder that holds `init.ox`, where a plugin can keep files of its own, as the tutorial keeps your place.

## Modules

The entry file is the module named after the plugin. Other files are `hello/name` modules, imported as `(import (hello name))`. `setting` works only in the entry, so pass settings to the other modules as arguments. Keep pure logic, such as parsing and drawing, in those modules and the commands in the entry, so the logic can be tested without the editor.

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
calendar     0.2.4    opt-in   Calendars in .ics files, with CalDAV sync, reminders and invitations
customize    0.1.3    on       Change the editor's and plugins' settings in a form
dired        0.2.5    on       List directories and manage their files
epub         0.1.4    opt-in   Read EPUB books in pages, with contents, search, highlights, notes and a library
explorer     0.1.3    on       A file tree beside the windows that follows the file you're in
git          0.3.1    opt-in   Git status and staging in the style of Magit
irc          0.9.7    opt-in   An IRCv3 client in the style of ERC
keycast      0.2.2    opt-in   Shows the keys you press and the commands they run, for demos
languages    0.1.0    on       Python, Go, C, C++, TypeScript, JavaScript, JSON and Markdown modes with their language servers
life         0.2.3    opt-in   Conway's Game of Life on a canvas
manual       0.5.45   on       The oxmacs tutorial and the Ox manual
markdown     0.2.3    opt-in   Markdown rendered as styled text
pdf          0.1.2    opt-in   Read PDF documents drawn by PDFium, with search, an outline, links and copying
processes    0.1.1    on       List the processes oxmacs runs, with their usage
profiler     0.1.2    on       Find where the Ox code in oxmacs spends its time
r            0.18.7   opt-in   R consoles, plots, data and help in the style of ESS and RStudio
remote       0.1.8    on       Browse and edit files on other machines over ssh, sftp and ftp
rodeo        0.1.1    opt-in   Play 2D games written in Ox beside their code, and change them while they run
screensaver  0.2.2    opt-in   A starfield or matrix rain after a while without input
tabs         0.1.1    opt-in   Workspace tabs across the top, each keeping its own windows
vault        0.2.2    opt-in   An encrypted password vault with one-time codes and imports
weather      0.3.2    opt-in   Forecasts from Open-Meteo
zap          0.2.0    opt-in   Flash the code you evaluate or send to a console, in the error color when it fails
zen          0.1.0    opt-in   Distraction-free writing in one centered column
```

## Learn from the bundled plugins

dired, git, weather, irc, calendar, vault, remote, markdown and this manual are all plugins written this way. Their sources are in the oxmacs repository under `crates/oxmacs-script/plugins`.
"##})
