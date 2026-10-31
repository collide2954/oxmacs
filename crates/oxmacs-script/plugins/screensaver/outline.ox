(module
  (import (ox math))
  (export path ellipse))

(def (path points paint width)
  "Lines joining points, a vector of [x y], one after another."
  (vec (map (fn [i]
              (let [[x1 y1] (nth points i)
                    [x2 y2] (nth points (inc i))]
                [:line x1 y1 x2 y2 paint width]))
            (range (max 0 (dec (count points)))))))

(def (ellipse cx cy rx ry paint width segments)
  "The outline of an ellipse around cx cy, in segments lines."
  (path (vec (map (fn [i]
                    (let [angle (/ (* 2 math/pi i) segments)]
                      [(+ cx (* rx (math/cos angle))) (+ cy (* ry (math/sin angle)))]))
                  (range (inc segments))))
        paint width))

(test "paths join their points in order"
  (expect (= (path [[0 0] [1 2] [3 4]] :foreground 1)
             [[:line 0 0 1 2 :foreground 1] [:line 1 2 3 4 :foreground 1]]))
  (expect (= (path [[0 0]] :foreground 1) [])))

(test "ellipses close on themselves"
  (let [lines (ellipse 10 10 4 2 :foreground 1 8)
        [_ x1 y1] (first lines)
        [_ _ _ x2 y2] (last lines)]
    (expect (= (count lines) 8))
    (expect (< (abs (- x1 x2)) 0.001))
    (expect (< (abs (- y1 y2)) 0.001))))
