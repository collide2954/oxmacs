(module (export page))

(def page
  {id: "editor"
   title: "Extending oxmacs"
   text: #r#"# Extending oxmacs

Everything below works in `init.ox`, in a plugin, or straight from the REPL. The [Editor API](ref:oxmacs) lists every function.

## Commands and keys

`defcommand` adds an `M-x` command. A command runs as a task, so it can wait for a prompt or a download without holding up typing.

```
(defcommand say-hello "Say hello in the echo area"
  (message "Hello from Ox"))
(bind-key "C-c h" 'say-hello)
```

`run-command` runs any command by name, as `M-x` does.

## Asking

`prompt` asks in the minibuffer and returns a promise of the answer. `:choices` offers completion, and `yes-or-no` asks a question.

```
(defcommand greet "Greet someone"
  (let [name (await (prompt "Name: " :choices ["Ana" "Bo"]))]
    (when (await (yes-or-no "Greet {name}?"))
      (message "Hello, {name}"))))
```

`prompt-path` asks for a file or a folder, with completion.

## The current buffer

```
[(buffer-name) (current-line) (current-column)]
```

`buffer-text` and `set-buffer-text` read and replace the text, `goto-line` moves point, `open-file` visits a file and `switch-to-buffer` shows a buffer.

## Buffers of your own

`open-buffer` shows lines in a read-only buffer with its own keys. A line is text, or a vector of pieces where `[text face]` colors a piece with a theme face such as `:heading`, `:link`, `:comment`, `:keyword`, `:string`, `:success` or `:error`.

```
(defcommand fruit-next "Move to the next fruit"
  (goto-line (+ (current-line) 1)))
(open-buffer "*fruit*"
             [[["Fruit" :heading]]
              ["  apple   " ["ripe" :success]]
              ["  quince  " ["hard" :warning]]]
             :keys {"n" 'fruit-next "q" 'quit-window})
```

`set-lines` redraws it and keeps point where it was. `:on-move` and `:on-resize` call back when point moves to another line or the window changes width, and `:input` adds an input line, as in a chat.

## Views

`open-view` draws widgets instead of text, and draws again whenever the atoms it reads change.

```
(def clicks (atom 0))
(open-view "*clicks*"
           (fn []
             (ui/column
              (ui/text "Clicked {@clicks} times" :face :heading)
              (ui/button "Click" :on-press (fn [] (swap! clicks inc))))))
```

`ui/canvas` draws rectangles, circles, lines and text, for charts and games.

## Reacting

Hooks run a function when something happens. Registering the same key again replaces the earlier function.

```text
(on-saved :log-saves (fn [path] (message "Saved {path}")))
(on-idle :rest 300 (fn [] (message "Time for a break")))
```

`on-disk-change`, `on-settings-change` and `on-open-directory` work the same way. `every` repeats work on a timer.

## The mode line and notifications

```
(set-mode-line :status "Ready")
(set-mode-line :status nil)
```

`notify` shows a desktop notification, and `clipboard-set` copies text without touching the kill ring.

## Settings and modes

`(settings {theme: :adwaita-dark})` changes editor settings, and `(settings)` reads them all. `defmode` adds a major mode for files by extension or name, with its own keys and highlighting.
"#})
