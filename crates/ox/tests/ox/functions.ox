;; functions are made with fn and called
((fn [x] (* x x)) 5) ;=> 25
(def square (fn [x] (* x x))) (square 7) ;=> 49
(def (add a b) (+ a b)) (add 2 3) ;=> 5
(def (answer) 42) (answer) ;=> 42
(def (g x) 1 2 (+ x 1)) (g 1) ;=> 2
((fn [x]) 1) ;=> nil
(def (inc x) (+ x 1)) (inc (inc (inc 0))) ;=> 3
(def x 100) (def (f x) x) (f 1) ;=> 1
(def (f a b) (- a b)) (+ 10 (f 5 (f 3 1))) ;=> 13

;; functions call themselves through their global name
(def (fact n) (if (< n 2) 1 (* n (fact (- n 1))))) (fact 10) ;=> 3628800

;; functions print with their names
(fn [x] x) ;=> #<fn>
(def (f) 1) f ;=> #<fn f>

;; calls with the wrong number of arguments are errors
((fn [x] x)) ;! fn takes 1 argument, got 0
(def (f a) a) (f 1 2) ;! f takes 1 argument, got 2

;; malformed functions are reported
(fn) ;! fn takes a parameter vector and a body
(fn x 1) ;! fn takes a parameter vector and a body
(fn [1] 1) ;! Parameters must be names
(def (1 x) x) ;! def takes a name and a value

;; closures capture the variables around them
(def (adder n) (fn [x] (+ x n))) ((adder 10) 5) ;=> 15
(def (f a) (fn [b] (fn [c] (+ a b c)))) (((f 1) 2) 3) ;=> 6
(let [x 5] ((fn [] (* x 2)))) ;=> 10
(let [y 2] (def h (fn [] y))) (h) ;=> 2
(def (f a) (let [b 2] (fn [] (fn [] (* a b))))) (((f 3))) ;=> 6

;; named functions can call themselves
((fn fact [n] (if (< n 2) 1 (* n (fact (- n 1))))) 10) ;=> 3628800
(fn named [x] x) ;=> #<fn named>
((fn count-down [n] (if (< n 1) :done (count-down (- n 1)))) 100000) ;=> :done
((fn outer [n] ((fn [] (if (< n 1) :done (outer (- n 1)))))) 5) ;=> :done
(let [f (fn me [] me)] (= (f) f)) ;=> true
((fn me [me] me) 5) ;=> 5
(fn me [] (set! me 1)) ;! set! cannot change a function's own name

;; optional parameters take their defaults
(def (f a (b 10)) (+ a b)) (f 1) ;=> 11
(def (f a (b 10)) (+ a b)) (f 1 2) ;=> 3
(def (f a (b (* a 2)) (c (+ b 1))) (+ a b c)) (f 1) ;=> 6
(def (f a (b (* a 2)) (c (+ b 1))) (+ a b c)) (f 1 5) ;=> 12
(def (f (x)) x) (f) ;=> nil

;; rest parameters collect the remaining arguments
(def (f a & more) more) (f 1 2 3) ;=> (2 3)
(def (f a & more) more) (f 1) ;=> nil
(def (f & all) all) (f) ;=> nil
(def (f (a 0) & more) more) (f 1 2) ;=> (2)

;; keyword parameters take names and defaults
(def (f x {scale 2 shift 0}) (+ (* x scale) shift)) (f 5) ;=> 10
(def (f x {scale 2 shift 0}) (+ (* x scale) shift)) (f 5 :shift 1) ;=> 11
(def (f x {scale 2 shift 0}) (+ (* x scale) shift)) (f 5 shift: 1 scale: 3) ;=> 16
(def (f (a 1) {b 2}) (+ a b)) (f :b 5) ;=> 6
(def (f (a 1) {b 2}) (+ a b)) (f 10 :b 5) ;=> 15
(def (f & xs {k 0}) k) (f 1 2 :k 9) ;=> 9
(def (f {k (+ 1 1)}) k) (f) ;=> 2
(def (g n {total 0}) (if (< n 1) total (g (- n 1) :total (+ total n)))) (g 1000) ;=> 500500

;; parameter mistakes are reported
(def (f a (b 1)) b) (f) ;! f takes 1 to 2 arguments, got 0
(def (f a (b 1)) b) (f 1 2 3) ;! f takes 1 to 2 arguments, got 3
(def (f a & r) r) (f) ;! f takes at least 1 argument, got 0
(def (f {k 1}) k) (f :j 2) ;! f has no keyword parameter :j
(def (f {k 1}) k) (f :k) ;! f was given :k without a value
(fn [(a 1) b] a) ;! Required parameters come before optional ones
(fn [& a b] a) ;! & takes one name for the rest of the arguments
(fn [{k 1} a] a) ;! Keyword parameters come last
(fn [(1 2)] 1) ;! Parameters must be names

;; apply spreads its last argument
(apply + [1 2 3]) ;=> 6
(apply + 1 2 [3 4]) ;=> 10
(apply + 1 (list 2)) ;=> 3
(apply + nil) ;=> 0
(apply vector 1 #{2}) ;=> [1 2]
(def (f a (b 2) & more) [a b more]) (apply f [1]) ;=> [1 2 nil]
(def (f a (b 2) & more) [a b more]) (apply f 1 [5 6 7]) ;=> [1 5 (6 7)]
(def (down n) (if (< n 1) :done (apply down [(- n 1)]))) (down 1000) ;=> :done
(apply :a [{a: 1}]) ;=> 1
(apply apply [+ [1 2]]) ;=> 3
(apply + 1) ;! apply expects a collection, got 1
(apply +) ;! apply takes at least 2 arguments, got 1
(apply 5 [1]) ;! 5 is not a function

;; keywords maps and sets can be called
(:a {a: 1}) ;=> 1
(:b {a: 1}) ;=> nil
(:b {a: 1} 0) ;=> 0
(:a nil) ;=> nil
(:a 5) ;=> nil
({a: 1} :a) ;=> 1
({a: 1} :b :none) ;=> :none
(#{:a :b} :a) ;=> :a
(#{:a :b} :c) ;=> nil
(#{[1]} (list 1)) ;=> [1]
(def (twice f x) (f (f x))) (twice :a {a: {a: 5}}) ;=> 5
(def (pick m) (:a m)) (pick {a: 7}) ;=> 7
(:a) ;! :a takes 1 to 2 arguments, got 0
({} 1 2 3) ;! {} takes 1 to 2 arguments, got 3
(#{} 1 2) ;! #{} takes 1 argument, got 2
([1 2] 0) ;! [1 2] is not a function
