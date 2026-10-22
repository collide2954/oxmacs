;; rounding gives whole numbers
[(round 2.5) (round -2.5) (round 2.4) (round 7)] ;=> [3 -3 2 7]
[(floor 2.7) (floor -2.2) (ceil 2.2) (ceil -2.7) (int 2.9) (int -2.9)] ;=> [2 -3 3 -2 2 -2]
(round 1e30) ;=> 1000000000000000019884624838656
(float 3) ;=> 3.0
(let [inf (* 1e308 10.0)] (round (- inf inf))) ;! round can't make nan whole
(round (* 1e308 10.0)) ;! round can't make inf whole
(round "x") ;! round expects numbers, got "x"

;; mod follows the divisor, rem the dividend
[(mod 7 3) (mod -7 3) (mod 7 -3) (rem 7 3) (rem -7 3)] ;=> [1 2 -2 1 -1]
[(mod 7.5 2) (mod 99999999999999999999 7)] ;=> [1.5 1]
(mod 1 0) ;! Division by zero

;; signs and parity
[(abs -3) (abs 2.5) (abs -99999999999999999999)] ;=> [3 2.5 99999999999999999999]
[(zero? 0) (zero? 0.0) (pos? 1) (neg? -1) (pos? 0)] ;=> [true true true true false]
[(even? 4) (odd? 3) (even? -2) (odd? 0)] ;=> [true true true false]
(even? 1.5) ;! even? expects a whole number, got 1.5
