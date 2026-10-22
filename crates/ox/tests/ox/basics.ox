;; constants evaluate to themselves
42 ;=> 42
-3.5 ;=> -3.5
"hi" ;=> "hi"
true ;=> true
nil ;=> nil
:key ;=> :key
\a ;=> \a

;; the last form gives the result
1 2 3 ;=> 3

;; read errors stop evaluation
1 (2 ;! This ( is never closed

;; if chooses a branch by truth
(if true 1 2) ;=> 1
(if false 1 2) ;=> 2
(if nil 1 2) ;=> 2
(if 0 1 2) ;=> 1
(if "" 1 2) ;=> 1
(if () 1 2) ;=> 2
(if false 1) ;=> nil
(if (< 1 2) (+ 1 1) (/ 1 0)) ;=> 2
(if (> 1 2) (/ 1 0) (* 3 3)) ;=> 9
(+ 1 (if true 10 20)) ;=> 11

;; malformed ifs are reported
(if) ;! if takes a test, a then form and an optional else form
(if 1) ;! if takes a test, a then form and an optional else form
(if 1 2 3 4) ;! if takes a test, a then form and an optional else form

;; do runs its forms in order and gives the last
(do 1 2 3) ;=> 3
(do) ;=> nil
(do (+ 1 1)) ;=> 2
(+ 1 (do 5 6)) ;=> 7

;; def names a global value
(def x 10) (+ x 1) ;=> 11
(def x 1) ;=> x
(def x 1) (def x 2) x ;=> 2
(def twice-ten (* 2 10)) twice-ten ;=> 20

;; malformed defs are reported
(def) ;! def takes a name and a value
(def x) ;! def takes a name and a value
(def 1 2) ;! def takes a name and a value
(def x 1 2) ;! def takes a name and a value

;; let binds names in order for its body
(let [x 1 y 2] (+ x y)) ;=> 3
(let [x 1 y (+ x 1)] (* x y)) ;=> 2
(let [x 1] (let [x (+ x 10)] x)) ;=> 11
(let [x 1] (let [x 5] x) x) ;=> 1
(def x 100) (let [x 1] x) ;=> 1
(def x 100) (let [y 1] x) ;=> 100
(let [] 5) ;=> 5
(let [x 2] 0 (+ x x)) ;=> 4
(def (f a) (let [b (* a 2)] (+ a b))) (f 3) ;=> 9
(+ 1 (let [x 2] (* x 10)) (let [y 3] y)) ;=> 24

;; malformed lets are reported
(let) ;! let takes a binding vector and a body
(let x 1) ;! let takes a binding vector and a body
(let [x] x) ;! let bindings come in name and value pairs
(let [1 2] 3) ;! Bindings must be names

;; set assigns locals captured variables and globals
(let [x 1] (set! x (+ x 1)) x) ;=> 2
(def (f x) (set! x 10) x) (f 1) ;=> 10
(let [x 1] (set! x 7)) ;=> 7
(def g 1) (set! g 5) g ;=> 5
(def (counter) (let [n 0] (fn [] (set! n (+ n 1)) n))) (def c (counter)) (c) (c) (c) ;=> 3
(let [n 0] (def inc! (fn [] (set! n (+ n 1)))) (def get (fn [] n))) (inc!) (inc!) (get) ;=> 2
(def (make) (let [n 0] (fn [] (set! n (+ n 1)) n))) (def a (make)) (def b (make)) (a) (a) (b) ;=> 1

;; malformed and undefined assignments are errors
(set! nowhere 1) ;! nowhere is not defined
(set!) ;! set! takes a name and a value
(set! x) ;! set! takes a name and a value
(set! 1 2) ;! set! takes a name and a value
