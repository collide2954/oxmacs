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

## Wiring it in

Three lines outside the crate add an applet to oxmacs:

1. `Cargo.toml` at the workspace root: `oxmacs-demo = { path = "crates/oxmacs-demo" }`.
2. `crates/oxmacs/Cargo.toml`: an optional dependency and a feature, `demo = ["dep:oxmacs-demo"]`.
3. `crates/oxmacs/src/applets.rs`: `#[cfg(feature = "demo")] Box::new(oxmacs_demo::Demo)`.

## Testing

Applets are tested like the rest of oxmacs: create an `Editor`, `install` the applet, press keys with `handle_key`, and check the buffer, the minibuffer, the effects it requests (`take_effects`) and what happens when an event is delivered (`deliver`). The tests at the bottom of `src/lib.rs` do exactly that.
