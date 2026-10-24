;; the core library is built into Ox
[vec take profile] ;=> [#<native vec> #<native take> #<native profile>]

;; generators passed where collections go are read to the end first
(vec (gen (yield 1) (yield (vec (gen (yield 2)))))) ;=> [1 [2]]
(let [g (gen (yield 1))] (vec g) (vec g)) ;=> []
(def g (gen (vec g))) (vec g) ;! This generator is already running
(vec (gen (raise (error "inside")))) ;! inside

;; take counts like the rest of Ox
[(take -1 [1 2]) (take 2.5 [1 2 3 4]) (take 9 nil) (take 1 "ab") (take 1 {a: 1})] ;=> [[] [1 2 3] [] [\a] [[:a 1]]]
(take :x [1]) ;! min expects numbers, got :x
(take 1 5) ;! vec expects a collection, got 5
(take :x (gen (yield 1))) ;! < expects numbers, got :x
(let [g (gen (yield 1) (yield 2) (yield 3))] [(take 2 g) (next g)]) ;=> [[1 2] 3]
(take 5 (gen (yield 1))) ;=> [1]
