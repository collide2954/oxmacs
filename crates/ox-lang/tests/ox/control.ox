;; when and unless
(when true 1 2) ;=> 2
(when false 1) ;=> nil
(unless false :yes) ;=> :yes
(unless true :yes) ;=> nil

;; and and or stop at the first answer
(and) ;=> true
(and 1 2 3) ;=> 3
(and 1 nil 3) ;=> nil
(and 1 false (/ 1 0)) ;=> false
(or) ;=> nil
(or nil false 3) ;=> 3
(or nil) ;=> nil
(or 1 (/ 1 0)) ;=> 1
(let [value 5] (or nil value)) ;=> 5
(let [value 5] (and true value)) ;=> 5

;; cond takes test and body clauses
(def (sign n) (cond [(< n 0) :negative] [(= n 0) :zero] [else :positive])) [(sign -2) (sign 0) (sign 3)] ;=> [:negative :zero :positive]
(cond [false 1]) ;=> nil
(cond [true 1 2]) ;=> 2
(cond) ;=> nil

;; threading
(-> 1 inc (* 10) (- 5)) ;=> 15
(->> [1 2 3] (map inc) (reduce +)) ;=> 9
(-> {a: {b: 2}} :a :b) ;=> 2

;; if-let and when-let
(if-let [x (get {a: 1} :a)] (+ x 1) :none) ;=> 2
(if-let [x (get {a: 1} :b)] (+ x 1) :none) ;=> :none
(when-let [x 5] (* x 2)) ;=> 10
(when-let [x nil] (* x 2)) ;=> nil
