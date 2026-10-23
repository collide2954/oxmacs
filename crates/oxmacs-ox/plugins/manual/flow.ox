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

`and` and `or` stop at the first answer and return it. `if-let` and `when-let` bind a value and go on only when it isn't `nil`:

```
(when-let [age (get {age: 3} :age)]
  (+ age 1))
```

## Matching

`match` compares a value with patterns and binds what they name. Patterns can be values, vectors, maps and records, and a clause can add a guard with `:when`.

```
(def (describe reply)
  (match reply
    [{status: 200 body: body} body]
    [{status: code} :when (>= code 500) :server-error]
    [{status: code} code]))
[(describe {status: 200 body: "ok"}) (describe {status: 503}) (describe {status: 404})]
```

## Loops

Ox has no loop statement. A function that calls itself last runs in constant space, so recursion is the loop; most of the time `map`, `filter` and `reduce` say it more directly.

```
(def (count-down n)
  (if (= n 0) :done (count-down (- n 1))))
(count-down 100000)
```

## Errors

`raise` throws a value. `try` catches by type, and `finally` always runs. `error` makes a plain error, and `deferror` defines an error type with fields and a message.

```
(deferror http-error [status] "The server answered {status}")
(try
  (raise (http-error 404))
  (catch http-error e e.message))
```

Ox's own errors have types: `zero-division-error`, `type-error`, `name-error`, `arity-error`, `range-error` and `no-match-error`, all caught by `error`. `(maybe expr)` gives `nil` instead of raising.

```
[(maybe (/ 1 0)) (try (/ 1 0) (catch zero-division-error e e.message))]
```

## Tests

`test` blocks sit next to the code they test. Loading a file skips them; `ox test` and the [ox command](tools) run them. A failed `expect` shows the value of each part.

```
(test "area multiplies"
  (expect (= (* 3 4) 12)))
```

## Macros

`defmacro` writes code that writes code. Backquote builds a form, `~` puts a value in and `~@` splices a list. Names a macro introduces never clash with yours.

```
(defmacro unless-zero [n & body]
  `(if (= ~n 0) nil (do ~@body)))
```

```
(unless-zero 5 :ran)
```

`defrule` writes a simple macro as a pattern and a template:

```
(defrule (my-unless test body ...)
  (if test nil (do body ...)))
```
"#})
