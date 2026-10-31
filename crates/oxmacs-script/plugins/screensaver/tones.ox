(module
  (import (ox string))
  (export hex rgb blend))

(def digits (vec (map str (string/chars "0123456789abcdef"))))

(def (hex level)
  "level from 0 to 255 as two hex digits."
  (let [n (max 0 (min 255 (int level)))]
    (str (nth digits (int (/ n 16))) (nth digits (mod n 16)))))

(def (rgb [r g b])
  "A color from red, green and blue parts between 0 and 1, as \"#rrggbb\"."
  (str "#" (hex (* 255 r)) (hex (* 255 g)) (hex (* 255 b))))

(def (blend [r1 g1 b1] [r2 g2 b2] t)
  "The color a fraction t of the way from the first to the second."
  [(+ r1 (* t (- r2 r1))) (+ g1 (* t (- g2 g1))) (+ b1 (* t (- b2 b1)))])

(test "colors become hex text"
  (expect (= (rgb [1 0 0.5]) "#ff007f"))
  (expect (= (blend [0 0 0] [1 1 1] 0.5) [0.5 0.5 0.5])))
