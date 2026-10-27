;; sort orders numbers, text, keywords, booleans and sequences
(sort [3 1 2]) ;=> [1 2 3]
(sort [1 2.5 -1]) ;=> [-1 1 2.5]
(sort ["b" "A" "a"]) ;=> ["A" "a" "b"]
(sort [:b :a]) ;=> [:a :b]
(sort [true false]) ;=> [false true]
(sort [[1 "b"] [1 "a"] [0 "z"] [1]]) ;=> [[0 "z"] [1] [1 "a"] [1 "b"]]
(sort '(2 1)) ;=> [1 2]
(sort [1 nil]) ;=> [nil 1]
(sort []) ;=> []
(sort [1 "a"]) ;! sort can't compare 1 with "a"
[(compare 1 2) (compare "b" "a") (compare :a :a)] ;=> [-1 1 0]

;; sort-by sorts by a key and keeps ties in order
(sort-by count ["ccc" "a" "bb"]) ;=> ["a" "bb" "ccc"]
(sort-by (fn [m] (- m.age)) [{age: 1} {age: 3}]) ;=> [{age: 3} {age: 1}]
(sort-by :n [{n: 1 k: :a} {n: 0 k: :b} {n: 1 k: :c}]) ;=> [{n: 0 k: :b} {n: 1 k: :a} {n: 1 k: :c}]

;; local times and abbreviated paths
(module (import (ox time))) (vec (keys (time/local 0))) ;=> [:year :month :day :hour :minute :second :weekday]
(module (import (ox time))) (>= (get (time/local (time/now)) :year) 2026) ;=> true
(module (import (ox path))) (= (path/abbreviate (path/join (path/home) "x")) (str "~" (path/separator) "x")) ;=> true
(module (import (ox path))) [(path/abbreviate (path/home)) (contains? #{"/" "\\"} (path/separator))] ;=> ["~" true]
(module (import (ox path))) [(path/expand "~bob/x") (= (path/expand "~") (path/home))] ;=> ["~bob/x" true]
(module (import (ox path))) (= (path/normalize (path/join "a" "b" ".." "." "c")) (path/join "a" "c")) ;=> true
(module (import (ox path))) (= (path/relative (path/join (path/home) "work" "a.ox") (path/home)) (path/join "work" "a.ox")) ;=> true
(module (import (ox path))) (= (path/relative (path/join (path/home) "x") (path/join (path/home) "y")) (path/join ".." "x")) ;=> true
(module (import (ox path))) [(path/inside? (path/join (path/home) "a" "b") (path/home)) (path/inside? (path/join (path/home) "ab") (path/join (path/home) "a"))] ;=> [true false]
