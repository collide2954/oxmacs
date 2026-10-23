(module (export page))

(def page
  {id: "language"
   title: "The language"
   text: #r#"# The language

An Ox program is a series of forms. A list `(f a b)` calls `f` with `a` and `b`; any other form is a value, or the value of a name.

```
(+ 1 2 3)
(* 2 (+ 3 4))
```

## Values

- Numbers: `42`, `-3.5`, `1_000_000`, `0xff`. Whole numbers grow as large as they need to.
- Strings: `"text"`. `{expression}` inside a string inserts its value; write `{{` for a brace.
- Keywords: `:name`. They name things, mostly as map keys.
- `true`, `false` and `nil`. Only `false` and `nil` are false; `0` and `""` are true.
- Characters: `\a`.
- Vectors `[1 2 3]`, maps `{name: "Ana" age: 3}` and sets `#{:red :green}`, covered in [Data](data).

```
(let [name "Ana"]
  "Hello, {name}! You have {(+ 1 2)} new messages.")
```

## Names

`def` names a value for the whole module. `let` names values for its body, each one seeing those before it.

```
(def limit 10)
(let [x 2
      y (* x limit)]
  (+ x y))
```

`set!` assigns a new value to a local or global name. Values themselves never change; [atoms](data) are how state changes.

## Functions

`fn` makes a function, and `def` with a list names one. A function returns its last form.

```
(def (area w h)
  "Width times height."
  (* w h))
(area 3 4)
((fn [x] (* x x)) 5)
```

A string right after the parameters documents the function; `C-c C-d` shows it.

Parameters can have defaults, collect the rest, or be passed by name:

```
(def (greet name (greeting "Hello") {loud false})
  (let [text "{greeting}, {name}"]
    (if loud (str text "!") text)))
(greet "Ana")
(greet "Bo" "Hi" :loud true)
```

`(def (log level & parts) …)` collects the arguments after `level` in `parts`.

Functions are values: pass them, return them, keep them in maps. Keywords, maps and sets can be called too: `(:name person)` reads a key.

```
(def (twice f x) (f (f x)))
(twice (fn [n] (* n 3)) 2)
(map :name [{name: "Ana"} {name: "Bo"}])
```

## Comments

`;` comments to the end of the line, and `#;` comments out the form after it.

## Modules

Each file is a module. Its header imports other modules and can list what it exports:

```text
(module
  (import (ox string) (ox json :as js))
  (export summary))
```

Imported names are called with the module's last word or alias: `string/split`, `js/read`. The [reference](ref:ox) lists every module.
"#})
