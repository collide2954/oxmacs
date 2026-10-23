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
```

## Maps

Maps keep the order keys were added. `key: value` is short for `:key value`.

```
(def ana {name: "Ana" age: 3})
(get ana :name)
ana.name
(assoc ana :age 4)
(update ana :age inc)
(dissoc ana :age)
```

`a.b.c` reads nested keys and gives `nil` when one is missing. `get-in`, `assoc-in` and `update-in` work on nested maps by path.

## Sets and lists

```
(def seen #{:a :b})
(contains? seen :a)
(conj seen :c)
(list 1 2 3)
```

## Working with sequences

Vectors, maps, sets, lists and strings all work with the same functions. Results are vectors.

```
(map inc [1 2 3])
(filter even? (range 10))
(reduce + 0 [1 2 3])
(sort-by :age [{name: "Bo" age: 5} {name: "Ana" age: 3}])
(group-by even? [1 2 3 4])
(frequencies [:a :b :a])
```

`->>` threads a value through calls as their last argument, and `->` as their first:

```
(->> (range 10)
     (filter odd?)
     (map (fn [n] (* n n)))
     (reduce +))
```

## Destructuring

`let` and function parameters take collections apart:

```
(let [[first-one & others] [1 2 3]
      {name age} {name: "Ana" age: 3}]
  [first-one others name age])
```

## Records

A record is a map with a fixed set of fields and its own type.

```
(defrecord Point [x y])
(def p (Point 1 2))
[p.x (with p :x 10)]
```

## Atoms

An atom holds a value that can be replaced. `@` reads it, `reset!` replaces it and `swap!` applies a function to it.

```
(def counter (atom 0))
(swap! counter inc)
(reset! counter 10)
@counter
```

## Strings

`str` joins values as text. The `ox string` module splits, trims, searches and pads them; see its [reference](ref:ox/string).

```
(str "a" 1 :b)
(count "héllo")
```
"#})
