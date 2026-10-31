(module
  (import (ox math) (screensaver noise :as noise))
  (export background star draw))

(def background "#020208")
(def stars 900)
(def tilt 0.45)
(def tints ["#cfe3ff" "#ffffff" "#bcd4ff" "#fff2d6" "#ffb3c7"])

(def (star i seconds)
  "Star i: on one of two arms, closer stars turning faster, as x y in the galaxy's plane."
  (let [out (math/pow (noise/of [i :out]) 1.6)
        arm (* math/pi (mod i 2))
        wound (+ arm (* 2.6 (math/log (+ 0.05 out))))
        scatter (* (- (noise/of [i :scatter]) 0.5) (if (< (noise/of [i :disk]) 0.35) (* 2 math/pi) (+ 0.18 (* 0.3 (- 1 out)))))
        turn (/ (* 0.18 seconds) (+ 0.25 out))
        angle (+ wound scatter turn)
        r (+ 0.02 out)]
    [(* r (math/cos angle)) (* r (math/sin angle)) out]))

(def (draw frame)
  (let [seconds (/ frame.time 1000.0)
        cx (/ frame.width 2.0)
        cy (/ frame.height 2.0)
        reach (* 0.46 (min frame.width (* 1.8 frame.height)))
        view (* 0.05 seconds)
        c (math/cos view)
        s (math/sin view)]
    (vec (concat
          (map (fn [k] [:circle cx cy (* reach (- 0.22 (* 0.035 k))) ["#ffe7b3" (+ 0.03 (* 0.05 k))]]) (range 6))
          (map (fn [i]
                 (let [[x y out] (star i seconds)
                       vx (- (* x c) (* y s))
                       vy (+ (* x s) (* y c))
                       tint (if (< out 0.08) "#ffe7b3" (nth tints (mod i (count tints))))]
                   [:circle (+ cx (* reach vx)) (+ cy (* reach tilt vy))
                    (+ 0.5 (* 1.1 (noise/of [i :size])))
                    [tint (+ 0.35 (* 0.6 (- 1 out)))]]))
               (range stars))))))

(test "inner stars go round faster than outer ones"
  (let [angle (fn [[x y]] (math/atan2 y x))
        turned (fn [i] (abs (- (angle (star i 1.0)) (angle (star i 1.1)))))
        inner (first (filter (fn [i] (< (nth (star i 0) 2) 0.1)) (range stars)))
        outer (first (filter (fn [i] (> (nth (star i 0) 2) 0.8)) (range stars)))]
    (expect (> (turned inner) (turned outer)))))
