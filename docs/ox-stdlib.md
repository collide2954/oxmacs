# The Ox standard library

Companion to [ox-design.md](ox-design.md) and [ox-internals.md](ox-internals.md). Agreed in four rounds of questions on 2026-10-21.

## Principles

- **Clojure-style names.** The vocabulary is built for immutable data and keywords: `count`, `first`, `rest`, `conj`, `assoc`, `dissoc`, `get`, `update`, `map`, `filter`, `reduce`, `keep`, `str`, `keys`, `vals`.
- **One sequence interface.** Lists, vectors, maps, sets, strings, ranges and generators all work with the same functions. Results are eager vectors by default. `(iter …)` pipelines stay lazy, for streams and huge data.
- **Protocols for polymorphism.** `defprotocol` and `extend` give fast, type-checked dispatch on type. The library itself is built on protocols for `=`, `hash`, `print`, `compare` and sequences, so records join in.
- **Batteries included.** Strings, math, regexes, JSON and TOML, dates and times, paths and files, processes, HTTP and concurrency all come with Ox. Each module sits on the core's native primitives. The editor API is separate, `(ox editor)`.
- **Typed and tested.** Every function has a type signature, which feeds inference, and documentation with examples that run as tests.
- **Failure raises.** Functions that can fail raise a typed error with a clear message, such as `parse-error`, `io-error` or `http-error`. `(maybe expr)` turns any error into `nil` when you would rather check, so no function needs a second "or nil" twin.
- **Missing keys give `nil`.** `(get m :k)` returns `nil` when the key is absent, and the checker types the result as `V?`, so nil safety makes you handle it. `(get m :k default)` gives a fallback, and field access on a map shape with a required key gives the plain type. `a.b` reads a key the same way, and gives `nil` through `nil`, so `buffer.file.path` is `nil` when there is no file.
- **Equality is exact.** `(= 1 1.0)` is false, so maps and sets never confuse an integer with a float. `(== 1 1.0)` is true; it compares numbers by value. Sequences compare by their items, so `(= [1 2] (list 1 2))` is true, and since `()` is `nil`, `(= [] nil)` is true too. On floats `=` is an equivalence (NaN equals NaN, and `0.0` differs from `-0.0`), so NaN keys work in maps and sets; `==` follows IEEE.
- **Maps keep insertion order.** Printing, iteration and JSON round-trips follow the order keys were added, so output and snapshots are predictable. Keyword keys print with the literal shorthand, `{name: "Ana" age: 3}`, and other keys as themselves, `{"a b" 1 42 "x"}`.
- **I/O never blocks.** File, process and network functions return promises to `await`, so no plugin can freeze typing. In the `ox` command line and REPL, top-level `await` just works.
- **Naming conventions.**
  - kebab-case;
  - `?` ends predicates and `!` ends functions that mutate;
  - conversions are named after the target type: `int`, `float`, `str`, `vec`, `set`, `keyword`.

## Syntax conveniences

```clojure
(->> forecast.days
     (filter #(> %.high 30))
     (map :day)
     (take 3))

(for [day days :when (> day.high 30) :let [f (fahrenheit day.high)]]
  "{day.name}: {f:.1}°F")

(each [line (s/lines text)] (println line))

(loop [i 0 total 0]
  (if (< i 10) (again (+ i 1) (+ total i)) total))

(while (running?) (step!))
```

- **Function shorthand.** `#(…)` builds a function: `%` (or `%1`) is its first argument, `%2` its second, and `%&` the rest.
- **Threading.** `->` threads a value as the first argument of each call and `->>` as the last; `some->` stops at `nil`; `as->` names the value.
- **Keywords, maps and sets as functions.** `(:name city)` and `(map :name cities)` read a key, and `(city :name)` does too. A set gives the item when it is a member and `nil` otherwise, so `(filter #{:todo :fixme} kinds)` works. Vectors are not callable; use `nth` or `get`.
- **Loops.**
  - `for` is a comprehension that builds a vector, with `:when`, `:let` and nested bindings.
  - `each` runs for side effects and returns `nil`.
  - `loop`/`again` is a local tail loop compiled to a jump.
  - `while` and `until` repeat on mutable state.
- **Format specs.** Format specs go inside interpolation: `"{price:.2}"`, `"{name:>10}"` and `"{count:,}"` (digit grouping).
- **Literals.**

  | Literal | Meaning |
  | --- | --- |
  | `#{:red :green}` | a set |
  | `#"\d+-\w+"` | a regex, compiled once and cached, with no doubled backslashes |
  | `#r"C:\notes\{draft}"`, `#r#"{"a": 1}"#` | a raw string: no escapes and no interpolation; add `#`s around the quotes to put quotes inside |
  | `#t"2026-10-21"`, `#t"2026-10-21T09:30-05:00"` | a date or instant, checked when read |
  | `#p"~/notes/plan.atlas"` | a path; `~` expands and separators are normalized on every system |

## The prelude (no import needed)

| Area | Functions and forms |
| --- | --- |
| Numbers | `+ - * / // mod rem inc dec abs min max == < > <= >= zero? pos? neg? even? odd? int float round floor ceil` |
| Comparing | `= not= compare identical? hash` |
| Logic | `not and or if when unless cond case if-let when-let some-> -> ->> as->` |
| Types | `nil? some? bool? number? int? float? string? char? keyword? symbol? vector? map? set? list? fn? type` |
| Sequences | `count empty? first second last rest nth get contains? conj cons concat into vec set list map filter remove keep reduce some every? take drop take-while drop-while partition chunk group-by sort sort-by reverse distinct frequencies range repeat iterate zip interleave flatten mapcat find iter` |
| Maps | `assoc dissoc update merge merge-with select-keys keys vals entries get-in assoc-in update-in` |
| Strings | `str name keyword symbol` (the rest are in `ox string`) |
| Functions | `identity constantly comp partial apply juxt memoize` |
| State | `atom deref reset! swap! compare-and-set!`; `swap!` retries until no other task changed the atom meanwhile |
| Records and protocols | `defrecord with defprotocol extend satisfies?` |
| Errors | `raise try catch maybe deferror error` |
| Output | `print println pr prn` |
| Generators | `gen generator yield next done?` |
| Async | `spawn await promise deliver fail worker chan send receive close chan? sleep` |
| Tests | `test expect for-all expect-snapshot` |

## Modules

Modules are imported with aliases and called qualified:

```clojure
(module
  (import (ox json)
          (ox http :as http)
          (ox time :only [now days])))
```

| Module | Contents |
| --- | --- |
| `ox string` | `to-number split join trim trim-start trim-end upper lower capitalize replace starts-with? ends-with? includes? index-of pad-start pad-end lines words chars reverse blank? truncate` |
| `ox regex` | `find find-all matches? replace split`; each match is `{text start end groups named}`, with character positions, numbered groups in `groups` and named ones in `named`; `replace` expands `$1` and `${name}`; patterns can be `#"…"` regexes or strings |
| `ox math` | `sqrt pow exp log log10 sin cos tan atan2 pi e clamp lerp random random-int random-pick` |
| `ox json` | `read write`; objects read with keyword keys, and keywords are written back as strings |
| `ox toml` | `read write` |
| `ox time` | `now today instant date datetime zone parse format plus minus between weekday` and durations: `millis seconds minutes hours days` |
| `ox path` | `join parent name stem extension absolute expand relative-to home` |
| `ox file` | `read write append lines exists? list glob walk remove rename copy metadata watch` (all async) |
| `ox process` | `run`, which awaits `{status stdout stderr}`; `spawn`, with line streams; `which`, `env` |
| `ox http` | `get post put patch delete request` (all async), each awaiting `{status headers body}`; JSON bodies are encoded and decoded automatically |
| `ox async` | `select timeout every all race` |
| `ox test` | the runner's API: run, filter and report; the test forms themselves are in the prelude |
| `ox editor` | the editor API (phase 2) |
| `ox ui` | views, the canvas and decorations (phase 2) |

## Concurrency

- **Promises.** `(spawn expr)` runs code as a green task and returns a promise of its result. `await` waits for a promise anywhere, with no `async` marking, because tasks are stackful. `promise`, `deliver` and `fail` make promises by hand.
- **Generators.** `(gen …)` makes a generator. `yield` works from any function the generator calls, `next` gives nil at the end, and `done?` tells the end apart from a yielded nil. Generators work with `map`, `filter`, `reduce` and `vec`.
- **Channels.** `chan`, `send`, `receive` and `close` carry streams between tasks and between worker isolates; `(chan)` holds 32 items, `send` waits when it is full, and after `close` `receive` gives nil and `done?` turns true. `select` (with timeouts) comes with `ox async`.
- **Workers.** `(worker 'module 'function args…)` runs a module's function in its own isolate on another thread and returns a promise of the result. Only data, records, channels and shared Rust values can be sent.
- **Timers.** `sleep` and `every`.
- **Fairness.** All of these cooperate with the UI isolate's time slices, so waiting never blocks a keystroke.

## Building it

The library grows alongside the language slices. Each function is added with its tests, its documentation examples and, once the checker exists, its type. Most of it is written in Ox, compiled into the bytecode cache and loaded lazily. Only the primitives live in Rust: arithmetic, string and collection internals, regexes, JSON and TOML, time, files, processes, HTTP and channels.
