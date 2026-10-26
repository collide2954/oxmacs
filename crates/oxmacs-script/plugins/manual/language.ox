(module (export page))

(def page
  {id: "language"
   title: "The language"
   text: #r##"# The language

An Ox program is a series of forms. A list `(f a b)` calls `f` with `a` and `b`; any other form is a value, or the value of a name.

```
(+ 1 2 3)
(* 2 (+ 3 4))
```

## Values

- Numbers: `42`, `-3.5`, `1e3`. Whole numbers grow as large as they need to. `/` always gives a float and `//` a whole number; `=` tells `1` from `1.0`, and `==` doesn't.
- Strings: `"text"`, with `\n`, `\t`, `\"`, `\\` and `\u{263A}` escapes. `{expression}` inside a string inserts its value; write `{{` and `}}` for braces.
- Raw strings: `#r"C:\temp\{x}"` keeps backslashes and braces as they are. Write `#r#"…"#` when the text holds a quote.
- Keywords: `:name`, or `name:` in front of a value. They name things, mostly as map keys.
- Symbols: `'name` is a name as data, as when `(bind-key "C-c h" 'hello)` names a command.
- `true`, `false` and `nil`. Only `false` and `nil` are false; `0`, `""` and `[]` are true.
- Characters: `\a`, `\space`, `\newline`, `\tab`, `\u{263A}`.
- Vectors `[1 2 3]`, maps `{name: "Ana" age: 3}` and sets `#{:red :green}`, covered in [Data](data).

```
(let [name "Ana"]
  "Hello, {name}! You have {(+ 1 2)} new messages.")
[(/ 7 2) (// 7 2) (= 1 1.0) (== 1 1.0)]
```

## Names

`def` names a value for the whole module. `let` names values for its body, each one seeing those before it.

```
(def limit 10)
(let [x 2
      y (* x limit)]
  (+ x y))
```

`set!` assigns a new value to a local or global name. Values themselves never change; [atoms](data) are how state changes. `defonce` defines a name only if it isn't defined yet, so loading a file again keeps its value.

## Functions

`fn` makes a function, and `def` with a list names one. A function returns its last form.

```
(def (area w h)
  "Width times height."
  (* w h))
(area 3 4)
((fn [x] (* x x)) 5)
```

A string right after the parameters documents the function; `C-c C-d` shows it. A name after `fn`, as in `(fn step [i] …)`, lets the function call itself.

Parameters can have defaults, collect the rest, or be passed by name:

```
(def (greet name (greeting "Hello") {loud false})
  (let [text "{greeting}, {name}"]
    (if loud (str text "!") text)))
(greet "Ana")
(greet "Bo" "Hi" :loud true)
(greet "Cy" :loud true)
```

- `(greeting "Hello")` is optional with a default, and `(greeting)` defaults to `nil`.
- `(def (log level & parts) …)` collects the arguments after `level` in the list `parts`.
- A map of names and defaults comes last and takes `:name value` pairs, in any order.
- A vector or record parameter takes its argument apart, as `let` does in [Data](data).

Functions are values: pass them, return them, keep them in maps. Keywords, maps and sets can be called too: `(:name person)` reads a key, and `(:name person "nobody")` gives a default.

```
(def (twice f x) (f (f x)))
(twice (fn [n] (* n 3)) 2)
(map :name [{name: "Ana"} {name: "Bo"}])
[((partial + 10) 5) ((comp inc abs) -3) (apply + 1 [2 3])]
```

## Comments

`;` comments to the end of the line, `#;` comments out the form after it, and `#|` … `|#` comments out a block, nesting. Commas count as spaces.

## Modules

Each file is a module. Its header imports other modules and can list what it exports:

```text
(module
  (import (ox string) (ox json :as js))
  (export summary))
```

Imported names are called with the module's last word or alias: `string/split`, `js/read`. Built-in names such as `map` and `str` need no import, and the `ox/...` modules need only the import; everything more comes from plugins, which serve as Ox's standard library. [Modules and libraries](modules) covers both, and the [reference](ref:ox) lists every name.
"##})
