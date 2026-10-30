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
(take 1 5) ;! take expects a collection, got 5
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
(flatten 5) ;! flatten expects a collection, got 5
[(distinct [1 1.0 1 "a" "a"]) (distinct "abca")] ;=> [[1 1.0 "a"] [\a \b \c]]
(frequencies "abca") ;=> {\a 2 \b 1 \c 1}
(frequencies [[1] [1] (list 1)]) ;=> {[1] 3}
[(zip [1 2]) (zip [1 2] nil) (interleave [1] [2] [3 4])] ;=> [[[1] [2]] [] [1 2 3]]
(zip) ;! map takes at least 2 arguments, got 1
(interleave) ;! map takes at least 2 arguments, got 1
(zip [1] 5) ;! zip expects a collection, got 5
[(repeat 0 :a) (repeat -2 :a) (repeat 2 nil)] ;=> [[] [] [nil nil]]
(repeat :a 1) ;! range expects integers, got :a

;; nested maps
[(get-in {a: nil} [:a :b] :d) (get-in {a: false} [:a :b]) (get-in [1 [2 3]] [1 0]) (get-in {a: 1} [] :d) (get-in nil [:a] :d :e)] ;=> [:d nil 2 {a: 1} :d]
(get-in {} 5) ;! get-in expects a collection, got 5
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
[@registered-tests (contains? plain-heads "and") (count plain-heads)] ;=> [[] true 12]
[(shown-plainly? 1) (shown-plainly? 'a) (shown-plainly? "s") (shown-plainly? 99999999999999999999) (shown-plainly? [1])] ;=> [true false true true false]
[(explain '(= (+ 1 1) 3) [2 3]) (explain '(f "a" x) ["a" 1]) (explain '(f) [])] ;=> ["(= (+ 1 1) 3)\n  (+ 1 1) => 2" "(f \"a\" x)\n  x => 1" "(f)"]
(explain 5 []) ;! rest expects a collection, got 5
(explain '(f a) 5) ;! explain expects a collection, got 5

;; functions that call the functions they are given
[complement comp partial juxt memoize swap! update every iterate] ;=> [#<native complement> #<native comp> #<native partial> #<native juxt> #<native memoize> #<native swap!> #<native update> #<native every> #<native iterate>]
[map filter remove reduce some every? keep sort-by mapcat take-while drop-while find group-by update-in merge-with] ;=> [#<native map> #<native filter> #<native remove> #<native reduce> #<native some> #<native every?> #<native keep> #<native sort-by> #<native mapcat> #<native take-while> #<native drop-while> #<native find> #<native group-by> #<native update-in> #<native merge-with>]
[(complement even?) (comp inc) (partial +) (juxt inc) (memoize inc) ((comp) 5) (comp)] ;=> [#<fn> #<fn> #<fn> #<fn> #<fn> 5 #<native identity>]
[((complement nil?) nil) ((comp str inc) 1) ((comp inc inc inc) 1) ((partial + 1 2) 3 4) ((juxt) 1) ((juxt first last) [1 2 3])] ;=> [false "2" 4 10 [] [1 3]]
((complement 5) 1) ;! 5 is not a function
((comp 5) 1) ;! 5 is not a function
((partial 5) 1) ;! 5 is not a function
(let [a (atom 1)] [(swap! a + 1 2) @a]) ;=> [4 4]
(swap! (atom 1) 5) ;! 5 is not a function
[(update {a: 1} :a inc) (update [1] 0 + 5) (update nil :a (fn [x] x))] ;=> [{a: 2} [6] {a: nil}]
(update 5 :a inc) ;! inc expects numbers, got nil
(update {} :a 5) ;! 5 is not a function
[(map inc []) (map vector [1 2] [3] [4 5]) (map inc nil) (map identity "ab") (map identity {a: 1})] ;=> [[] [[1 3 4]] [] [\a \b] [[:a 1]]]
(map inc 5) ;! map expects a collection, got 5
(map inc [1] 5) ;! map expects a collection, got 5
(map 5 [1]) ;! 5 is not a function
(map (fn [] 1) [1]) ;! fn takes 0 arguments, got 1
(map inc [:a]) ;! inc expects numbers, got :a
[(filter :a [{a: 1} {b: 2}]) (remove :a [{a: 1} {b: 2}]) (keep :a [{a: 1} {b: 2} {a: false}])] ;=> [[{a: 1}] [{b: 2}] [1 false]]
(filter even? 5) ;! filter expects a collection, got 5
(remove 5 [1]) ;! 5 is not a function
[(reduce + [1]) (reduce + 1 []) (reduce +) (reduce + 1 2 [3]) (reduce conj [] "ab")] ;=> [1 1 nil 4 [\a \b]]
(reduce + 5) ;! reduce expects a collection, got 5
(reduce 5 [1 2]) ;! 5 is not a function
(reduce + :x [1]) ;! + expects numbers, got :x
[(some even? []) (some :a [{} {a: false} {a: 2}]) (every? even? []) (every? identity [1 nil]) (find nil? [1 nil 2]) (find even? [1])] ;=> [nil 2 true false nil nil]
(every? 5 [1]) ;! 5 is not a function
[(sort-by - [3 1 2]) (sort-by count ["ccc" "a" "bb" "d"]) (sort-by identity [])] ;=> [[3 2 1] ["a" "d" "bb" "ccc"] []]
(sort-by identity [1 :a]) ;! sort can't compare [1 0] with [:a 1]
(sort-by 5 [1]) ;! 5 is not a function
[(mapcat (fn [x] [x x]) [1 2]) (mapcat list [1] [2]) (mapcat identity [])] ;=> [[1 1 2 2] [1 2] []]
(mapcat identity [1]) ;! concat expects a collection, got 1
[(take-while even? [2 4 5 6]) (take-while even? (gen (yield 2) (yield 3))) (drop-while even? [2 3 4]) (drop-while even? [2 4])] ;=> [[2 4] [2] [3 4] []]
(take-while 5 (gen (yield 1))) ;! 5 is not a function
(let [g (gen (yield 2) (yield 3) (yield 4))] [(take-while even? g) (next g)]) ;=> [[2] 4]
[(group-by even? [1 2 3]) (group-by :k [{k: 1} {k: 2} {k: 1}]) (group-by identity [])] ;=> [{false [1 3] true [2]} {1 [{k: 1} {k: 1}] 2 [{k: 2}]} {}]
[(update-in {a: {b: 1}} [:a :b] + 10) (update-in {} [:a] (fn [x] x)) (update-in nil [:a :b] (constantly 1))] ;=> [{a: {b: 11}} {a: nil} {a: {b: 1}}]
(update-in {a: 1} (gen (yield :a)) inc) ;! first expects a collection, got #<generator>
(update-in {a: 1} [:a] 5) ;! 5 is not a function
[(merge-with + {a: 1} nil {a: 2 b: 3}) (merge-with +) (merge-with + nil) (merge-with + {} {a: 1})] ;=> [{a: 3 b: 3} nil nil {a: 1}]
(merge-with + {a: 1} 5) ;! keys expects a map, got 5
(merge-with 5 {a: 1} {a: 2}) ;! 5 is not a function
(let [calls (atom 0) f (memoize (fn [& xs] (swap! calls inc) (count xs)))] [(f) (f) (f 1) (f 1) @calls]) ;=> [0 0 1 1 2]
(take 3 (iterate (fn [x] (* x 2)) 1)) ;=> [1 2 4]
(take 2 (iterate 5 1)) ;! 5 is not a function
(let [n (atom 0)] (await (every 1 (fn [] (swap! n inc) (when (= @n 3) :stop)))) @n) ;=> 3
(every :x inc) ;=> #<promise>
[(map + [1 2] (gen (yield 10) (yield 20))) (reduce + (gen (yield 1) (yield 2))) (reduce + 0 (gen (yield 1) (yield 2)))] ;=> [[11 22] 3 3]
[(some even? (gen (yield 1) (yield 2))) (sort-by - (gen (yield 1) (yield 2)))] ;=> [true [2 1]]

;; the core macros expand the same way
(-> 1 nil) ;! nil is not a function
(->> 1 [2] (vector 3)) ;! [2] is not a function
[(-> {a: 1} :a) (->> [1 2] (map inc)) (let [x 5] (-> x (- 1) (* 2)))] ;=> [1 [2 3] 8]
[(some-> 1) (some-> nil inc) (some-> {a: {b: nil}} :a :b inc) (as-> 1 x) (as-> 1 x [x x])] ;=> [1 nil nil 1 [1 1]]
[(cond [else 1 2]) (cond (true 3)) (cond [nil 1] [false 2])] ;=> [2 3 nil]
(cond 5) ;! first expects a collection, got 5
[(if-let [x 1] x) (if-let [x nil] x) (when-let [x false] 1)] ;=> [1 nil nil]
(if-let 5 1) ;! second expects a collection, got 5
(if-let [x nil] x 1 2) ;! if takes a test, a then form and an optional else form
(when) ;! when takes at least 1 argument, got 0
[(atomically 1 2) (await (spawn 3)) (vec (gen))] ;=> [2 3 []]
(maybe (raise 5)) ;! Raised 5
(let [and 1] (and true and)) ;! 1 is not a function
[(expect (= 1 1)) (expect (when true 1)) (expect 1)] ;=> [true true true]
[(try (expect (when false 1)) (catch expectation-failed e e.message)) (try (expect nil) (catch expectation-failed e e.message))] ;=> ["(when false 1)" "nil"]
[(try (expect (> 1 2 3)) (catch expectation-failed e e.message)) (try (expect ((fn [x] x) false)) (catch expectation-failed e e.message))] ;=> ["(> 1 2 3)" "((fn [x] x) false)"]
(let [m {on: true off: false}] [(expect m.on) (try (expect m.off) (catch expectation-failed _ :failed))]) ;=> [true :failed]
(do (test "x" 1) (test "y" (expect false)) [(map :name @registered-tests) (try ((:run (last @registered-tests))) (catch expectation-failed e e.message))]) ;=> [["x" "y"] "false"]
(defrules my-unless [] [(my-unless test body) (if test nil body)]) [(my-unless false 1) (my-unless true 1)] ;=> [1 nil]
(defrules swap-pair [] [(swap-pair a b) [b a]]) (swap-pair 1 2) ;=> [2 1]
(defmacro quoted [form] (list 'quote form)) (quoted (-> a b (c d))) ;=> (-> a b (c d))