;; equality is exact and structural
(= 1 1) ;=> true
(= 1 1.0) ;=> false
(= 1.5 1.5) ;=> true
(= "abc" (str "a" "bc")) ;=> true
(= "abc" "abd") ;=> false
(= :a :a) ;=> true
(= \a \a) ;=> true
(= nil nil) ;=> true
(= nil false) ;=> false
(= 100000000000000000000 (* 10000000000 10000000000)) ;=> true
(= 1) ;=> true
(= 1 1 1) ;=> true
(= 1 1 2) ;=> false
(def (f & xs) xs) (= (f 1 "a" (f 2)) (f 1 "a" (f 2))) ;=> true
(def (f & xs) xs) (= (f 1 2) (f 1 2 3)) ;=> false
(= + +) ;=> true
(= (fn [] 1) (fn [] 1)) ;=> false
(not= 1 2) ;=> true
(not= 1 1) ;=> false

;; equality on floats is an equivalence
(def big (* 1e308 10)) (def x (- big big)) (= x x) ;=> true
(def big (* 1e308 10)) (def x (- big big)) (= x (- big big)) ;=> true
(def big (* 1e308 10)) (def x (- big big)) (== x x) ;=> false
(= 0.0 -0.0) ;=> false
(== 0.0 -0.0) ;=> true

;; identical compares references
(def s "abc") (identical? s s) ;=> true
(identical? (str "a") (str "a")) ;=> false
(identical? :a :a) ;=> true
(identical? 1 1) ;=> true
(identical? 1 1.0) ;=> false

;; lists and vectors with the same items are equal
(= [1 2] [1 2]) ;=> true
(= [1 2] [2 1]) ;=> false
(= [1 2] (list 1 2)) ;=> true
(= (list 1 2) [1 2]) ;=> true
(= [1 [2 3]] (list 1 (list 2 3))) ;=> true
(= [1 2] (list 1 2 3)) ;=> false
(= [1 2 3] (list 1 2)) ;=> false
(= [1 1.0] [1 1]) ;=> false
(= [] nil) ;=> true
(= nil []) ;=> true
(= [] (list)) ;=> true
(= [nil] []) ;=> false
(= [nil] nil) ;=> false
(= [] "") ;=> false
(not= [1] (list 1)) ;=> false

;; maps with the same entries are equal in any order
(= {a: 1 b: 2} {b: 2 a: 1}) ;=> true
(= {a: 1 b: 2 c: 3 d: 4 e: 5 f: 6 g: 7 h: 8 i: 9 j: 10} {j: 10 i: 9 h: 8 g: 7 f: 6 e: 5 d: 4 c: 3 b: 2 a: 1}) ;=> true
(= {a: 1} {a: 1 b: 2}) ;=> false
(= {a: 1 b: 2} {a: 1}) ;=> false
(= {a: 1} {a: 2}) ;=> false
(= {a: 1} {b: 1}) ;=> false
(= {a: [1 2]} {a: (list 1 2)}) ;=> true
(= {} {}) ;=> true
(= {} nil) ;=> false
(= {} []) ;=> false
(= (dissoc {a: 1 b: 2} :a) {b: 2}) ;=> true
(= (hash {a: 1 b: 2}) (hash {b: 2 a: 1})) ;=> true
(= (hash {a: 1 b: 2 c: 3 d: 4 e: 5 f: 6 g: 7 h: 8 i: 9 j: 10}) (hash {j: 10 i: 9 h: 8 g: 7 f: 6 e: 5 d: 4 c: 3 b: 2 a: 1})) ;=> true
(= (hash {a: 1}) (hash {a: 2})) ;=> false
(= (hash {a: 1 b: 2}) (hash [[:a 1] [:b 2]])) ;=> false
(get {{a: 1 b: 2} :found} {b: 2 a: 1}) ;=> :found

;; sets with the same items are equal in any order
(= #{1 2} #{2 1}) ;=> true
(= #{1 2} #{1 2 3}) ;=> false
(= #{1 2} #{1 3}) ;=> false
(= #{[1]} #{(list 1)}) ;=> true
(= #{} {}) ;=> false
(= #{} []) ;=> false
(= #{} nil) ;=> false
(= (hash #{1 2}) (hash #{2 1})) ;=> true
(= (hash #{1 2}) (hash [1 2])) ;=> false
(= (hash #{}) (hash {})) ;=> false
(get {#{1 2} :found} #{2 1}) ;=> :found

;; equal values hash alike
(= (hash [1 2]) (hash (list 1 2))) ;=> true
(= (hash [1 [2]]) (hash (list 1 (list 2)))) ;=> true
(= (hash []) (hash nil)) ;=> true
(= (hash "ab") (hash (str "a" "b"))) ;=> true
(= (hash 100000000000000000000) (hash (* 10000000000 10000000000))) ;=> true
(= (hash :a) (hash :a)) ;=> true
(= (hash 1) (hash 2)) ;=> false
(= (hash [1 2]) (hash [2 1])) ;=> false
(= (hash [1]) (hash 1)) ;=> false
(def big (* 1e308 10)) (= (hash (- big big)) (hash (- big big))) ;=> true
