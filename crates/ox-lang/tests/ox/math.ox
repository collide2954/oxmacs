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

;; ox/math: roots, powers, logarithms and angles
(module (import (ox math))) [(math/sqrt 16) (math/sqrt 2.25)] ;=> [4.0 1.5]
(module (import (ox math))) [(math/pow 2 10) (math/pow 2 100) (math/pow 2 -1) (math/pow 4 0.5) (math/pow 0 0)] ;=> [1024 1267650600228229401496703205376 0.5 2.0 1]
(module (import (ox math))) [(math/exp 0) (math/log math/e) (math/log10 1000) (math/log 1)] ;=> [1.0 1.0 3.0 0.0]
(module (import (ox math))) [(math/sin 0) (math/cos 0) (math/tan 0) (round (* 1000 (math/sin (/ math/pi 2))))] ;=> [0.0 1.0 0.0 1000]
(module (import (ox math))) (== (math/atan2 1 1) (/ math/pi 4)) ;=> true
(module (import (ox math))) [(> math/pi 3.14) (< math/pi 3.15) (> math/e 2.71) (< math/e 2.72)] ;=> [true true true true]
(module (import (ox math))) (math/sqrt -4) ;! math/sqrt can't take the square root of -4
(module (import (ox math))) (math/log 0) ;! math/log needs a number above 0, got 0
(module (import (ox math))) (math/log10 -1.5) ;! math/log10 needs a number above 0, got -1.5
(module (import (ox math))) (math/sqrt "x") ;! math/sqrt expects numbers, got "x"

;; clamp and lerp
(module (import (ox math))) [(math/clamp 5 0 3) (math/clamp -1 0 3) (math/clamp 2 0 3) (math/clamp 2.5 0 3)] ;=> [3 0 2 2.5]
(module (import (ox math))) (math/clamp 1 3 0) ;! math/clamp needs low at most high, got 3 and 0
(module (import (ox math))) [(math/lerp 0 10 0.5) (math/lerp 10 20 0) (math/lerp 0 4 1.5)] ;=> [5.0 10.0 6.0]

;; angles
(module (import (ox math))) [(math/degrees math/pi) (math/radians 180)] ;=> [180.0 3.141592653589793]
