;; generators yield values one at a time
(def g (gen (yield 1) (yield 2))) [(next g) (next g) (next g) (done? g)] ;=> [1 2 nil true]
(let [g (gen (yield 1))] [(done? g) (next g) (done? g) (next g) (done? g)]) ;=> [false 1 false nil true]
(let [g (gen (yield nil))] [(next g) (done? g) (next g) (done? g)]) ;=> [nil false nil true]
(let [g (gen 1 2 3)] [(next g) (done? g)]) ;=> [nil true]
(let [n 3 g (gen (yield (* n 2)))] (next g)) ;=> 6
(next (generator (fn [] (yield 5)))) ;=> 5
(gen 1) ;=> #<generator>
[(generator? (gen 1)) (generator? (fn [] 1))] ;=> [true false]

;; generators work as sequences
(def (count-up n) (gen ((fn step [i] (when (< i n) (yield i) (step (+ i 1)))) 0))) (vec (count-up 4)) ;=> [0 1 2 3]
(map inc (gen (yield 1) (yield 2))) ;=> [2 3]
(filter (fn [x] (> x 1)) (gen (yield 1) (yield 2) (yield 3))) ;=> [2 3]
(reduce + (gen (yield 1) (yield 2) (yield 3))) ;=> 6
(vec (gen)) ;=> []

;; yield works from helper functions, and is a value like any function
(def (each items f) (reduce (fn [_ x] (f x)) nil items)) (vec (gen (each [1 2 3] yield))) ;=> [1 2 3]
(def (twice x) (yield x) (yield x)) (vec (gen (twice :a) (twice :b))) ;=> [:a :a :b :b]

;; generators nest
(def (inner) (gen (yield :a) (yield :b))) (vec (gen (yield 1) (let [i (inner)] (yield (next i)) (yield (next i))) (yield 2))) ;=> [1 :a :b 2]
(def (each items f) (reduce (fn [_ x] (f x)) nil items)) (def (numbers) (gen (yield 1) (yield 2))) (def (doubled) (gen (each (numbers) (fn [x] (yield (* x 2)))))) (vec (doubled)) ;=> [2 4]

;; errors inside a generator reach whoever called next
(let [g (gen (raise (error "boom")))] (try (next g) (catch error e e.message))) ;=> "boom"
(let [g (gen (/ 1 0))] (try (next g) (catch error e nil)) [(done? g) (next g)]) ;=> [true nil]
(let [g (gen (try (yield 1) (/ 1 0) (catch error e (yield :caught))))] [(next g) (next g)]) ;=> [1 :caught]
(def log (atom [])) (let [g (gen (try (yield 1) (finally (swap! log conj :cleanup))))] (next g) (next g) @log) ;=> [:cleanup]

;; mistakes are reported
(yield 1) ;! yield is only allowed inside a generator
(next 5) ;! next expects a generator, got 5
(def g (gen (next g))) (next g) ;! This generator is already running
(let [gen (fn [] :mine)] (gen)) ;=> :mine
