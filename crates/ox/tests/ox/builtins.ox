;; the core library is built into Ox
[vec take profile] ;=> [#<native vec> #<native take> #<native profile>]
[identity not nil? some? constantly drop chunk partition flatten distinct frequencies zip interleave repeat] ;=> [#<native identity> #<native not> #<native nil?> #<native some?> #<native constantly> #<native drop> #<native chunk> #<native partition> #<native flatten> #<native distinct> #<native frequencies> #<native zip> #<native interleave> #<native repeat>]
[get-in assoc-in merge select-keys shown-plainly? explain profile-report] ;=> [#<native get-in> #<native assoc-in> #<native merge> #<native select-keys> #<native shown-plainly?> #<native explain> #<native profile-report>]

;; generators passed where collections go are read to the end first
(vec (gen (yield 1) (yield (vec (gen (yield 2)))))) ;=> [1 [2]]
(let [g (gen (yield 1))] (vec g) (vec g)) ;=> []
(def g (gen (vec g))) (vec g) ;! This generator is already running
(vec (gen (raise (error "inside")))) ;! inside
(chunk 2 (gen (yield 1) (yield 2) (yield 3))) ;=> [[1 2] [3]]
[(distinct (gen (yield 1) (yield 1) (yield 2))) (zip (gen (yield 1)) (gen (yield 2)))] ;=> [[1 2] [[1 2]]]

;; take and drop count like the rest of Ox
[(take -1 [1 2]) (take 2.5 [1 2 3 4]) (take 9 nil) (take 1 "ab") (take 1 {a: 1})] ;=> [[] [1 2 3] [] [\a] [[:a 1]]]
(take :x [1]) ;! min expects numbers, got :x
(take 1 5) ;! vec expects a collection, got 5
(take :x (gen (yield 1))) ;! < expects numbers, got :x
(let [g (gen (yield 1) (yield 2) (yield 3))] [(take 2 g) (next g)]) ;=> [[1 2] 3]
(take 5 (gen (yield 1))) ;=> [1]
[(drop 9 [1]) (drop 0 nil) (drop 1 "abc")] ;=> [[] [] [\b \c]]
(drop -1 []) ;! Index -1 is out of range for a vector of 0 items
(drop -1 [1 2]) ;! Index -1 is out of range for a vector of 2 items
(drop 1.5 [1 2 3]) ;! nth expects an integer index, got 1.5
(drop :x [1]) ;! min expects numbers, got :x

;; groups and flattening
[(chunk 3 []) (chunk 0 []) (partition 2 [1 2 3]) (partition 2.0 [1 2])] ;=> [[] [] [[1 2]] []]
(chunk 2.0 [1 2 3]) ;! nth expects an integer index, got 2.0
(chunk -1 [1 2]) ;! Index -1 is out of range for a vector of 2 items
(chunk :a [1]) ;! + expects numbers, got :a
(chunk 0 [1]) ;! chunk expects a size other than 0, got 0
(partition 0.0 [1]) ;! partition expects a size other than 0, got 0.0
(flatten [1 nil [2 (list 3 nil)] #{4} {a: 1} "s"]) ;=> [1 2 3 #{4} {a: 1} "s"]
(flatten 5) ;! vec expects a collection, got 5
[(distinct [1 1.0 1 "a" "a"]) (distinct "abca")] ;=> [[1 1.0 "a"] [\a \b \c]]
(frequencies "abca") ;=> {\a 2 \b 1 \c 1}
(frequencies [[1] [1] (list 1)]) ;=> {[1] 3}
[(zip [1 2]) (zip [1 2] nil) (interleave [1] [2] [3 4])] ;=> [[[1] [2]] [] [1 2 3]]
(zip) ;! map takes at least 2 arguments, got 1
(interleave) ;! map takes at least 2 arguments, got 1
(zip [1] 5) ;! vec expects a collection, got 5
[(repeat 0 :a) (repeat -2 :a) (repeat 2 nil)] ;=> [[] [] [nil nil]]
(repeat :a 1) ;! range expects integers, got :a

;; nested maps
[(get-in {a: nil} [:a :b] :d) (get-in {a: false} [:a :b]) (get-in [1 [2 3]] [1 0]) (get-in {a: 1} [] :d) (get-in nil [:a] :d :e)] ;=> [:d nil 2 {a: 1} :d]
(get-in {} 5) ;! vec expects a collection, got 5
[(merge) (merge nil nil) (merge {} nil) (merge {a: 1} {a: nil})] ;=> [nil nil {} {a: nil}]
(merge 5) ;! keys expects a map, got 5
(merge {a: 1} [1 2]) ;! keys expects a map, got [1 2]
[(select-keys 5 []) (select-keys #{:a :b} [:a :c]) (select-keys {a: 1} (list :a))] ;=> [{} {a: :a} {a: 1}]
(select-keys 5 [:a]) ;! contains? expects a map or set, got 5
[(assoc-in [1 2] [0] 9) (assoc-in {} [] 1) (assoc-in nil [:a :b] 1)] ;=> [[9 2] {nil 1} {a: {b: 1}}]
(assoc-in {a: 5} [:a :b] 1) ;! assoc expects a map or vector, got 5
(assoc-in 5 [:a] 1) ;! assoc expects a map or vector, got 5
(assoc-in {} 5 1) ;! first expects a collection, got 5
(assoc-in {a: [1]} [:a 3] 1) ;! Index 3 is out of range for a vector of 1 item

;; small functions, error types and what expect uses
[(constantly 1) ((constantly 1) 2 3) (identity 4) (not false) (nil? false) (some? nil)] ;=> [#<fn> 1 4 true false false]
[(error) (error "m") (type-error "t" 5) (expectation-failed)] ;=> [#error{message: "error" traceback: nil} #error{message: "m" traceback: nil} #type-error{message: "t" traceback: 5} #expectation-failed{message: "expectation-failed" traceback: nil}]
(error 1 2 3) ;! error takes 0 to 2 arguments, got 3
[@registered-tests (contains? plain-heads "and") (count plain-heads)] ;=> [[] true 11]
[(shown-plainly? 1) (shown-plainly? 'a) (shown-plainly? "s") (shown-plainly? 99999999999999999999) (shown-plainly? [1])] ;=> [true false true true false]
[(explain '(= (+ 1 1) 3) [2 3]) (explain '(f "a" x) ["a" 1]) (explain '(f) [])] ;=> ["(= (+ 1 1) 3)\n  (+ 1 1) => 2" "(f \"a\" x)\n  x => 1" "(f)"]
(explain 5 []) ;! rest expects a collection, got 5
(explain '(f a) 5) ;! vec expects a collection, got 5
