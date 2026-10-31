(module
  (import (ox math) (screensaver noise :as noise))
  (export background shard draw))

(def background "#07030d")
(def shards 16)
(def colors ["#ff4d6d" "#ffd23f" "#3bceac" "#0ead69" "#4361ee" "#f15bb5" "#fee440" "#00bbf9"])

(def (shard i seconds)
  "Shard i inside one wedge of the mirror, as a distance, an angle, a size and a color."
  {reach: (+ 0.08 (* 0.9 (+ 0.5 (* 0.5 (math/sin (+ (* seconds (+ 0.07 (* 0.1 (noise/of [i :r])))) (* 9 (noise/of [i :rp]))))))))
   angle: (* (/ math/pi 6) (+ 0.5 (* 0.5 (math/sin (+ (* seconds (+ 0.09 (* 0.12 (noise/of [i :a])))) (* 9 (noise/of [i :ap])))))))
   size: (+ 0.02 (* 0.06 (noise/of [i :size])))
   color: (nth colors (mod i (count colors)))})

(def (mirrored s turn)
  (mapcat (fn [k]
            (let [base (+ turn (* k (/ math/pi 3)))]
              [(+ base s.angle) (- base s.angle)]))
          (range 6)))

(def (draw frame)
  (let [seconds (/ frame.time 1000.0)
        cx (/ frame.width 2.0)
        cy (/ frame.height 2.0)
        reach (* 0.5 (min frame.width frame.height))
        turn (* 0.05 seconds)
        found (map (fn [i] (shard i seconds)) (range shards))]
    (vec (mapcat (fn [s]
                   (let [r (* reach s.reach)
                         size (* reach s.size)]
                     (mapcat (fn [angle]
                               (let [x (+ cx (* r (math/cos angle)))
                                     y (+ cy (* r (math/sin angle)))
                                     inner (* 0.55 r)]
                                 [[:line (+ cx (* inner (math/cos angle))) (+ cy (* inner (math/sin angle))) x y
                                   [s.color 0.35] (* 0.5 size)]
                                  [:circle x y size [s.color 0.55]]]))
                             (mirrored s turn))))
                 found))))

(test "every shard shows twelve times"
  (expect (= (count (mirrored (shard 3 1.0) 0)) 12))
  (expect (= (count (draw {time: 1000 width: 800 height: 600 font-size: 14})) (* shards 24))))
