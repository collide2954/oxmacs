(module (export page))

(def page
  {id: "flow"
   title: "Control and errors"
   text: #r#"# Control and errors

## Choosing

`if` takes a test, a form for true and an optional form for false. `when` and `unless` run several forms; `cond` tries tests in order.

```
(def (size n)
  (cond
    [(< n 10) :small]
    [(< n 100) :medium]
    [else :large]))
[(size 5) (size 50) (size 500)]
```

`and` and `or` stop at the first answer and return it. `if-let` and `when-let` bind a value and go on only when it is true, that is, neither `false` nor `nil`:

```
(when-let [age (get {age: 3} :age)]
  (+ age 1))
```

## Matching

`match` compares a value with patterns in turn, runs the first clause that fits and binds the names its pattern holds. A clause can add a guard with `:when`.

```
(def (describe reply)
  (match reply
    [{status: 200 body: body} body]
    [{status: code} :when (>= code 500) :server-error]
    [{status: code} code]))
[(describe {status: 200 body: "ok"}) (describe {status: 503}) (describe {status: 404})]
```

- `_` matches anything. Any other name matches anything too, and binds it; to compare with a name's value, use a guard such as `:when (= n limit)`.
- Numbers, strings, keywords, `true`, `false`, `nil` and quoted symbols such as `'stop` match themselves.
- `[a b]` matches a vector or list of exactly two items, and `[a & more]` one of at least one.
- `{status: 200 body}` matches a map or record that has those keys; it may have others.
- `#Point{x: 0 y}` matches only a `Point`.

A value no clause matches raises a `no-match-error`, so end with `[_ …]` when anything can come.

```
(defrecord Point [x y])
(def (where p)
  (match p
    [#Point{x: 0 y: 0} :origin]
    [#Point{x: 0} :on-the-y-axis]
    [#Point{x y} :when (= x y) :diagonal]
    [_ :elsewhere]))
[(where (Point 0 0)) (where (Point 0 5)) (where (Point 2 2)) (where (Point 1 3))]
```

## Loops

Ox has no loop statement. A function that calls itself last runs in constant space, so recursion is the loop; most of the time `map`, `filter` and `reduce` say it more directly. Calls that aren't last can nest 400,000 deep before Ox raises a "Stack overflow" error.

```
(def (count-down n)
  (if (= n 0) :done (count-down (- n 1))))
(count-down 100000)
```

## Errors

`raise` throws a value. `try` catches by type, and `finally` always runs. `error` makes a plain error, and `deferror` defines an error type with fields and a message that can use them.

```
(deferror http-error [status] "The server answered {status}")
(try
  (raise (http-error 404))
  (catch http-error e e.message))
```

An error is a record: `e.message` is its text, its own fields are there by name, and `e.traceback` lists where it was raised. Catches are tried in order, and one can `raise` the error again. `(catch error e …)` catches every error type, and `(catch _ e …)` also catches values that aren't errors, such as `(raise 42)`.

```
(try
  (raise (http-error 503))
  (catch zero-division-error e :zero)
  (catch http-error e [:http e.status]))
```

Ox's own errors have types: `zero-division-error`, `type-error`, `name-error`, `arity-error`, `range-error`, `no-match-error` and `expectation-failed`, all caught by `error`. `(maybe expr)` gives `nil` instead of raising.

```
[(maybe (/ 1 0)) (try (/ 1 0) (catch zero-division-error e e.message))]
```

An error nothing catches ends the command or task it happened in; oxmacs shows it in the echo area and keeps it in `*ox log*`. Code stopped for running too long can't catch that; see [Tasks](tasks).

## Tests

`test` blocks sit next to the code they test. Loading a file only records them; `ox test` runs them, and `expect` checks a value. A failed `expect` shows the value of each part. See [the ox command](tools).

```
(test "area multiplies"
  (expect (= (* 3 4) 12)))
```

## Macros

`defmacro` writes code that writes code: it runs when the code is read and returns the code to use instead. Backquote builds a form, `~` puts a value in and `~@` splices a list. Names a macro introduces never clash with yours.

```
(defmacro unless-zero [n & body]
  `(if (= ~n 0) nil (do ~@body)))
(unless-zero 5 :ran)
```

`defrule` writes a simple macro as a pattern and a template, where `...` repeats what it follows:

```
(defrule (my-unless test body ...)
  (if test nil (do body ...)))
(my-unless false :ran)
```

`defrules` takes several `[pattern template]` rules and uses the first that fits. The words in its first vector match only themselves.

```
(defrules swap-order [with]
  [(swap-order a with b) [b a]]
  [(swap-order a) [a]])
[(swap-order 1 with 2) (swap-order 3)]
```
"#})
