;; arithmetic works on integers and floats
(+ 1 2) ;=> 3
(+ 1 2.5) ;=> 3.5
(+) ;=> 0
(*) ;=> 1
(- 5) ;=> -5
(- 10 1 2) ;=> 7
(* 2 3 4) ;=> 24
(/ 7 2) ;=> 3.5
(/ 6 3) ;=> 2.0
(// 7 2) ;=> 3
(// -7 2) ;=> -4
(// 7 -2) ;=> -4
(// 7.5 2) ;=> 3.0
(+ 1 (* 2 3)) ;=> 7
(- (* 2 3) 1) ;=> 5
(+ (* 2 (+ 1 1)) (- 10 (* 2 3))) ;=> 8

;; arithmetic mistakes are errors
(/ 1 0) ;! Division by zero
(// 1 0) ;! Division by zero
(+ 1 "a") ;! + expects numbers, got "a"
(1 2) ;! 1 is not a function
(nope 1) ;! nope is not defined
(<) ;! < takes at least 1 argument, got 0

;; comparisons chain across their arguments
(< 1 2 3) ;=> true
(< 1 3 2) ;=> false
(<= 1 1 2) ;=> true
(>= 2 2 1) ;=> true
(> 2.5 1) ;=> true
(> 1 2.5) ;=> false

;; numeric equality compares numbers by value
(== 1 1.0) ;=> true
(== 1 1.0 1) ;=> true
(== 1 2) ;=> false
(== 9223372036854775808 9223372036854775808.0) ;=> true
(== "a" 1) ;! == expects numbers, got "a"

;; integers grow into bignums and shrink back
(+ 9223372036854775807 1) ;=> 9223372036854775808
(- -9223372036854775808 1) ;=> -9223372036854775809
(- -9223372036854775808) ;=> 9223372036854775808
(* 99999999999 99999999999 99999999999) ;=> 999999999970000000000299999999999
123456789012345678901234567890 ;=> 123456789012345678901234567890
(+ 123456789012345678901234567890 1) ;=> 123456789012345678901234567891
(- (+ 9223372036854775807 1) 1) ;=> 9223372036854775807
(// 100000000000000000000 3) ;=> 33333333333333333333
(// -100000000000000000000 3) ;=> -33333333333333333334
(/ 100000000000000000000 4) ;=> 2.5e19
(< 9223372036854775807 9223372036854775808) ;=> true
(> 100000000000000000000 1.5) ;=> true

;; two-argument arithmetic in a loop keeps every meaning of the general functions
(def (sum-to n) ((fn walk [i acc] (if (< i n) (walk (+ i 1) (+ acc i)) acc)) 0 0)) (sum-to 1000) ;=> 499500
(def (grow x) (* x x)) (grow 4294967296) ;=> 18446744073709551616
(def (halves x) (- x 0.5)) (halves 2) ;=> 1.5
(def (same? a b) (= a b)) [(same? 1 1) (same? :a :a) (same? [1] [1]) (same? 1 2)] ;=> [true true true false]
(def (third v) (nth v 2)) [(third [1 2 3]) (third "abc")] ;=> [3 \c]
(def (third v) (nth v 2)) (third [1]) ;! Index 2 is out of range for a vector of 1 item
(def (plus-one x) (+ x 1)) (plus-one "a") ;! + expects numbers, got "a"
(let [x 1] (+ x (do (set! x 5) x))) ;=> 6
(let [+ (fn [a b] (str a b))] (+ 1 2)) ;=> "12"
(def (adds a b) (+ a b)) (def before (adds 1 2)) (def (+ a b) (* a b)) [before (adds 3 4)] ;=> [3 12]
