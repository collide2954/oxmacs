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

The faces are `:heading`, `:link`, `:comment`, `:doc-comment`, `:keyword`, `:string`, `:function`, `:type`, `:constant`, `:attribute`, `:macro`, `:success`, `:warning`, `:error`, `:added`, `:removed`, `:marked`, `:flagged`, `:region` and `:symlink`, and `:nick-0` to `:nick-7` for names in a chat. `[text face :pill]` draws a piece as a rounded label, and a line written as `{line: pieces background: :region}` gets a background.

`set-lines` fills the buffer again and keeps point where it was, and `append-lines` adds lines at the end, as a log does. `open-buffer` also takes:

- `:keys`, a map of keys to commands. The commands run with this buffer current, so `current-line` tells them where point is.
- `:on-move`, called with the line when point moves to another line, and `:on-resize`, called with the width when the window changes width.
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

`defmode` adds a major mode for files by extension or name, with a comment marker, keys of its own and highlighting. `:highlight` is called for each line with its number and text, and returns `[start end face]` spans, counting characters.

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
