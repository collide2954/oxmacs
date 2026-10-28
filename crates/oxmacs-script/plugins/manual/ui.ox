(module (export page))

(def page
  {id: "ui"
   title: "Buffers and views"
   text: #r##"# Buffers and views

A plugin shows its work in buffers of text lines, in views of widgets, or in menus, and a mode gives files of a kind their own keys and colors.

## Buffers of your own

`open-buffer` shows lines in a read-only buffer with its own keys, making it the first time and filling it again after that. A line is text, or a vector of pieces where `[text face]` colors a piece with a theme face.

```
(defcommand fruit-next "Move to the next fruit"
  (goto-line (+ (current-line) 1)))
(open-buffer "*fruit*"
             [[["Fruit" :heading]]
              ["  apple   " ["ripe" :success]]
              ["  quince  " ["hard" :warning]]]
             :keys {"n" 'fruit-next "q" 'quit-window})
```

The faces are `:heading`, `:link`, `:comment`, `:doc-comment`, `:keyword`, `:string`, `:function`, `:type`, `:constant`, `:attribute`, `:macro`, `:success`, `:warning`, `:error`, `:added`, `:removed`, `:marked`, `:flagged`, `:region` and `:symlink`, and `:nick-0` to `:nick-7` for names in a chat. `[text face :pill]` draws a piece as a rounded label, and `[text face :link target]` makes it a link.

A line written as a map, such as `{line: pieces background: :region}`, gets a background, and fields of your own in it make the line an item. `line-item` gives the map of the line at point, `goto-item` moves to the line whose item has the fields it is given, and `next-item` steps from item to item, so commands find their rows without counting lines:

```
(defcommand fruit-show "Say which fruit this is"
  (message (or (get (line-item) :name) "No fruit here")))
(let [buffer (open-buffer "*fruit*"
                          [{line: "  apple" name: "apple"} {line: "  quince" name: "quince"}]
                          :keys {"RET" 'fruit-show "q" 'quit-window})]
  (set-header buffer [[["Fruit" :heading]]])
  (goto-item {name: "quince"}))
```

`set-header` keeps headings above the lines, where they stay while the lines scroll and never shift them. `set-lines` fills the buffer again and keeps point where it was, and `append-lines` adds lines at the end, as a log does. `open-buffer` also takes:

- `:keys`, a map of keys to commands. The commands run with this buffer current, so `current-line` tells them where point is.
- `:on-move`, called with the line when point moves to another line, and `:on-resize`, called with the width when the window changes width.
- `:on-link`, called with the target of a link when it is clicked or `follow-link` runs on it; bind `follow-link`, `next-link` and `previous-link` in `:keys`, usually to `RET`, `TAB` and `S-TAB`.
- `:line` to move point, `:beside true` to show the buffer in another window, and `:select true` to move there too.
- `:input "> "` for an input line below the lines, as in a chat. `take-input` reads and empties it, and `M-p` and `M-n` recall earlier input.
- `:directory`, the folder that file prompts start in, and `:show false` to make or fill the buffer without showing it.

## Views

`open-view` draws widgets instead of text. It calls its function to build them, and builds them again whenever an atom changes, so keep what a view shows in atoms. The widgets come from the `ox ui` module.

```
(module (import (ox ui)))
(def clicks (atom 0))
(open-view "*clicks*"
           (fn []
             (ui/column
              (ui/text "Clicked {@clicks} times" :face :heading)
              (ui/button "Click" :on-press (fn [] (swap! clicks inc))))))
```

- `ui/column` and `ui/row` stack their children `:spacing` apart. Strings among them become text, `nil` is left out and vectors are spread out.
- `ui/text` shows a value in a `:face`, at an optional `:size`.
- `ui/button` calls `:on-press` as a task when it is pressed.
- `ui/space` adds room, `ui/scrollable` lets its child scroll and `ui/image` shows an `ox/image` image.

`open-view` takes `:keys`, `:beside` and `:select` like `open-buffer`. The function must return quickly, since it is stopped after 250 ms: fetch data in a task and keep it in an atom instead.

## State for each buffer

`set-buffer-value!` keeps a value with the current buffer under a keyword, `buffer-value` reads it back, and it goes away when the buffer closes. `open-view` and `open-buffer` return the buffer, so a plugin that opens many keeps each one's state in an atom stored this way, and its commands find the state of the buffer they run in:

```
(module (import (ox ui)))
(defcommand counter-bump "Count once more"
  (swap! (buffer-value :counter) inc))
(let [counted (atom 0)]
  (with-buffer (open-view "*counter*" (fn [] (ui/text "Counted {@counted}"))
                          :keys {"SPC" 'counter-bump})
    (set-buffer-value! :counter counted)))
```

A buffer that shows a file of its own, as the PDF reader's do, takes `:file`: it is named after the file as file buffers are, opening the file again comes back to it, and the file's changes on disk reach `on-disk-change`.

## Drawing

`ui/canvas` draws a vector of shapes, `:height` tall. A color is a face, `:foreground`, `:background`, a `"#rrggbb"` string, or `[color alpha]`.

```
(module (import (ox ui)))
(open-view "*chart*"
           (fn []
             (ui/canvas [[:rect 10 10 60 30 :keyword]
                         [:circle 120 25 15 ["#e06c75" 0.8]]
                         [:line 10 60 180 60 :comment 2]
                         [:text 10 80 "Sales" :foreground 14]]
                        :height 100)))
```

The shapes are `[:rect x y width height color]`, `[:circle x y radius color]`, `[:line x1 y1 x2 y2 color width]` and `[:text x y text color size]`, the last number of a line or text being optional. With `:relative true`, x positions and widths are fractions of the canvas's width, so the drawing fills whatever width it gets. `take-over` draws shapes over the whole window, frame by frame, until a key or a click, as the screensaver does.

## Modes

`defmode` adds a major mode for files by extension or name, with a comment marker, keys of its own and highlighting. `:highlight` is called for each line with its number and text, and returns `[start end face]` spans, counting characters. A highlighter that takes a third argument also gets what it returned as `:state` for the line before, starting from `nil`, when it returns `{spans: [...] state: x}`, so strings and comments can run across lines; after an edit, only the lines from the first changed one are highlighted again. `:indent {width: 4 after: #"[:{]\s*$" dedent: #"^\s*}"}` indents `RET` and `TAB` one width past an opening line and back on a closing one, with tabs when it adds `tabs: true`; without it they keep the indentation of the line above. A file's `.editorconfig` has the last word on tabs, indent size and tab width.

```
(module (import (ox string)))
(defmode notes "Notes"
  :extensions ["note"]
  :comment "#"
  :highlight (fn [_line text]
               (if (string/starts-with? text "#")
                 [[0 (string/length text) :heading]]
                 [])))
```

The highlighter runs again whenever the buffer changes and is stopped after 250 ms, so keep it simple. A plugin also names its modes in its manifest, so opening such a file starts it; see [Writing a plugin](plugins).

## Menus

`transient` opens a menu of switches and actions in the style of Magit. `-` and a switch's key turn it on or off, and an action's key calls its function with the flags of the switches that are on.

```
(transient "Deploy"
  [{title: "Options" switches: [["v" "--verbose" "Say more"]]}
   {title: "Run" actions: [["d" "Deploy" (fn [flags] (message "Deploying with {flags}"))]]}])
```
"##})
