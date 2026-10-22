;; atoms hold a value that can change
(atom 1) ;=> #<atom 1>
(deref (atom 1)) ;=> 1
@(atom [1 2]) ;=> [1 2]
(def a (atom 1)) (reset! a 5) ;=> 5
(def a (atom 1)) (reset! a 5) @a ;=> 5
(def a (atom 0)) (def (bump) (reset! a (+ @a 1))) (bump) (bump) @a ;=> 2
(atom? (atom nil)) ;=> true
(atom? 5) ;=> false
(= (atom 1) (atom 1)) ;=> false
(def a (atom 1)) (= a a) ;=> true
(deref 5) ;! deref expects an atom, got 5
(reset! 5 1) ;! reset! expects an atom, got 5
