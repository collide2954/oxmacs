(module
  (import (ox math) (screensaver noise :as noise))
  (export background heading draw))

(def background "#05060a")
(def streams 90)
(def length 22)
(def stride 9.0)
(def life 9.0)
(def palette ["#7fdbff" "#b19cd9" "#ffadad" "#9bf6ff" "#caffbf"])

(def (heading x y seconds)
  "Which way the current runs at x y."
  (* math/pi (+ (math/sin (+ (* 0.0045 x) (* 0.06 seconds)))
                (math/cos (- (* 0.0052 y) (* 0.045 seconds)))
                (* 0.6 (math/sin (+ (* 0.0021 (+ x y)) (* 0.03 seconds)))))))

(def (stream i seconds width height)
  (let [shifted (+ seconds (* life (noise/of [i :born])))
        round (int (/ shifted life))
        age (/ (- shifted (* round life)) life)
        fade (math/sin (* math/pi age))
        head (- (mod (+ (* 0.35 seconds) (noise/of [i :head])) 1.4) 0.2)
        color (nth palette (mod i (count palette)))]
    (loop [k 0
           x (* width (noise/of [i round :x]))
           y (* height (noise/of [i round :y]))
           out []]
      (if (= k length)
        out
        (let [angle (heading x y seconds)
              nx (+ x (* stride (math/cos angle)))
              ny (+ y (* stride (math/sin angle)))
              pulse (+ 0.12 (* 0.88 (max 0 (- 1 (* 4 (abs (- (/ k length) head)))))))]
          (again (inc k) nx ny (conj out [:line x y nx ny [color (* 0.7 fade pulse)] 1.4])))))))

(def (draw frame)
  (let [seconds (/ frame.time 1000.0)]
    (vec (mapcat (fn [i] (stream i seconds frame.width frame.height)) (range streams)))))

(test "the currents turn slowly"
  (let [a (heading 300 200 10)
        b (heading 300 200 10.1)]
    (expect (< (abs (- a b)) 0.05))))

(test "every stream draws its whole length"
  (expect (= (count (draw {time: 4000 width: 960 height: 540 font-size: 14}))
             (* streams length))))
