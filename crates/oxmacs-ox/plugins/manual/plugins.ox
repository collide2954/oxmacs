(module (export page))

(def page
  {id: "plugins"
   title: "Writing a plugin"
   text: #r##"# Writing a plugin

A plugin is a folder under `~/.oxmacs/plugins` with a manifest and Ox modules. It costs nothing until it is used: at startup oxmacs reads only the manifest, and the code loads the first time one of its commands, keys or modes is used.

## Plugins are the standard library

Ox has no standard library of its own. The built-in functions and the `ox/...` modules, such as `ox/string` and `ox/json`, are primitives: Rust code plugged straight into Ox through the bridge, as the editor API is. Everything else is a plugin.

So plugins do two jobs. They extend the editor, and they are where shared Ox code lives. Some bundled plugins, such as dired and git, are on from the start; others, such as life, wait until `use-plugin` turns them on. A plugin can import another plugin's modules, as this manual does with `(import (markdown render))`, so a plugin you write can be a library as well as a feature. See [Modules and libraries](modules).

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
- `:entry` names the entry file, the plugin's name with `.ox` unless given.
- `:commands` maps each command to its summary. They exist from startup and load the plugin when first run.
- `:keys` binds global keys, such as `{"C-c h" hello}`; your `init.ox` can rebind them.
- `:settings` declares settings with a `:type` (`:string`, `:int`, `:number`, `:bool`, `:list` or a vector of keywords to choose from), a `:default` and a `:doc`. `M-x customize` shows them, `use-plugin :config` sets them and `(setting :name)` reads them in the entry file.
- `:start-screen` adds commands to the start screen, as `{hello: "Say hello"}`.
- `:modes` declares file modes that wake the plugin, as `[{name: "Notes" extensions: ["note"] comment: "#"}]`; the entry then defines them with `defmode`.
- `:startup-when :name` starts the plugin with oxmacs when its setting `name` is true, as the weather plugin does for its mode line.
- `:opens-directories true` makes the plugin the way directories open, as dired is.
- `:opt-in true` keeps a bundled plugin off until `use-plugin` turns it on.

`(use-plugin hello :disabled true)` turns a plugin off, and a plugin in your folder with the name of a bundled one replaces it. `(plugin-setting 'weather :units)` reads another plugin's setting, and `(oxmacs-home)` gives the folder that holds `init.ox`, where a plugin can keep files of its own, as the tutorial keeps your place.

## Modules

The entry file is the module named after the plugin. Other files are `hello/name` modules, imported as `(import (hello name))`. `setting` works only in the entry, so pass settings to the other modules as arguments. Keep pure logic, such as parsing and drawing, in those modules and the commands in the entry, so the logic can be tested without the editor.

Another plugin can import `(hello name)` once hello has started. A library with no commands of its own can start with oxmacs through a setting that is on by default, and its entry file can be empty:

```text
(plugin strings
  :version "0.1.0"
  :settings {load: {type: :bool default: true doc: "Load at startup"}}
  :startup-when :load)
```

## Test it

`ox check ~/.oxmacs/plugins/hello` reports mistakes such as unknown names and wrong arities before you run anything; in a plugin folder it knows the editor API and the plugin's own modules. `ox test` runs the `test` blocks of modules that need neither the editor nor another module of the plugin; give it those files, as in `ox test ~/.oxmacs/plugins/hello/words.ox`. See [the ox command](tools).

## Learn from the bundled plugins

dired, git, weather, irc, calendar, vault, remote, markdown and this manual are all plugins written this way. Their sources are in the oxmacs repository under `crates/oxmacs-ox/plugins`.
"##})
