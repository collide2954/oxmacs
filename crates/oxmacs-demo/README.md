# oxmacs-demo

An example applet. It does little on its own; it exists to show, in one small file, how an applet plugs into oxmacs. Copy it to start your own.

Build oxmacs with it and try it with `M-x demo`:

```sh
cargo run -p oxmacs --release --features demo
```

## What an applet is

An applet is an ordinary Rust crate. It depends on `oxmacs-core` (and on `oxmacs-gui` if it draws or performs work in the GUI), implements two traits, and is compiled into the editor when its feature is enabled. There is no plugin loading: if the feature is off, none of its code is in the binary.

## The pieces, in `src/lib.rs`

**The applet itself.** `struct Demo` implements `oxmacs_core::applet::Applet`:

- `name` and `version` identify it. Optional applets carry their own version, taken from their crate's `Cargo.toml`; `M-x oxmacs-doctor` lists them.
- `opt_in` says it is not built by default.
- `install` runs once at startup and registers everything below.

**State.** `editor.set_applet_state(List { .. })` stores the applet's own data in the editor, keyed by its type. Commands read and change it with `applet_state` and `applet_state_mut`.

**A scope and commands.** `editor.add_scope("*Demo*")` creates a keymap of its own and the label `C-h f` uses for it. `editor.add_command(CommandSpec { .. })` registers a command with its name, summary, optional help, scope and handler. `demo` is global, so it shows in `M-x` everywhere; `demo-add`, `demo-delete` and `demo-roll` belong to the demo scope, so they only appear there.

**Keys.** `editor.bind(scope, "a", command)` binds keys in that scope, using the same notation as the rest of oxmacs (`C-c n`, `M-x`, `<up>`). Core commands can be bound too: `q` runs `quit-window`.

**A buffer.** `View` implements `AppletBuffer`: it names the scope whose keys apply in it and returns each line's colors. `editor.open_applet_buffer` shows it, `find_applet_buffer` and `applet_buffer_mut` find it again, `show_buffer` brings it back, and `fill_buffer` replaces its text while keeping point. `Page` builds styled text the same way core's special buffers do.

**Prompts and questions.** `editor.prompt_with("Add item: ", offer, done)` opens the minibuffer with completions the applet offers; `done` receives the choice. `editor.prompt` asks for free text, and `editor.ask("Delete …?", then)` asks y or n.

**Effects and events.** Commands never block. `editor.request(Roll)` asks the GUI to do work; `GuiApplet::perform` in the same crate recognizes `Roll`, does the work in an iced `Task`, and answers with an event, `Rolled(4)`. `editor.on_event(rolled)` routes that typed event back to the applet.

**Drawing in the GUI.** `GuiApplet` can also add widgets around a window. `above` draws the die over the demo buffer and `beside` draws the tally to its right; both check `pane.applet_buffer::<View>()` so they only appear in the demo's own windows. Anything they emit is a typed event that comes back through `on_event`. `animating` asks for animation frames while a roll is pending, and `subscription` and `tick` are there for applets that need timers, streams or GUI-side state.

**Configuration.** An applet keeps its settings in a table of its own in config.toml. The demo reads `[demo] sides = 20` to choose its die. `editor.add_config_table::<Settings>("demo")` registers a serde struct for the table, and `editor.applet_config::<Settings>("demo")` reads it, falling back to `Settings::default()`. Unknown keys or bad values, such as fewer than two sides, show in the echo area without throwing away the rest of the config. A table that no compiled-in applet reads is kept as it is and pointed out by `M-x oxmacs-doctor`, so one config.toml works across builds. `editor.add_settings("demo", SETTINGS)` lists the table's keys in `M-x customize` under their own heading, where they are changed, checked and saved like any other setting.

**The doctor.** `editor.add_doctor_section("Demo", report)` adds a section to `M-x oxmacs-doctor`. `report` returns `DoctorLine`s, each a label, a finding (`Info`, `Good`, `Warning` or `Problem`) and some text.

## Beyond the demo

The demo keeps to the basics. The other built-in applets use more of the same API:

- **Languages.** `editor.add_language(spec, scope)` adds a file type with its highlighting, indentation, comments and outline. Markdown is built this way.
- **Files and folders.** `add_file_opener(&["pdf"], open)` claims a file type, as the PDF viewer does, and `on_open_directory` takes over opening folders, as dired does. An applet buffer can name a `file()` to have it watched for changes, a `directory()` to stand for a folder, and a `position()` to show in the mode line instead of line and column.
- **More prompts.** `prompt_pick` offers choices and also accepts typed text, `prompt_path` asks for a file or folder like `C-x C-f`, `prompt_styled` highlights what is typed, `ask_typed` wants a typed yes or no, and `ask_choice` takes one of several letters.
- **Hooks.** `on_command`, `on_save`, `on_saved`, `on_disk_change`, `on_configured` and `on_quit` run after the matching editor event, `on_display` runs before the editor is drawn, and `fetch` downloads a page and hands back its text.
- **Whole windows.** `GuiApplet::body` draws a buffer's entire window, as the Markdown preview and the PDF viewer do, and `minibuffer_style` restyles the minibuffer while a prompt waits, as the glow applet does.
- **Tools.** `add_managed_tool` lists a program or library in `M-x list-tools`, which downloads it from its GitHub releases and checks it against the published checksum; `installed_tool` finds it again. The PDF viewer gets PDFium this way.
- **The start screen and the mode line.** `set_splash_entry` adds a line to the start screen and `add_mode_line_segment` adds text before the clock.

## Wiring it in
Three lines outside the crate add an applet to oxmacs:

1. `Cargo.toml` at the workspace root: `oxmacs-demo = { path = "crates/oxmacs-demo" }`.
2. `crates/oxmacs/Cargo.toml`: an optional dependency and a feature, `demo = ["dep:oxmacs-demo"]`.
3. `crates/oxmacs/src/applets.rs`: `#[cfg(feature = "demo")] Box::new(oxmacs_demo::Demo)`.

## Testing

Applets are tested like the rest of oxmacs: create an `Editor`, `install` the applet, press keys with `handle_key`, and check the buffer, the minibuffer, the effects it requests (`take_effects`) and what happens when an event is delivered (`deliver`). The tests at the bottom of `src/lib.rs` do exactly that.
