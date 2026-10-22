# Ox in the editor: phases 2 and 3

How oxmacs hosts Ox, and the slices that take the weather plugin from nothing to a working port. The language itself is in [ox-design.md](ox-design.md) and [ox-internals.md](ox-internals.md).

## Decisions

- **The bridge.** A new crate, `oxmacs-ox`, depends on `ox` and `oxmacs-core`, and `oxmacs-gui` hosts it. The GUI owns the editor and the bridge side by side, so it can lend the editor to Ox for each call. Ox stays out of core, whose state must be `Send`.
- **Reaching the editor.** Natives get the editor during each call, so `(insert "x")` and `(buffer-text)` act immediately, as in Emacs Lisp. `ox` passes an opaque host (`&mut dyn Any`) through every call, and `define_host_native::<Editor>` downcasts it. Ox runs on the UI thread in time-boxed slices, so this never blocks a frame for long.
- **Commands.** An Ox command is an editor dynamic command whose handler queues its name in bridge state kept in the editor; the next `Ox::tick` runs the Ox function with the editor lent to it.
- **Asynchronous work.** `ox` natives that block, such as `http/get`, run on a thread with `heap.background`, which settles their promise when a tick polls it. Host natives that start editor work hold their promise with `heap.hold`, a plain id that can travel in a `Send` callback, and resolve it when the result comes back.
- **Configuration.** `~/.oxmacs/init.ox` (moved with `OXMACS_HOME`) is the only place plugins are configured: `(use-plugin weather :config {location: "Berlin" units: :metric mode-line: true})`, or `:disabled true` to turn one off. A plugin declares its settings in `plugin.ox` and reads them with `(setting :location)`; unknown keys and wrong types are logged and the defaults stay.
- **Bundled plugins.** Their files are embedded in the binary at build time, and they are on unless `use-plugin` turns them off. Other plugins in `~/.oxmacs/plugins` stay off until `use-plugin` names them, except that one with a bundled plugin's name replaces it, which is how you edit one live.
- **Lazy wake-ups.** At startup only the `plugin.ox` manifests are read. A plugin loads on its first command or key, or at startup when a config key its manifest names is on (`:startup-when :mode-line`).
- **Errors.** Every Ox error, with its traceback and the time, is added to `*ox log*`, and the echo area gets one line. An error in `init.ox` opens the log beside the editor; the forms before it still apply.
- **Mode line.** A plugin pushes its text with `(set-mode-line :weather "☀ 21°")`; drawing never runs Ox.
- **Views.** `(open-view "*Weather*" view-fn :keys {g: 'weather-refresh q: 'quit-window})` opens a buffer drawn from `view-fn`. The widgets come from `(ox ui)`: `column` and `row` (with `:spacing`), `text` (with `:face` and `:size`), `button` (with `:on-press`), `space` and `scrollable`. They are Rust nodes that Ox holds, and the GUI draws them with Iced in the editor's font, so padded text lines up. After any atom changes, open views are rendered again and the screen updates only when the tree differs; tracking which atoms a view read can come later. Buttons call their Ox function directly.

## Plugins

A plugin is a folder with a manifest and its modules:

```clojure
; plugin.ox
(plugin weather
  :version "0.2.0"
  :summary "Forecasts from Open-Meteo"
  :commands {weather: "Show the weather for your location"
             weather-refresh: "Fetch the forecast again"}
  :settings {location: {type: :string doc: "City, such as Berlin"}
             units: {type: [:metric :imperial] default: :metric}
             mode-line: {type: :bool default: false}}
  :startup-when :mode-line
  :entry "weather.ox")
```

The manifest is read as quoted data. Its commands exist from startup and wake the plugin when first run. The entry file loads as the module `weather`, and the plugin's other files as `weather/<file>`, so `(import (weather forecast))` works. Each plugin module gets a `setting` function for its own settings.

## Weather

The first bundled plugin lives in `crates/oxmacs-ox/plugins/weather`: `plugin.ox`, the entry `weather.ox` (state in a `defonce` atom, fetching in a spawned task, commands, the mode line and its refresh), `forecast.ox` (URLs, parsing Open-Meteo JSON and formatting) and `page.ox` (the view, a pure function of the state). Its tests feed recorded Open-Meteo replies to the modules and read the drawn view as lines.

## Slices

Each slice is a tested, gated commit; about three go in each cycle.

1. **Pass a host through every call,** with `define_host_native`, `eval_with`, `call_with` and `run_with`, and hold values by id.
2. **Host a UI isolate in the GUI** (`oxmacs-ox`): `message`, and a per-frame scheduler tick while tasks are pending.
3. **Define commands and keys in Ox:** `defcommand` and `bind-key`.
4. **Load `init.ox` at startup,** reporting errors with their tracebacks in `*ox log*`.
5. **Load plugins from manifests,** bundled or from `~/.oxmacs/plugins`, with `use-plugin`, `:config` and lazy wake-ups.
6. **Declare and read settings, and set mode-line text.**
7. **Sleep and repeat on timers:** `sleep`, `every` and `now`.
8. **Fetch over HTTP and read and write JSON.**
9. **Format numbers, strings and times** as the weather view needs.
10. **Build views and render them in a plugin buffer.**
11. **Redraw views when the atoms they read change,** and run Ox from buttons and buffer keys.
12. **Port weather:** `M-x weather`, refresh, the mode line and its 30-minute refresh, tested against recorded Open-Meteo replies.
13. **Work on Ox in the editor:** an `.ox` mode, `M-x ox-repl`, `C-x C-e`, eval-buffer and reloading a plugin when its file is saved.
