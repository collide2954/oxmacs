;; vector literals evaluate their items
[] ;=> []
[1 (+ 1 1) 3] ;=> [1 2 3]
[[1 2] "a" :k nil \c] ;=> [[1 2] "a" :k nil \c]
(let [x 5] [x (* x x)]) ;=> [5 25]
(def (f & xs) xs) (f [1] [2]) ;=> ([1] [2])
[1 (undefined-thing)] ;! undefined-thing is not defined

;; vectors and lists are built counted and indexed
(vector 1 2 3) ;=> [1 2 3]
(vector) ;=> []
(list 1 2 3) ;=> (1 2 3)
(list) ;=> nil
(vector? [1]) ;=> true
(vector? (list 1)) ;=> false
(count [1 2 3]) ;=> 3
(count (list 1 2)) ;=> 2
(nth [:a :b :c] 2) ;=> :c
(nth (list :a :b :c) 1) ;=> :b
(nth [:a] 3) ;! Index 3 is out of range for a vector of 1 item
(nth (list :a :b) 2) ;! Index 2 is out of range for a list of 2 items
(nth 5 0) ;! nth expects a collection, got 5
(get [1 2 3] 1) ;=> 2
(get [1 2 3] 5) ;=> nil
(get [1 2 3] 5 :none) ;=> :none
(get [1 2 3] :a) ;=> nil
(get "abc" 2) ;=> \c
(get nil 0) ;=> nil
(vec (list 1 2)) ;=> [1 2]
(vec [1 2]) ;=> [1 2]
(vec nil) ;=> []
(vec "héllo") ;=> [\h \é \l \l \o]
(vec 5) ;! vec expects a collection, got 5

;; vectors and lists change without changing the original
(conj [1 2] 3) ;=> [1 2 3]
(conj [] 1 2 3) ;=> [1 2 3]
(conj (list 1 2) 0) ;=> (0 1 2)
(conj nil 1 2) ;=> (2 1)
(def v [1 2 3]) (conj v 4) v ;=> [1 2 3]
(conj 5 1) ;! conj expects a collection, got 5
(assoc [1 2 3] 0 :a) ;=> [:a 2 3]
(assoc [1 2 3] 3 4) ;=> [1 2 3 4]
(assoc [1 2 3] 0 :a 2 :c) ;=> [:a 2 :c]
(def v [1 2 3]) (assoc v 1 :b) v ;=> [1 2 3]
(assoc [1 2 3] 5 :x) ;! Index 5 is out of range for a vector of 3 items
(assoc [1 2 3] 0 :a 1) ;! assoc takes a collection and key value pairs
(assoc [1] :k 2) ;! assoc expects an integer index, got :k
(pop [1 2 3]) ;=> [1 2]
(pop (list 1 2 3)) ;=> (2 3)
(pop nil) ;=> nil
(pop []) ;! Cannot pop an empty vector
(peek [1 2 3]) ;=> 3
(peek (list 1 2 3)) ;=> 1
(peek []) ;=> nil
(peek 5) ;! peek expects a vector or list, got 5

;; map literals evaluate and print in insertion order
{} ;=> {}
{name: "Ana" age: (+ 1 2)} ;=> {name: "Ana" age: 3}
{:b 1 :a 2} ;=> {b: 1 a: 2}
{"a b" 1 42 [1 2] nil :k} ;=> {"a b" 1 42 [1 2] nil :k}
{a: {b: 1}} ;=> {a: {b: 1}}
(let [k :z] {k 1}) ;=> {z: 1}
{a: 1 b: 2 c: 3 d: 4 e: 5 f: 6 g: 7 h: 8 i: 9 j: 10} ;=> {a: 1 b: 2 c: 3 d: 4 e: 5 f: 6 g: 7 h: 8 i: 9 j: 10}
{a: 1 b: 2 a: 3} ;! Duplicate key :a in a map literal
{[1] 1 (list 1) 2} ;! Duplicate key (1) in a map literal

;; maps are read by key
(get {a: 1} :a) ;=> 1
(get {a: 1} :b) ;=> nil
(get {a: 1} :b 0) ;=> 0
(get {[1 2] :v} (list 1 2)) ;=> :v
(count {a: 1 b: 2}) ;=> 2
(map? {}) ;=> true
(map? []) ;=> false
(contains? {a: nil} :a) ;=> true
(contains? {a: 1} :b) ;=> false
(contains? nil :a) ;=> false
(contains? [1] 0) ;! contains? expects a map or set, got [1]
(keys {b: 1 a: 2}) ;=> [:b :a]
(vals {b: 1 a: 2}) ;=> [1 2]
(entries {b: 1 a: 2}) ;=> [[:b 1] [:a 2]]
(keys {}) ;=> []
(keys nil) ;=> []
(vals 5) ;! vals expects a map, got 5
(vec {a: 1}) ;=> [[:a 1]]

;; maps change without changing the original
(assoc {a: 1} :b 2) ;=> {a: 1 b: 2}
(assoc {a: 1 b: 2} :a 3) ;=> {a: 3 b: 2}
(assoc {} :a 1 :b 2) ;=> {a: 1 b: 2}
(assoc nil :a 1) ;=> {a: 1}
(def m {a: 1}) (assoc m :b 2) m ;=> {a: 1}
(assoc 5 :a 1) ;! assoc expects a map or vector, got 5
(dissoc {a: 1 b: 2 c: 3} :b) ;=> {a: 1 c: 3}
(dissoc {a: 1 b: 2 c: 3} :a :c :z) ;=> {b: 2}
(def m {a: 1}) (dissoc m :a) m ;=> {a: 1}
(dissoc nil :a) ;=> nil
(dissoc [1] 0) ;! dissoc expects a map, got [1]
(conj {a: 1} [:b 2]) ;=> {a: 1 b: 2}
(conj {a: 1} {b: 2 a: 3}) ;=> {a: 3 b: 2}
(conj {} 5) ;! conj on a map expects [key value] pairs or maps, got 5

;; set literals evaluate and print in insertion order
#{} ;=> #{}
#{3 1 2} ;=> #{3 1 2}
#{[1] :a "s"} ;=> #{[1] :a "s"}
(let [x 5] #{x (+ x 1)}) ;=> #{5 6}
#{1 2 1} ;! Duplicate item 1 in a set literal
#{[1] (list 1)} ;! Duplicate item (1) in a set literal

;; sets are built read and changed without changing the original
(set [1 2 1 3]) ;=> #{1 2 3}
(set (list :a :b)) ;=> #{:a :b}
(set nil) ;=> #{}
(set "aba") ;=> #{\a \b}
(set {a: 1}) ;=> #{[:a 1]}
(set 5) ;! set expects a collection, got 5
(set? #{}) ;=> true
(set? {}) ;=> false
(map? #{}) ;=> false
(count #{1 2}) ;=> 2
(contains? #{1 2} 2) ;=> true
(contains? #{1 2} 3) ;=> false
(contains? #{[1 2]} (list 1 2)) ;=> true
(get #{[1 2]} (list 1 2)) ;=> [1 2]
(get #{1 2} 3 :none) ;=> :none
(conj #{1} 2 1 3) ;=> #{1 2 3}
(def s #{1}) (conj s 2) s ;=> #{1}
(disj #{1 2 3} 2) ;=> #{1 3}
(disj #{1 2 3} 1 3 9) ;=> #{2}
(disj nil 1) ;=> nil
(disj [1] 1) ;! disj expects a set, got [1]
(vec #{2 1}) ;=> [2 1]

;; sequences are taken apart and built
(first [1 2 3]) ;=> 1
(first (list 1 2)) ;=> 1
(first nil) ;=> nil
(first []) ;=> nil
(first "abc") ;=> \a
(first {a: 1}) ;=> [:a 1]
(first #{5 6}) ;=> 5
(second [1 2 3]) ;=> 2
(last [1 2 3]) ;=> 3
(last nil) ;=> nil
(rest [1 2 3]) ;=> [2 3]
(rest (list 1 2 3)) ;=> (2 3)
(rest [1]) ;=> []
(rest nil) ;=> nil
(empty? []) ;=> true
(empty? nil) ;=> true
(empty? "") ;=> true
(empty? {a: 1}) ;=> false
(range 4) ;=> [0 1 2 3]
(range 2 5) ;=> [2 3 4]
(range 0 10 3) ;=> [0 3 6 9]
(range 3 0 -1) ;=> [3 2 1]
(range 0 1 0) ;! range needs a step that is not zero
(range :a) ;! range expects integers, got :a
(concat [1] (list 2) nil #{3}) ;=> [1 2 3]
(reverse (list 1 2 3)) ;=> [3 2 1]
(cons 0 (list 1 2)) ;=> (0 1 2)
(cons 0 [1 2]) ;=> (0 1 2)
(into [] (list 1 2)) ;=> [1 2]
(into #{} [1 1 2]) ;=> #{1 2}
(into {} [[:a 1]]) ;=> {a: 1}
(min 3 1 2) ;=> 1
(max 3 1 2.5) ;=> 3

;; map and record literals need a value for every key
{a: 1 b:} ;! This map has a key without a value
(defrecord P [x y]) #P{x: 1 y:} ;! This record literal has a field without a value
