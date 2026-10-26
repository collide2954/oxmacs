;; the prelude maps filters and reduces
(map inc [1 2 3]) ;=> [2 3 4]
(map + [1 2 3] [10 20]) ;=> [11 22]
(map :a [{a: 1} {a: 2}]) ;=> [1 2]
(filter (fn [x] (> x 1)) [1 2 3]) ;=> [2 3]
(remove (fn [x] (> x 1)) (list 1 2 3)) ;=> [1]
(reduce + [1 2 3]) ;=> 6
(reduce + 10 [1 2 3]) ;=> 16
(reduce + []) ;=> 0
(reduce conj [] (list 1 2)) ;=> [1 2]
(some (fn [x] (> x 1)) [1 2 3]) ;=> true
(some #{3} [1 2 3]) ;=> 3
(some #{4} [1 2 3]) ;=> nil
(every? (fn [x] (> x 0)) [1 2]) ;=> true
(every? (fn [x] (> x 1)) [1 2]) ;=> false
(keep :a [{a: 1} {b: 2} {a: 3}]) ;=> [1 3]
(mapcat (fn [x] [x x]) [1 2]) ;=> [1 1 2 2]
(count (map inc (range 10000))) ;=> 10000

;; the prelude builds functions from functions
(identity 5) ;=> 5
((constantly 7) 1 2) ;=> 7
((comp inc inc) 1) ;=> 3
((comp str inc) 1) ;=> "2"
((comp) 5) ;=> 5
((partial + 1 2) 3 4) ;=> 10
((complement nil?) 1) ;=> true
(not nil) ;=> true
(not 0) ;=> false
(nil? nil) ;=> true
(nil? []) ;=> false
(some? false) ;=> true
(inc 1) ;=> 2
(dec 1) ;=> 0

;; swap and update come from the prelude
(def a (atom 1)) (swap! a + 10) ;=> 11
(def a (atom 1)) (swap! a + 10) @a ;=> 11
(def a (atom [1])) (swap! a conj 2 3) @a ;=> [1 2 3]
(update {n: 1} :n + 5) ;=> {n: 6}
(update [1 2 3] 0 -) ;=> [-1 2 3]
(defrecord P [x]) (update (P 1) :x * 3) ;=> #P{x: 3}
(update {} :n (fn [x] (if x x 0))) ;=> {n: 0}
(swap! 5 +) ;! deref expects an atom, got 5
