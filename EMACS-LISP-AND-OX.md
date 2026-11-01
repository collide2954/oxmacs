# Emacs Lisp and Ox, side by side

This page puts Emacs Lisp next to Ox, the language oxmacs is extended in, one task at a time. It's meant for people who already write Emacs Lisp and want to read or write Ox quickly. For the bigger picture, see [oxmacs and GNU Emacs](EMACS.md); for everything about Ox, the manual inside oxmacs (`C-h i`).

Ox code blocks here are checked with `ox check`. Names prefixed `my-` in the Emacs Lisp follow its convention of prefixing everything, which Ox modules make unnecessary.

## Quick reference

| Emacs Lisp | Ox |
| --- | --- |
| `(defun f (x) …)` | `(def (f x) …)` |
| `(defvar x 1)`, `(setq x 2)` | `(def x 1)`, `(set! x 2)` |
| `(lambda (x) …)` | `(fn [x] …)` |
| `(funcall f x)`, `#'f` | `(f x)`, `f` |
| `(let ((a 1) (b 2)) …)`, `let*` | `(let [a 1 b 2] …)`, always sequential |
| `t`, `nil` | `true`, `false`, `nil` |
| `(null x)` | `(nil? x)` |
| `(equal a b)` | `(= a b)` |
| `(format "%s: %d" a b)` | `$"{a}: {b}"` |
| `(concat "a" "b")` | `(str "a" "b")` |
| `(number-to-string n)` | `(str n)` |
| `(string-to-number s)` | `(string/to-number s)` |
| `(length xs)` | `(count xs)` |
| `(nth 1 xs)` | `(nth xs 1)` |
| `(car xs)`, `(cdr xs)` | `(first xs)`, `(rest xs)` |
| `(append a b)` | `(concat a b)` |
| `(mapcar f xs)` | `(map f xs)` |
| `(seq-filter f xs)` | `(filter f xs)` |
| `(seq-reduce f xs init)` | `(reduce f init xs)` |
| `(seq-find f xs)` | `(find f xs)` |
| `(seq-position xs x)` | `(index-of xs x)` |
| `(seq-uniq xs)` | `(distinct xs)` |
| `(sort xs #'<)` | `(sort xs)` |
| `(gethash k h)`, `(alist-get k al)` | `(get m k)` or `m.k` |
| `(puthash k v h)` | `(assoc m k v)`, which gives a new map |
| `(remhash k h)` | `(dissoc m k)` |
| `(hash-table-keys h)` | `(keys m)` |
| `(when-let ((x …)) …)` | `(when-let [x …] …)` |
| `(dolist (x xs) …)` | `(each [x xs] …)` |
| `(dotimes (i n) …)` | `(each [i (range n)] …)` |
| `(cl-loop for x in xs collect …)` | `(for [x xs] …)` |
| `(pcase x …)` | `(match x …)` |
| `(cl-defstruct point x y)` | `(defrecord Point [x y])` |
| `(condition-case e … (error …))` | `(try … (catch error e …))` |
| `(unwind-protect body cleanup)` | `(try body (finally cleanup))` |
| `(error "Oops")` | `(raise (error "Oops"))` |
| `(ignore-errors x)` | `(maybe x)` |
| `(string-match re s)`, `(match-string 1 s)` | `(regex/find s #"…")`, its `:groups` |
| `(replace-regexp-in-string re rep s)` | `(string/replace s #"…" rep)` |
| `(split-string s ",")` | `(string/split s ",")` |
| `(string-trim s)` | `(string/trim s)` |
| `(string-prefix-p p s)` | `(string/starts-with? s p)` |
| `(upcase s)` | `(string/upper s)` |
| `(expand-file-name "~/x")` | `(path/expand "~/x")` |
| `(file-exists-p f)` | `(file/exists? f)` |
| `(shell-command-to-string "git status")` | `(process/run "git" ["status"])` |
| `(run-at-time 2 nil f)` | `(spawn (sleep 2000) (f))` |
| `(sleep-for 1)` | `(sleep 1000)`, which pauses only its task |
| `(json-parse-string s)`, `(json-encode x)` | `(json/read s)`, `(json/write x)` |
| `(defun f () (interactive) …)` | `(defcommand f "Doc" …)` |
| `(global-set-key (kbd "C-c h") #'f)` | `(bind-key "C-c h" 'f)` |
| `(read-string "Name: ")` | `(prompt "Name: ")` |
| `(y-or-n-p "Sure? ")` | `(yes-or-no "Sure? ")` |
| `(completing-read "Pick: " xs)` | `(prompt "Pick: " :choices xs)` |
| `(with-current-buffer b …)` | `(with-buffer b …)` |
| `(buffer-string)` | `(buffer-text)` |
| `(point)`, `(goto-char p)` | `(point)`, `(set-point p)`, counting from 0 |
| `(add-hook 'after-save-hook f)` | `(on-saved :key f)` |
| `(defcustom …)` | `:settings` in the plugin's `plugin.ox` |
| `(require 'x)`, `(provide 'x)` | `(module (import …) (export …))` |
| `(ert-deftest …)`, `(should …)` | `(test …)`, `(expect …)` |

## Functions

Emacs Lisp keeps functions and variables in separate namespaces, so passing a function takes `#'` and calling one from a variable takes `funcall`. Ox has one namespace, and a function in a variable is called like any other.

```elisp
(defun my-area (w h)
  "Width times height."
  (* w h))

(defun my-twice (f x)
  (funcall f (funcall f x)))

(my-twice #'1+ 5)
```

```clojure
(def (area w h)
  "Width times height."
  (* w h))

(def (twice f x)
  (f (f x)))

(twice inc 5)
```

Optional, rest and named parameters are part of `def` itself; Emacs Lisp needs `cl-defun` for defaults and keywords.

```elisp
(cl-defun my-greet (name &optional (greeting "Hello") &key loud)
  (let ((text (format "%s, %s" greeting name)))
    (if loud (concat text "!") text)))

(my-greet "Bo" "Hi" :loud t)
```

```clojure
(def (greet name (greeting "Hello") {loud false})
  (let [text $"{greeting}, {name}"]
    (if loud (str text "!") text)))

(greet "Bo" "Hi" :loud true)
```

## Truth and equality

In Emacs Lisp, `nil` is false, the empty list and the symbol `nil` all at once, and there are three equalities. In Ox, `false` and `nil` are the only false values, an empty vector is a vector, and `=` compares values.

```elisp
(if '() 'yes 'no)                     ; no: the empty list is nil
(eq (list 1 2) (list 1 2))            ; nil: two different lists
(equal (list 1 2) (list 1 2))         ; t
```

```clojure
(if [] :yes :no)                      ; :yes
(= [1 2] [1 2])                       ; true
[(= 1 1.0) (== 1 1.0)]                ; [false true]: == compares numbers by value
```

## Strings

Ox fills in values with `$"…"`, joins with `str`, and keeps its string functions in the `ox/string` module.

```elisp
(let ((who "Ana") (n 3))
  (format "%s has %d items" who n))
(string-join (split-string "a,b,c" ",") " and ")
(string-trim "  hi  ")
(upcase "hi")
```

```clojure
(module (import (ox string)))

(let [who "Ana" n 3]
  $"{who} has {n} items")
(string/join " and " (string/split "a,b,c" ","))
(string/trim "  hi  ")
(string/upper "hi")
```

Text that should stay exactly as written goes in `"""` quotes, without escapes:

```clojure
"""C:\Users\ana\notes"""
```

## Regular expressions

An Emacs Lisp regex is a string, so its backslashes are doubled, and results come back through global match data. An Ox regex is a value written `#"…"`, checked before the code runs, and a match is a map you can keep.

```elisp
(when (string-match "\\([0-9]+\\)-\\([0-9]+\\)" "pages 10-24")
  (list (match-string 1 "pages 10-24")
        (match-string 2 "pages 10-24")))   ; ("10" "24")

(replace-regexp-in-string "[0-9]+" "#" "a1b22")   ; "a#b#"
```

```clojure
(module (import (ox regex) (ox string)))

(:groups (regex/find "pages 10-24" #"(\d+)-(\d+)"))   ; ["10" "24"]

(string/replace "a1b22" #"\d+" "#")                    ; "a#b#"
```

The `ox string` functions that look for a piece of text, `string/replace`, `string/split` and `string/includes?`, take a regex in its place too.

## Lists, vectors and maps

Emacs Lisp data changes in place: `push`, `puthash` and `setf` alter what's there. Ox data never changes; functions return a new value that shares most of its structure with the old one, and atoms hold the state that changes.

```elisp
(defvar my-names (list "Bo"))
(push "Ana" my-names)                  ; my-names is now ("Ana" "Bo")

(let ((person (make-hash-table :test #'equal)))
  (puthash "name" "Ana" person)
  (puthash "age" 3 person)
  (gethash "name" person))

(alist-get 'age '((name . "Ana") (age . 3)))
(plist-get '(:name "Ana" :age 3) :age)
```

```clojure
(def names ["Bo"])
(conj names "Ana")                     ; ["Bo" "Ana"]; names is still ["Bo"]

(def person {name: "Ana" age: 3})
person.name                            ; "Ana"
(assoc person :age 4)                  ; {name: "Ana" age: 4}
(update person :age inc)               ; {name: "Ana" age: 4}

(def visits (atom 0))
(swap! visits inc)
@visits                                ; 1
```

Sets are built in too: `#{:red :green}`, with `contains?`, `conj` and `disj`.

## Taking values apart

Emacs Lisp has several ways to destructure: `pcase-let`, `cl-destructuring-bind`, `seq-let` and `let-alist`. Ox has one: `let`, function parameters and `match` all take the same patterns.

```elisp
(cl-destructuring-bind (a b &rest more) '(1 2 3 4)
  (list a b more))                     ; (1 2 (3 4))

(let-alist '((name . "Ana") (age . 3))
  (list .name .age))                   ; ("Ana" 3)
```

```clojure
(let [[a b & more] [1 2 3 4]
      {name age} {name: "Ana" age: 3}]
  [a b more name age])                 ; [1 2 [3 4] "Ana" 3]
```

## Choosing and matching

`cond` reads the same in both, with Ox clauses in brackets and `else` for the fallback. `match` plays the part of `pcase`, matching maps and records by their keys.

```elisp
(defun my-size (n)
  (cond ((< n 10) 'small)
        ((< n 100) 'medium)
        (t 'large)))

(defun my-describe (reply)            ; reply is (STATUS . BODY)
  (pcase reply
    (`(200 . ,body) body)
    (`(,code . ,_) (if (>= code 500) 'server-error code))))
```

```clojure
(def (size n)
  (cond
    [(< n 10) :small]
    [(< n 100) :medium]
    [else :large]))

(def (describe reply)
  (match reply
    [{status: 200 body: body} body]
    [{status: code} :when (>= code 500) :server-error]
    [{status: code} code]))
```

## Loops

```elisp
(dolist (name '("Ada" "Grace"))
  (message "Hello, %s" name))

(cl-loop for x from 0 below 5
         when (cl-oddp x) collect (* x x))   ; (1 9)

(let ((i 0) (total 0))
  (while (< i 10)
    (setq total (+ total i)
          i (1+ i)))
  total)                                     ; 45
```

```clojure
(each [name ["Ada" "Grace"]]
  (message $"Hello, {name}"))

(for [x (range 5) :when (odd? x)]
  (* x x))                                   ; [1 9]

(loop [i 0 total 0]
  (if (< i 10)
    (again (+ i 1) (+ total i))
    total))                                  ; 45
```

`loop` and `again` run in constant space, and so does a function that calls itself last.

## Records

```elisp
(cl-defstruct my-point x y)

(let ((p (make-my-point :x 1 :y 2)))
  (my-point-x p))
```

```clojure
(defrecord Point [x y])

(let [p (Point 1 2)]
  p.x)
```

Records match by type in `match`, as `#Point{x: 0 y}`, and errors are records too.

## Errors

Both languages define error types and clean up whatever happens. In Ox, an error carries named fields and a message built from them, and `C-g` raises a quit that a `catch` for errors lets pass, as `condition-case` does with `quit`.

```elisp
(define-error 'my-http-error "HTTP error")

(condition-case err
    (signal 'my-http-error '(404))
  (my-http-error (format "The server answered %s" (cadr err))))

(unwind-protect
    (call-process "make")
  (message "Done"))

(ignore-errors (/ 1 0))
```

```clojure
(module (import (ox process)))

(deferror http-error [status] $"The server answered {status}")

(try
  (raise (http-error 404))
  (catch http-error e e.message))

(try
  (process/run "make" [])
  (finally (message "Done")))

(maybe (/ 1 0))
```

## Files

```elisp
(with-temp-buffer
  (insert-file-contents "notes.txt")
  (buffer-string))

(with-temp-file "out.txt"
  (insert "hello"))

(directory-files (expand-file-name "~/notes") nil "\\.txt\\'")
```

```clojure
(module (import (ox file) (ox path) (ox string)))

(file/read "notes.txt")

(file/write "out.txt" "hello")

(map :name (filter (fn [entry] (string/ends-with? entry.name ".txt"))
                   (file/list (path/expand "~/notes"))))
```

Ox's file calls run off the editor's thread and pause only the code that made them.

## Programs

Running a program and reading its output is one call in both. Reading the output while the program runs takes a process filter in Emacs Lisp; in Ox, the lines arrive on a channel.

```elisp
(shell-command-to-string "git status --short")

(make-process
 :name "log"
 :command '("git" "log" "--oneline")
 :filter (lambda (_process output)
           (message "%s" output)))
```

```clojure
(module (import (ox process)))

(let [ran (process/run "git" ["status" "--short"])]
  ran.stdout)

(let [git (process/start "git" ["log" "--oneline"])]
  (loop []
    (when-let [line (receive git.lines)]
      (message line)
      (again))))
```

## The web and JSON

```elisp
(url-retrieve
 "https://api.github.com/repos/collide2954/oxmacs"
 (lambda (_status)
   (goto-char url-http-end-of-headers)
   (let ((repo (json-parse-buffer :object-type 'alist)))
     (message "%s stars" (alist-get 'stargazers_count repo)))))
```

```clojure
(module (import (ox http) (ox json)))

(let [reply (http/get "https://api.github.com/repos/collide2954/oxmacs")
      repo (json/read reply.body)]
  (message $"{repo.stargazers_count} stars"))
```

`json/read` turns objects into maps with keyword keys, so their fields read with a dot.

## Timers and waiting

`sleep-for` and `sit-for` stop the whole editor; `sleep` in Ox pauses only its own task.

```elisp
(run-at-time 2 nil (lambda () (message "Two seconds later")))
(run-with-timer 0 300 (lambda () (message "Stretch")))
```

```clojure
(spawn (sleep 2000) (message "Two seconds later"))
(every 300000 (fn [] (message "Stretch")))
```

`every` stops when its function returns `:stop`.

## Running things at once

Emacs Lisp threads take turns, and only one runs at a time. Ox tasks also take turns on the editor's thread, but anything that waits gives way, so several requests can be in flight together. `worker` runs heavy computation on another thread.

```elisp
(make-thread (lambda () (url-retrieve-synchronously "https://example.com/a")))
```

```clojure
(module (import (ox http)))

(let [a (spawn (http/get "https://example.com/a"))
      b (spawn (http/get "https://example.com/b"))]
  [(get (await a) :status) (get (await b) :status)])

(let [ch (chan)]
  (spawn (send ch :ping))
  (receive ch))                        ; :ping
```

## Generators

```elisp
(require 'generator)

(iter-defun my-count-up (n)
  (dotimes (i n)
    (iter-yield i)))

(cl-loop for x iter-by (my-count-up 4) collect x)   ; (0 1 2 3)
```

```clojure
(def (count-up n)
  (gen (loop [i 0]
         (when (< i n)
           (yield i)
           (again (inc i))))))

(vec (count-up 4))                                 ; [0 1 2 3]
```

## Commands and keys

```elisp
(defun my-hello ()
  "Say hello."
  (interactive)
  (message "Hello"))

(global-set-key (kbd "C-c h") #'my-hello)
```

```clojure
(defcommand hello "Say hello"
  (message "Hello"))

(bind-key "C-c h" 'hello)
```

Commands run as tasks, so a command can ask a question or wait on the network without holding up typing.

## Asking

```elisp
(let ((name (read-string "Name: ")))
  (when (y-or-n-p (format "Greet %s? " name))
    (message "Hello, %s" name)))

(completing-read "Color: " '("red" "green"))
```

```clojure
(let [name (prompt "Name: ")]
  (when (yes-or-no $"Greet {name}? ")
    (message $"Hello, {name}")))

(prompt "Color: " :choices ["red" "green"])
```

## Buffers

Buffer positions count from 1 in Emacs Lisp and from 0 in Ox.

```elisp
(with-current-buffer "notes.txt"
  (goto-char (point-max))
  (insert "\nDone"))

(with-current-buffer "notes.txt"
  (buffer-string))
```

```clojure
(with-buffer "notes.txt"
  (set-point (buffer-size))
  (insert "\nDone"))

(with-buffer "notes.txt"
  (buffer-text))
```

## Hooks

An Ox hook is registered under a key, and registering the same key again replaces it, so reloading a file never adds a second copy.

```elisp
(add-hook 'after-save-hook
          (lambda () (message "Saved %s" buffer-file-name)))
```

```clojure
(on-saved :note
  (fn [path] (message $"Saved {path}")))
```

## Settings

Emacs Lisp declares an option with `defcustom`; an Ox plugin declares its settings in its manifest, where they get a type that is checked, a form in `M-x customize`, and a place in `custom.ox`.

```elisp
(defcustom my-width 80
  "Columns to fill to."
  :type 'integer
  :group 'my-wrap)

(setq my-width 72)
```

```clojure
(plugin wrap
  :version "0.1.0"
  :settings {width: {type: :integer default: 80 doc: "Columns to fill to"}}
  :entry "wrap.ox")
```

```clojure
(settings 'wrap {width: 72})
```

Inside the plugin, `(setting :width)` reads it.

## Packages and modules

An Emacs Lisp package lives in the one global namespace, so every name carries its prefix. An Ox file is a module that imports what it uses and exports what it offers, and a plugin's `plugin.ox` names the plugins it needs with their versions.

```elisp
;;; my-notes.el --- Notes  -*- lexical-binding: t; -*-
(require 'subr-x)

(defun my-notes-count (notes)
  (seq-count (lambda (note) (not (string-blank-p note))) notes))

(provide 'my-notes)
```

```clojure
(module
  (import (ox string))
  (export note-count))

(def (note-count notes)
  (count (remove string/blank? notes)))
```

## Macros

Both write macros with backquote; Ox writes `~` and `~@` where Emacs Lisp writes `,` and `,@`. Names an Ox macro introduces never capture the caller's, so it needs no `make-symbol`.

```elisp
(defmacro my-unless-zero (n &rest body)
  (let ((v (make-symbol "v")))
    `(let ((,v ,n))
       (if (= ,v 0) nil ,@body))))
```

```clojure
(defmacro unless-zero [n & body]
  `(let [v ~n]
     (if (= v 0) nil (do ~@body))))
```

## Tests

```elisp
(defun my-area (w h)
  (* w h))

(ert-deftest my-area-test ()
  (should (= (my-area 3 4) 12)))
```

```clojure
(def (area w h)
  (* w h))

(test "area multiplies"
  (expect (= (area 3 4) 12)))
```

`M-x ert` runs Emacs Lisp tests; `ox test` runs the `test` blocks of a file or folder, and a failed `expect` shows the value of each part.

## Tools

| Job | Emacs Lisp | Ox |
| --- | --- | --- |
| Find mistakes | byte-compiler warnings, checkdoc, package-lint | `ox check`, and as you type in oxmacs |
| Format | `indent-region`, one buffer at a time | `ox fmt`, one style for every file |
| Test | ERT | `ox test` |
| Profile | `M-x profiler-start` | `ox profile`, or `M-x ox-profiler-start` |
| Benchmark | `benchmark-run` | `ox perf bench` |
| Update old code | by hand | `ox migrate` |
| Run a script | `emacs --batch -l script.el` | `ox script.ox` |
| Other editors | — | `ox lsp` for Helix, Neovim and other LSP editors |
