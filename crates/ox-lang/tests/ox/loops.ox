;; loop names values and again goes round with new ones
(loop [i 0 total 0] (if (< i 10) (again (+ i 1) (+ total i)) total)) ;=> 45
(loop [] 7) ;=> 7
(loop [x 1 y (+ x 1)] [x y]) ;=> [1 2]
(loop [n 100000] (if (> n 0) (again (dec n)) :done)) ;=> :done
(loop [i 0] (cond [(< i 3) (again (inc i))] [else i])) ;=> 3
(loop [i 0] (let [j (* i 2)] (if (< j 10) (again (inc i)) j))) ;=> 10
(loop [i 0] (match i [3 :three] [_ (again (inc i))])) ;=> :three
(def (count-down n) (loop [n n seen []] (if (zero? n) seen (again (dec n) (conj seen n))))) (count-down 3) ;=> [3 2 1]
(loop [i 0 rows []] (if (< i 2) (again (inc i) (conj rows (loop [j 0 row []] (if (< j 2) (again (inc j) (conj row [i j])) row)))) rows)) ;=> [[[0 0] [0 1]] [[1 0] [1 1]]]

;; each time round has its own names, even ones set! changes
(loop [i 0 fs []] (if (< i 3) (again (inc i) (conj fs (fn [] i))) (map (fn [f] (f)) fs))) ;=> [0 1 2]
(loop [i 0 fs []] (if (< i 3) (do (set! i (* i 10)) (again (inc (// i 10)) (conj fs (fn [] i)))) (map (fn [f] (f)) fs))) ;=> [0 10 20]

;; again only ends a loop
(loop [i 0] (+ 1 (again i))) ;! again must be the last thing its loop does
(loop [i 0] (try (again 1) (catch error e e))) ;! again must be the last thing its loop does
(again 1) ;! again only works at the end of a loop
(loop [i 0] (fn [] (again 1))) ;! again only works at the end of a loop
(loop [i 0] (again)) ;! again takes 1 value, one for each name of its loop
(loop [a 1 b 2] (again 1)) ;! again takes 2 values, one for each name of its loop
(loop [i] i) ;! loop bindings come in name and value pairs
(loop [[a b] [1 2]] a) ;! loop names must be plain names; destructure inside the loop with let

;; each runs its body for every item and returns nil
(def seen (atom [])) (each [x [1 2 3]] (swap! seen conj (* x 10))) @seen ;=> [10 20 30]
(each [x [1 2]] x) ;=> nil
(def seen (atom [])) (each [x [1 2] y [:a :b]] (swap! seen conj [x y])) @seen ;=> [[1 :a] [1 :b] [2 :a] [2 :b]]
(def seen (atom [])) (each [x (range 6) :when (even? x) :let [y (* x x)]] (swap! seen conj y)) @seen ;=> [0 4 16]
(def seen (atom [])) (each [[k v] {a: 1}] (swap! seen conj [k v])) @seen ;=> [[:a 1]]
(vec (gen (each [x [1 2 3]] (yield (* x 2))))) ;=> [2 4 6]

;; for builds a vector
(for [x [1 2 3]] (* x x)) ;=> [1 4 9]
(for [x [1 2] y [:a :b]] [x y]) ;=> [[1 :a] [1 :b] [2 :a] [2 :b]]
(for [day [{high: 31} {high: 25} {high: 35}] :when (> day.high 30) :let [f (+ 32 (* day.high 2))]] f) ;=> [94 102]
(for [x []] x) ;=> []
(for [x [1 2]]) ;=> [nil nil]

;; while and until repeat on changing state
(def n (atom 0)) (while (< @n 5) (swap! n inc)) @n ;=> 5
(def n (atom 0)) (until (>= @n 3) (swap! n inc)) @n ;=> 3
(while false 1) ;=> nil

;; case picks the clause holding the value
(def (name n) (case n [1 :one] [(2 3) :few] [else :many])) [(name 1) (name 3) (name 9)] ;=> [:one :few :many]
(case :b [:a 1] [:b 2 3]) ;=> 3
(case "x" ["x" :found]) ;=> :found
(case 5 [1 :one]) ;=> nil
(case 'red [red :stop] [green :go]) ;=> :stop
[(case nil [nil :none] [else :some]) (case 1 [nil :none] [else :some])] ;=> [:none :some]
(case 1 (1 :one)) ;=> :one
(case 1 :one) ;! case clauses are [value body...] vectors
(each x [1 2]) ;! each takes a binding vector and a body
(for [x [1] :while true] x) ;! for takes :when and :let, not :while
(for [x] x) ;! for bindings come in name and collection pairs
(let [calls (atom 0)] (case (do (swap! calls inc) 2) [1 :a] [2 :b] [3 :c]) @calls) ;=> 1
