;; defmacro rewrites code before it compiles
(defmacro m [] 1) ;=> m
(defmacro unless [test & body] `(if ~test nil (do ~@body))) (unless false 1 2) ;=> 2
(defmacro unless [test & body] `(if ~test nil (do ~@body))) (unless true 1) ;=> nil
(defmacro twice [form] `(do ~form ~form)) (def n (atom 0)) (twice (swap! n inc)) @n ;=> 2
(defmacro my-if [c a b] (list 'if c a b)) (my-if true :a :b) ;=> :a
(defmacro infix [a op b] (list op a b)) (infix 1 + 2) ;=> 3
(defmacro swap-args [f a b] (list f b a)) (swap-args - 1 10) ;=> 9
(defmacro m [x] x) (def (f) (m 5)) (f) ;=> 5
(defmacro m [x] `[~x ~x]) (m (+ 1 2)) ;=> [3 3]
(defmacro is-else [x] (if (= x 'else) :yes :no)) (is-else else) ;=> :yes
(defmacro shadowed [] 1) (let [shadowed (fn [] 2)] (shadowed)) ;=> 2

;; mistakes in macros are reported
(defmacro forever [] '(forever)) (forever) ;! Macro expansion went too deep in forever
(defmacro bad [] (atom 1)) (bad) ;! bad produced #<atom 1>, which is not code
(defmacro m [] (+ 1 :a)) (m) ;! + expects numbers, got :a

;; macros cannot capture user names or be captured by them
(defmacro my-or [a b] `(let [tmp ~a] (if tmp tmp ~b))) (let [tmp 5] (my-or false tmp)) ;=> 5
(defmacro swap [a b] `(let [tmp ~a] (set! ~a ~b) (set! ~b tmp))) (let [tmp 1 other 2] (swap tmp other) [tmp other]) ;=> [2 1]
(def (helper) :global) (defmacro call-helper [] '(helper)) (let [helper (fn [] :local)] (call-helper)) ;=> :global
(defmacro with-x [body] `(let [x 10] ~body)) (let [x 1] (with-x x)) ;=> 1
(defmacro with-x [body] `(let [x 10] (+ x ~body))) (with-x 5) ;=> 15
(defmacro adder [n] `(fn [x] (+ x ~n))) (let [x 100] ((adder x) 1)) ;=> 101
