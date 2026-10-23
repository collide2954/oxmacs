(module (export page))

(def page
  {id: "tasks"
   title: "Tasks, time and I/O"
   text: #r#"# Tasks, time and I/O

Ox code runs in small slices between keystrokes, so it never freezes the editor. Anything slow, such as reading a file or a web page, returns a promise, and `await` waits for it without blocking anything else.

## Tasks and promises

`spawn` runs code as a task and returns a promise of its result. `await` waits for a promise anywhere, with nothing to mark.

```
(def total (spawn (reduce + (range 1000))))
(await total)
```

`promise` and `deliver` make a promise by hand, for results that arrive from elsewhere.

```
(let [answer (promise)]
  (spawn (deliver answer 42))
  (await answer))
```

## Time

`sleep` pauses a task for some milliseconds. `every` calls a function on a timer until it returns `:stop`.

```
(do (sleep 10) :awake)
```

The `ox time` module reads, formats and adds times in any time zone; see its [reference](ref:ox/time).

## Files, processes and the web

These modules return promises, so their results need `await`:

```text
(module (import (ox file) (ox process) (ox http) (ox json)))

(def notes (await (file/read "~/notes.txt")))
(def listing (await (process/run "git" ["status" "--short"])))
(def reply (await (http/get "https://example.com/data.json")))
(json/read reply.body)
```

`process/run` gives `{status stdout stderr}`, and `http/get` gives `{status headers body}`.

## Generators

A generator yields values one at a time. Sequence functions read generators like vectors.

```
(def (count-up n)
  (gen ((fn step [i] (when (< i n) (yield i) (step (+ i 1)))) 0)))
(vec (count-up 4))
```

## Channels

Channels carry values between tasks. `receive` waits until something arrives.

```
(let [ch (chan)]
  (spawn (send ch :ping))
  (receive ch))
```

`worker` runs a module's function in its own isolate on another thread, for long computations: `(await (worker 'my/module 'crunch data))`.
"#})
