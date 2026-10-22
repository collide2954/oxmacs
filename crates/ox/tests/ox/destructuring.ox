;; let destructures vectors, maps and records
(let [[a b] [1 2]] (+ a b)) ;=> 3
(let [[a b] [1]] b) ;=> nil
(let [[a b] [1 2 3]] b) ;=> 2
(let [[a b & more] [1 2 3 4]] more) ;=> [3 4]
(let [[a [b c]] [1 [2 3]]] (+ a b c)) ;=> 6
(let [[a b] (list 1 2)] [b a]) ;=> [2 1]
(let [[_ b] [1 2]] b) ;=> 2
(let [[x y] [1 2] z (+ x y)] z) ;=> 3
(let [{name age} {name: "Ana" age: 3}] [name age]) ;=> ["Ana" 3]
(let [{name: n} {name: "Bo"}] n) ;=> "Bo"
(let [{missing} {}] missing) ;=> nil
(defrecord Point [x y]) (let [#Point{x y} (Point 1 2)] (* x y)) ;=> 2
(defmacro m [] :macro) (let [[m] [5]] m) ;=> 5
(let [[a 1] [1 1]] a) ;! let patterns bind names; use match to test values

;; fn parameters destructure vectors and records
((fn [[a b]] (+ a b)) [1 2]) ;=> 3
(defrecord Point [x y]) (def (dist #Point{x y}) (+ x y)) (dist (Point 3 4)) ;=> 7
((fn [a [b c] & more] [a b c more]) 1 [2 3] 4) ;=> [1 2 3 (4)]
(def (first-two [a b & _]) [a b]) (first-two [1 2 3]) ;=> [1 2]
(def (f [a b]) (set! a 10) (+ a b)) (f [1 2]) ;=> 12
