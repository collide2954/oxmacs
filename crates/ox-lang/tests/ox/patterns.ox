;; match tries its clauses in order
(match 1 [1 :one] [2 :two]) ;=> :one
(match 2 [1 :one] [_ :other]) ;=> :other
(match :b [:a 1] [:b 2]) ;=> 2
(match "x" ["x" :found]) ;=> :found
(match nil [nil :nothing] [_ :something]) ;=> :nothing
(match 5 [x (* x 2)]) ;=> 10
(match 1 [x 1 2 x]) ;=> 1
(match 3 [1 :one]) ;! No match for 3
(match 'a ['a :symbol-a] [_ :other]) ;=> :symbol-a

;; vector patterns match vectors and lists
(match [1 2] [[a b] (+ a b)]) ;=> 3
(match (list 1 2) [[a b] (+ a b)]) ;=> 3
(match [1 2 3] [[a b] :two] [[a b c] :three]) ;=> :three
(match [1 2 3 4] [[a & more] more]) ;=> [2 3 4]
(match [1 [2 3]] [[a [b c]] (+ a b c)]) ;=> 6
(match [] [[] :empty]) ;=> :empty
(match 5 [[a] :vector] [_ :other]) ;=> :other

;; map patterns test keys and bind values
(match {status: 200 body: "ok"} [{status: 200 body: b} b] [_ :error]) ;=> "ok"
(match {status: 404} [{status: 200 body: b} b] [{status: s} s]) ;=> 404
(match {name: "Ana" age: 3} [{name age} [name age]]) ;=> ["Ana" 3]
(match {name: "Ana"} [{name age} :both] [{name} name]) ;=> "Ana"
(match {"a b" 1} [{"a b" x} x]) ;=> 1
(match [1] [{a} a] [_ :not-a-map]) ;=> :not-a-map

;; record patterns check the type
(defrecord Point [x y]) (match (Point 1 2) [#Point{x y} (+ x y)]) ;=> 3
(defrecord Point [x y]) (match (Point 1 2) [#Point{x: 0} :on-axis] [#Point{x: px} px]) ;=> 1
(defrecord Point [x y]) (defrecord Pair [x y]) (match (Pair 1 2) [#Point{x} :point] [#Pair{x} :pair]) ;=> :pair
(defrecord Point [x y]) (match {x: 1 y: 2} [#Point{x} :point] [{x} :map]) ;=> :map

;; guards run after the pattern binds
(match 15 [n :when (> n 10) :big] [n :small]) ;=> :big
(match 5 [n :when (> n 10) :big] [n :small]) ;=> :small
(match {age: 20} [{age} :when (> age 17) :adult] [_ :minor]) ;=> :adult

;; matched names follow the usual scope rules
(let [x 1] (match 2 [x x])) ;=> 2
(def (classify v) (match v [[a b] (list a b)] [n n])) [(classify [1 2]) (classify 3)] ;=> [(1 2) 3]
((match 1 [x (fn [] x)])) ;=> 1
(defmacro m [] :macro) (match 5 [m m]) ;=> 5

;; mistakes in match are reported
(match 1) ;! No match for 1
(match) ;! match takes a value and clauses
(match 1 (1 :one)) ;! match clauses are [pattern body…] vectors
