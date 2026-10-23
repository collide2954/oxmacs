(module (export page))

(def page
  {id: "plugins"
   title: "Writing a plugin"
   text: #r#"# Writing a plugin

A plugin is a folder under `~/.oxmacs/plugins` with a manifest and Ox modules. It costs nothing until it is used: at startup oxmacs reads only the manifest.

## Make one in four steps

1. Create the folder `~/.oxmacs/plugins/hello`.

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

4. Press `C-c C-k` in `hello.ox`. The plugin starts, and `M-x hello` works. To have it on in every session, add this to `init.ox`:

```text
(use-plugin hello :config {name: "Ana"})
```

## Develop it live

Saving a loaded plugin's file evaluates it again in place, so a change takes effect at once. Values made with `defonce` keep their state across reloads; use it for atoms. `C-x C-e` evaluates a form inside the plugin's own module, and errors open `*ox stacktrace*`.

## Manifests

- `:commands` maps each command to its summary. They exist from startup and load the plugin when first run.
- `:keys` binds global keys, such as `{"C-c h" hello}`; your `init.ox` can rebind them.
- `:settings` declares settings with a `:type` (`:string`, `:int`, `:number`, `:bool`, `:list` or a vector of choices), a `:default` and a `:doc`. `M-x customize` shows them, `use-plugin :config` sets them and `(setting :name)` reads them in the entry file.
- `:start-screen` adds commands to the start screen, and `:modes` names file modes that wake the plugin.
- `:opt-in true` keeps a bundled plugin off until `use-plugin` turns it on.
- `:version` follows semantic versioning.

## Modules

The entry file is the module named after the plugin. Other files are `hello/name` modules, imported as `(import (hello name))`. Keep pure logic, such as parsing and drawing, in those modules and the commands in the entry, so the logic can be tested without the editor.

## Test it

`test` blocks run with `ox test ~/.oxmacs/plugins/hello`, and `ox check` reports mistakes such as unknown names and wrong arities before you run anything. See [the ox command](tools).

## Learn from the bundled plugins

dired, git, weather, irc, calendar, vault, remote, markdown and this manual are all plugins written this way. Their sources are in the oxmacs repository under `crates/oxmacs-ox/plugins`.
"#})
