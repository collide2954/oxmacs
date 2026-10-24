(module (export page))

(def page
  {id: "editor"
   title: "Extending oxmacs"
   text: #r#"# Extending oxmacs

Everything below works in `init.ox`, in a plugin, or straight from the REPL. The [Editor API](ref:oxmacs) lists every function, and [Buffers and views](ui) covers what a plugin can show.

## Commands and keys

`defcommand` adds an `M-x` command. A command runs as a task, so it can wait for a prompt or a download without holding up typing. Defining a command again replaces it.

```
(defcommand say-hello "Say hello in the echo area"
  (message "Hello from Ox"))
(bind-key "C-c h" 'say-hello)
```

`bind-key` binds keys everywhere; modes and plugin buffers have keys of their own. `run-command` runs any command by name, as `M-x` does. An error a command doesn't catch shows in the echo area as `Ox: …` and is kept in `*ox log*`.

## Asking

`prompt` asks in the minibuffer and returns a promise of the answer. `:choices` offers completion, `:initial` fills in text and `:secret true` hides what is typed. `yes-or-no` asks a question.

```
(defcommand greet "Greet someone"
  (let [name (await (prompt "Name: " :choices ["Ana" "Bo"]))]
    (when (await (yes-or-no "Greet {name}?"))
      (message "Hello, {name}"))))
```

`prompt-path` asks for a file or a folder, with completion. `C-g` cancels a prompt by failing its promise, which ends the command unless it catches the error.

## The current buffer

```
[(buffer-name) (current-line) (current-column)]
```

Lines and columns count from 0. `buffer-text` and `set-buffer-text` read and replace the text, `line-text` reads one line, `region-lines` gives the first and last lines of the selection, and `goto-line` moves point. `open-file` visits a file, `buffer-file` gives the file a buffer visits and `switch-to-buffer` shows a buffer.

## Reacting

Hooks call a function, as a task, when something happens. Registering the same key again replaces the earlier function.

```text
(on-saved :log-saves (fn [path] (message "Saved {path}")))
(on-idle :rest 300 (fn [] (message "Time for a break")))
```

`on-disk-change` and `on-settings-change` take a key and a function the same way, and `on-processes-change` follows the processes oxmacs runs, which `processes` lists. `(on-open-directory f)` makes `f` the way directories open, with no key; a plugin that does this declares `:opens-directories true`, as dired does. `every` repeats work on a timer.

## The mode line and notifications

```
(set-mode-line :status "Ready")
(set-mode-line :status nil)
```

`notify` shows a desktop notification, and `clipboard-set` copies text without touching the kill ring, for secrets.

## Settings

`(settings {theme: :adwaita-dark})` changes editor settings at once, `nil` putting one back to its default, and `(settings)` reads them all as a map. `settings-schema` describes each one, as `M-x customize` lists them.

```
(get (settings) :tab-width)
```

Settings made this way last until oxmacs quits; keep them in `init.ox`. Plugins declare settings of their own, read with `setting`; see [Writing a plugin](plugins).
"#})
