;; take and drop work from the front, and take reads generators lazily
[(take 2 [1 2 3]) (take 5 [1 2]) (take 0 [1]) (drop 1 [1 2 3]) (drop 5 [1 2])] ;=> [[1 2] [1 2] [] [2 3] []]
[(take-while (fn [x] (< x 3)) [1 2 3 1]) (drop-while (fn [x] (< x 3)) [1 2 3 1])] ;=> [[1 2] [3 1]]
(take 3 (iterate inc 5)) ;=> [5 6 7]
(take-while (fn [x] (< x 4)) (iterate inc 1)) ;=> [1 2 3]
(take 2 (list :a :b :c)) ;=> [:a :b]

;; grouping, pairing and flattening
[(partition 2 [1 2 3 4 5]) (chunk 2 [1 2 3 4 5])] ;=> [[[1 2] [3 4]] [[1 2] [3 4] [5]]]
(flatten [1 [2 [3 (list 4)]] [] 5]) ;=> [1 2 3 4 5]
[(find even? [1 3 4 6]) (find even? [1 3])] ;=> [4 nil]
(frequencies [:a :b :a]) ;=> {a: 2 b: 1}
(group-by even? [1 2 3 4]) ;=> {false [1 3] true [2 4]}
[(interleave [1 2 3] [:a :b]) (zip [1 2] [:a :b :c])] ;=> [[1 :a 2 :b] [[1 :a] [2 :b]]]
(repeat 3 :x) ;=> [:x :x :x]

;; nested maps
(get-in {a: {b: [10 20]}} [:a :b 1]) ;=> 20
[(get-in {a: 1} [:x :y]) (get-in {a: 1} [:x] :none)] ;=> [nil :none]
(assoc-in {a: {b: 1}} [:a :c] 2) ;=> {a: {b: 1 c: 2}}
(assoc-in {} [:a :b] 1) ;=> {a: {b: 1}}
(update-in {a: {n: 1}} [:a :n] + 10) ;=> {a: {n: 11}}
[(merge {a: 1} {b: 2} nil {a: 3}) (merge-with + {a: 1} {a: 2 b: 5}) (select-keys {a: 1 b: 2 c: 3} [:c :a :z])] ;=> [{a: 3 b: 2} {a: 3 b: 5} {c: 3 a: 1}]

;; functions made from functions
((juxt inc dec) 5) ;=> [6 4]
(def calls (atom 0)) (def double (memoize (fn [x] (swap! calls inc) (* x 2)))) [(double 2) (double 2) (double 3) @calls] ;=> [4 4 6 2]

;; names, keywords, symbols and types
[(keyword "name") (keyword :done) (keyword 'go) (name :done) (name 'sym) (name "text") (symbol "go")] ;=> [:name :done :go "done" "sym" "text" go]
(keyword 5) ;! keyword expects a string, symbol or keyword, got 5
[(type 1) (type 1.5) (type "s") (type :k) (type 'x) (type [1]) (type {}) (type #{}) (type (list 1)) (type nil) (type true) (type \c) (type inc) (type +) (type (atom 1))] ;=> [:int :float :string :keyword :symbol :vector :map :set :list :nil :bool :char :fn :fn :atom]
(defrecord Point [x y]) (type (Point 1 2)) ;=> :Point

;; threading that names the value or stops at nil
[(some-> {a: {b: 1}} :a :b) (some-> {a: nil} :a :b) (some-> {a: 2} :a (+ 10)) (as-> 1 x (+ x 1) (* x 3))] ;=> [1 nil 12 6]
