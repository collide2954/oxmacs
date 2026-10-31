(module
  (import (ox math) (ox string))
  (export background hex palette draw))

(def background "#000000")
(def tile 32)
(def digits (vec (map str (string/chars "0123456789abcdef"))))

(def (hex level)
  "level from 0 to 255 as two hex digits."
  (let [n (max 0 (min 255 (int level)))]
    (str (nth digits (int (/ n 16))) (nth digits (mod n 16)))))

(def palette
  (vec (map (fn [k]
              (let [angle (* 2 math/pi (/ k 64.0))]
                (str "#"
                     (hex (+ 128 (* 127 (math/sin angle))))
                     (hex (+ 128 (* 127 (math/sin (+ angle 2.1)))))
                     (hex (+ 128 (* 127 (math/sin (+ angle 4.2))))))))
            (range 64))))

(def (level x y seconds cx cy)
  (let [dx (- x cx) dy (- y cy)]
    (+ (math/sin (+ (* 0.011 x) seconds))
       (math/sin (+ (* 0.013 y) (* 0.7 seconds)))
       (math/sin (+ (* 0.007 (+ x y)) (* 1.3 seconds)))
       (math/sin (- (* 0.012 (math/sqrt (+ (* dx dx) (* dy dy)))) seconds)))))

(def (draw frame)
  (let [seconds (* 0.6 (/ frame.time 1000.0))
        columns (+ 1 (int (/ frame.width tile)))
        rows (+ 1 (int (/ frame.height tile)))
        cx (+ (/ frame.width 2.0) (* 0.3 frame.width (math/sin (* 0.2 seconds))))
        cy (/ frame.height 2.0)]
    (vec (map (fn [index]
                (let [x (* tile (mod index columns))
                      y (* tile (int (/ index columns)))
                      v (level (+ x 16) (+ y 16) seconds cx cy)
                      shade (int (mod (* 64 (+ 0.5 (/ v 8) (* 0.03 seconds))) 64))]
                  [:rect (+ x 1) (+ y 1) (- tile 2) (- tile 2) (nth palette shade)]))
              (range (* columns rows))))))

(test "colors are written as hex"
  (expect (= (hex 255) "ff"))
  (expect (= (hex 0) "00"))
  (expect (= (hex 171.4) "ab"))
  (expect (every? (fn [color] (= (count color) 7)) palette)))
