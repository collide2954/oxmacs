(module (export page))

(def page
  {id: "data"
   title: "Data"
   text: #r#"# Data

Ox data never changes in place. Functions that "change" a collection return a new one and leave the old one as it was, sharing most of its structure, so this is cheap.

## Vectors

```
(def colors [:red :green])
(conj colors :blue)
(nth colors 0)
(count colors)
[(get colors 5) (assoc colors 0 :blue)]
```

`nth` raises a `range-error` for a missing index, where `get` gives `nil`.

## Maps

Maps keep the order keys were added. `key: value` is short for `:key value`, and any value can be a key.

```
(def ana {name: "Ana" age: 3})
(get ana :name)
ana.name
(assoc ana :age 4)
(update ana :age inc)
(dissoc ana :age)
(merge ana {city: "Oslo"})
```

`a.b.c` reads nested keys and gives `nil` when one is missing. `get-in`, `assoc-in` and `update-in` work on nested maps by path, and `keys`, `vals` and `entries` list a map's parts.

## Sets and lists

```
(def seen #{:a :b})
(contains? seen :a)
(conj seen :c)
(disj seen :a)
(list 1 2 3)
```

`contains?` asks about map keys and set members; for a vector, use `some`. `conj` adds to a list at the front and to a vector at the end.

## Working with sequences

Vectors, maps, sets, lists and strings all work with the same functions. Results are vectors: a map gives `[key value]` pairs and a string gives characters.

```
(map inc [1 2 3])
(filter even? (range 10))
(reduce + 0 [1 2 3])
(sort-by :age [{name: "Bo" age: 5} {name: "Ana" age: 3}])
(group-by even? [1 2 3 4])
(frequencies [:a :b :a])
(map (fn [[k v]] [v k]) {a: 1 b: 2})
```

`take`, `drop`, `distinct`, `keep`, `some`, `every?`, `find`, `chunk`, `flatten`, `zip` and `mapcat` are there too; the [reference](ref:ox) has them all.

`->>` threads a value through calls as their last argument, and `->` as their first:

```
(->> (range 10)
     (filter odd?)
     (map (fn [n] (* n n)))
     (reduce +))
```

`some->` stops at the first `nil`, and `as->` names the value so it can go anywhere in each call.

## Destructuring

`let` takes collections apart:

```
(let [[first-one & others] [1 2 3]
      {name age} {name: "Ana" age: 3}
      {city: town} {city: "Oslo"}]
  [first-one others name age town])
```

`{name age}` reads `:name` and `:age`, and `{city: town}` reads `:city` into `town`. A part that is missing is `nil`. Vector and record patterns work in function parameters too, but a map there declares named parameters instead. [Matching](flow) uses the same patterns to test values.

## Records

A record has a fixed set of fields and its own type. Its fields read like map keys, but a record isn't a map: `assoc` changes only the fields it has, and `keys` and `count` don't take it.

```
(defrecord Point [x y])
(def p (Point 1 2))
[p.x (:y p) (assoc p :x 10) (type p) (record? p)]
```

Fields can have defaults, written like parameters:

```
(defrecord Todo [title (done false) {tags []}])
[(Todo "Write") (Todo "Read" true :tags [:book])]
```

`#Point{x: 1 y: 2}` writes a record directly.

## Atoms

An atom holds a value that can be replaced. `@` reads it, `reset!` replaces it and `swap!` applies a function to it. `swap!` is safe when several tasks change the same atom.

```
(def counter (atom 0))
(swap! counter inc)
(reset! counter 10)
@counter
```

## Strings

`str` joins values as text, leaving out `nil`. The `ox string` module splits, trims, searches and pads them; see its [reference](ref:ox/string). Strings count characters, not bytes.

```
(str "a" 1 :b nil)
(count "héllo")
```

`#"…"` writes a regular expression, and the `ox regex` module matches, finds and replaces with it:

```
(module (import (ox regex)))
(regex/replace #"(\d+)-(\d+)" "10-20" "$2-$1")
```
"#})
