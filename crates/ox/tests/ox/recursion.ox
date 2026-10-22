;; deep recursion runs on the vms own stack
(def (down n) (if (< n 1) 0 (+ 1 (down (- n 1))))) (down 100000) ;=> 100000

;; tail calls run in constant space
(def (count-down n) (if (< n 1) :done (count-down (- n 1)))) (count-down 500000) ;=> :done
(def (even? n) (if (< n 1) true (odd? (- n 1)))) (def (odd? n) (if (< n 1) false (even? (- n 1)))) (even? 500001) ;=> false
(def (sum n total) (if (< n 1) total (let [next (- n 1)] (sum next (+ total n))))) (sum 500000 0) ;=> 125000250000
(def (ends-native n) (if (< n 1) (+ 1 1) (ends-native (- n 1)))) (ends-native 10) ;=> 2
(def (grow n) (let [k n] (set! k (+ k 1)) (if (< k 500000) (grow k) k))) (grow 0) ;=> 500000

;; runaway recursion is a stack overflow
(def (forever n) (+ 1 (forever n))) (forever 0) ;! Stack overflow
