(module
  (import (ox math))
  (export background depth draw))

(def background "#02020a")
(def rings 26)
(def sides 36)
(def speed 0.22)
(def colors ["#ff6ec7" "#7d5fff" "#4cc9f0" "#4ef0b5" "#ffd166"])

(def (depth i seconds)
  "How far ring i is, from 1 at the far end to 0 as it passes you."
  (let [z (mod (- (/ i rings) (* speed seconds)) 1)]
    (if (= z 0) 1 z)))

(def (middle z seconds cx cy)
  [(+ cx (* 0.18 cx (math/sin (+ (* 0.5 seconds) (* 3 z)))))
   (+ cy (* 0.18 cy (math/cos (+ (* 0.4 seconds) (* 2.5 z)))))])

(def (corners i seconds cx cy reach)
  (let [z (depth i seconds)
        [mx my] (middle z seconds cx cy)
        radius (/ reach (+ 0.08 (* 4 z)))
        twist (+ (* 0.3 seconds) (* 2 z))]
    (vec (map (fn [k]
                (let [angle (+ twist (/ (* 2 math/pi k) sides))]
                  [(+ mx (* radius (math/cos angle))) (+ my (* radius (math/sin angle)))]))
              (range (inc sides))))))

(def (ring i seconds cx cy reach)
  (let [z (depth i seconds)
        points (corners i seconds cx cy reach)
        color (nth colors (mod i (count colors)))
        alpha (* 0.9 (- 1 z))]
    (map (fn [k]
           (let [[x1 y1] (nth points k)
                 [x2 y2] (nth points (inc k))]
             [:line x1 y1 x2 y2 [color alpha] (+ 0.6 (* 2.5 (- 1 z)))]))
         (range sides))))

(def (draw frame)
  (let [seconds (/ frame.time 1000.0)
        cx (/ frame.width 2.0)
        cy (/ frame.height 2.0)
        reach (* 0.5 (max frame.width frame.height))
        order (sort-by (fn [i] (- (depth i seconds))) (range rings))]
    (vec (mapcat (fn [i] (ring i seconds cx cy reach)) order))))

(test "rings come toward you and start again far away"
  (expect (< (depth 10 1.0) (depth 10 0.5)))
  (expect (every? (fn [i] (< 0 (depth i 7.3) 1.0001)) (range rings))))
