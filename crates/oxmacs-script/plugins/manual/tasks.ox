(module (export page))

(def page
  {id: "tasks"
   title: "Tasks, time and I/O"
   text: #r#"# Tasks, time and I/O

Ox code runs in small slices between keystrokes, so it never freezes the editor. Anything slow, such as reading a file or a web page, returns a promise, and `await` waits for it without blocking anything else.

## Tasks and promises

`spawn` runs code as a task and returns a promise of its result. `await` waits for a promise anywhere, with nothing to mark, and gives back any other value as it is.

```
(def total (spawn (reduce + (range 1000))))
(await total)
```

`promise` and `deliver` make a promise by hand, for results that arrive from elsewhere; `fail` makes whatever awaits it raise an error instead.

```
(let [answer (promise)]
  (spawn (deliver answer 42))
  (await answer))
```

An error in a task is raised again in whatever awaits its promise; when nothing does, oxmacs shows it in the echo area. Commands, hooks and buttons each run as tasks of their own. At the top level, as in the REPL, awaiting a promise that nothing can deliver raises an error instead of hanging.

`swap!` is safe when tasks change the same atom. For anything longer, `atomically` runs its body without letting other tasks in.

## Time

`sleep` pauses a task for some milliseconds. `every` calls a function on a timer, in a task of its own, until it returns `:stop`.

```
(do (sleep 10) :awake)
```

```text
(every (time/minutes 5) (fn [] (message "Stretch")))
```

The `ox time` module reads, formats and adds times in any time zone; see its [reference](ref:ox/time).

```
(module (import (ox time)))
(time/format (time/instant "2026-10-23T09:30Z") "%A %e %B, %H:%M" :zone "UTC")
```

## Files, processes and the web

These modules return promises, so their results need `await`. Paths are used as they are given, so expand a leading `~` with `path/expand`.

```text
(module (import (ox file) (ox path) (ox process) (ox http) (ox json)))

(def notes (await (file/read (path/expand "~/notes.txt"))))
(def listing (await (process/run "git" ["status" "--short"])))
(def reply (await (http/get "https://example.com/data.json")))
(json/read reply.body)
```

`process/run` gives `{status stdout stderr}`, and `http/get` gives `{status headers body}`. A file that can't be read or a program that can't start raises an error where it is awaited.

`process/start` runs a program alongside your code instead. Its `:lines` channel gives each line the program prints, as it comes, and `:exit` is a promise of its exit status. `process/stop` asks it to end and `process/kill` ends it at once, with any programs it started. In oxmacs it shows in `M-x list-processes`, where its `:id` names it for `stop-process` and `kill-process`.

```text
(let [git (process/start "git" ["log" "--oneline"])]
  (spawn ((fn show [] (when-let [line (receive git.lines)] (message line) (show)))))
  (await git.exit))
```

`net/connect` gives a promise of a TCP connection whose `:lines` channel works the same way, and `net/send` writes a line to it. `ox/csv` reads CSV, and `ox/crypto` hashes, encrypts and makes one-time codes.

## Generators

A generator yields values one at a time, running only as far as it is read.

```
(def (count-up n)
  (gen ((fn step [i] (when (< i n) (yield i) (step (+ i 1)))) 0)))
(vec (count-up 4))
(take 3 (iterate inc 10))
```

`vec` drains a generator into a vector, and `map`, `filter`, `reduce`, `take` and `take-while` read one directly. Other functions, such as `count` and `first`, need `vec` first. `iterate` never ends, so only take from it.

## Channels

Channels carry values between tasks. `send` waits while a channel is full and `receive` while it is empty; `(chan 1)` holds one value, and `(chan)` holds 32.

```
(let [ch (chan)]
  (spawn (send ch :ping))
  (receive ch))
```

`close` ends a channel: `receive` gives what is left, then `nil`.

```
(let [ch (chan 4)]
  (send ch 1)
  (send ch 2)
  (close ch)
  [(receive ch) (receive ch) (receive ch)])
```

## Workers

`worker` runs a module's function on another thread, for long computations: `(await (worker 'stats/math 'crunch data))`. It takes and gives plain data only; see [Modules and libraries](modules).

## Staying responsive

oxmacs runs tasks a few milliseconds at a time between keystrokes, so a long loop in a command slows nothing down; it only takes longer. Code that the editor waits for directly has a time limit instead:

- A view's drawing function, a mode's highlighter and a take-over frame are stopped after 250 ms.
- `init.ox`, `custom.ox`, loading a plugin and the REPL are stopped after 10 seconds.

`*ox log*` then says "This took longer than 250 ms and was stopped", and `try` can't catch it. Code you evaluate in an Ox buffer runs as a task, and `C-c C-c` stops it. Keep slow work in a task: `spawn` it, and keep its result in an atom that the view reads.
"#})
