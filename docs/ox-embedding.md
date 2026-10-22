# Ox in the editor: phases 2 and 3

How oxmacs hosts Ox, and the slices that take the weather plugin from nothing to a working port. The language itself is in [ox-design.md](ox-design.md) and [ox-internals.md](ox-internals.md).

## Decisions

- **The bridge.** A new crate, `oxmacs-ox`, depends on `ox` and `oxmacs-core`, and `oxmacs-gui` hosts it. The GUI owns the editor and the bridge side by side, so it can lend the editor to Ox for each call. Ox stays out of core, whose state must be `Send`.
- **Reaching the editor.** Natives get the editor during each call, so `(insert "x")` and `(buffer-text)` act immediately, as in Emacs Lisp. `ox` passes an opaque host (`&mut dyn Any`) through every call, and `define_host_native::<Editor>` downcasts it. Ox runs on the UI thread in time-boxed slices, so this never blocks a frame for long.
- **Commands.** An Ox command is an editor dynamic command whose handler queues a request; the GUI hands the request to the bridge, which runs the Ox function with the editor lent to it.
- **Asynchronous work.** Natives that start work outside the isolate (a fetch, a timer) hold their promise with `heap.hold`, which gives a plain id that can travel in a `Send` callback, and resolve it when the result comes back.
- **Configuration.** `~/.oxmacs/init.ox` (moved with `OXMACS_HOME`) is the only place plugins are configured: `(use-plugin weather :config {location: "Berlin" units: :metric mode-line: true})`. A plugin declares its settings with `defsetting` and reads them with `(setting :location)`; unknown keys and wrong types are reported.
- **Bundled plugins.** Their files are embedded in the binary at build time. A plugin of the same name in `~/.oxmacs/plugins` overrides the bundled one, which is how you edit one live.
- **Lazy wake-ups.** At startup only the `plugin.ox` manifests are read. A plugin loads on its first command or key, or at startup when a config key its manifest names is on (`:startup-when :mode-line`).
- **Mode line.** A plugin pushes its text with `(set-mode-line :weather "☀ 21°")`; drawing never runs Ox.
- **Views.** A plugin buffer shows a view: Ox functions such as `column`, `row`, `text` and `button` build a widget tree that the GUI renders with Iced. A view reads atoms, and changing an atom it read (`(reset! forecast …)` from a spawned task, say) redraws it. Buttons call Ox functions directly.

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
