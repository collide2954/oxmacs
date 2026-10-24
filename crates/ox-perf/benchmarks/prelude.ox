(def n 100000)
(def numbers (vec (range n)))
(def small (vec (range 100)))
(def words (map (fn [i] (str "w" (mod i 97))) numbers))
(def nested {a: {b: {c: 1}}})
(def left {a: 1 b: 2 c: 3})
(def right {c: 4 d: 5})

(def (repeated work) (fn [] ((fn step [i] (when (< i n) (work) (step (+ i 1)))) 0)))

{map: [n (fn [] (map inc numbers))]
 filter: [n (fn [] (filter even? numbers))]
 remove: [n (fn [] (remove even? numbers))]
 reduce: [n (fn [] (reduce + 0 numbers))]
 some: [n (fn [] (some (fn [x] (= x -1)) numbers))]
 every: [n (fn [] (every? number? numbers))]
 keep: [n (fn [] (keep (fn [x] (when (even? x) x)) numbers))]
 distinct: [n (fn [] (distinct words))]
 sort-by: [n (fn [] (sort-by - numbers))]
 frequencies: [n (fn [] (frequencies words))]
 group-by: [n (fn [] (group-by (fn [x] (mod x 10)) numbers))]
 take: [n (fn [] (take n numbers))]
 flatten: [n (fn [] (flatten (chunk 10 numbers)))]
 zip: [n (fn [] (zip numbers numbers))]
 not: [n (repeated (fn [] (not nil)))]
 get-in: [n (repeated (fn [] (get-in nested [:a :b :c])))]
 assoc-in: [n (repeated (fn [] (assoc-in nested [:a :b :c] 2)))]
 merge: [n (repeated (fn [] (merge left right)))]
 swap: [n (let [box (atom 0)] (repeated (fn [] (swap! box inc))))]
 comp: [n (let [twice (comp inc inc)] (repeated (fn [] (twice 1))))]
 partial: [n (let [add (partial + 1)] (repeated (fn [] (add 1))))]
 thread: [n (repeated (fn [] (-> 1 inc (+ 2) (* 3))))]
 cond: [n (repeated (fn [] (cond ((= 1 2) :a) ((= 1 3) :b) (else :c))))]
 and-or: [n (repeated (fn [] (or (and 1 nil) (and 2 3))))]
 small-map: [n (repeated (fn [] (map inc small)))]}
