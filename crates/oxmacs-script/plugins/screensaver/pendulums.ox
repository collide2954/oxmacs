(module
  (import (ox math) (screensaver tones :as tones))
  (export swing shades sphere draw))

(def count-of 16)
(def cycle 60.0)
(def least 30)
(def colors [[0.85 0.20 0.22] [0.92 0.45 0.15] [0.95 0.75 0.20] [0.55 0.78 0.22] [0.20 0.68 0.45]
             [0.15 0.60 0.80] [0.25 0.40 0.85] [0.50 0.30 0.80]])

(def (swing k seconds)
  "How far pendulum k is from the middle, from -1 to 1: it swings least + k times each cycle, so they fall in and out of step."
  (math/cos (* 2 math/pi (+ least k) (/ seconds cycle))))

(def (shades color)
  "The layers of a glossy ball of color, from its dark rim to the side facing the light."
  (let [dark (tones/blend color [0 0 0] 0.65)
        light (tones/blend color [1 1 1] 0.35)]
    (vec (map (fn [i] (tones/rgb (tones/blend dark light (/ i 6.0)))) (range 7)))))

(def shaded (vec (map shades colors)))

(def (sphere x y r layers alpha)
  "A glossy ball lit from the upper left: a dark rim, lighter layers toward the light and a bright spot."
  (vec (concat (map (fn [i]
                      (let [t (/ i 6.0)
                            shift (* r 0.32 t)]
                        [:circle (- x shift) (- y shift) (* r (- 1 (* 0.8 t))) [(nth layers i) alpha]]))
                    (range 7))
               [[:glow (- x (* 0.38 r)) (- y (* 0.38 r)) (* 0.6 r) ["#ffffff" (* 0.85 alpha)]]
                [:circle (- x (* 0.38 r)) (- y (* 0.38 r)) (* 0.12 r) ["#ffffff" (* 0.9 alpha)]]])))

(def (draw frame)
  (let [seconds (/ frame.time 1000.0)
        cx (/ frame.width 2.0)
        top (* 0.08 frame.height)
        reach (* 0.30 frame.width)
        gap (/ (* 0.84 frame.height) count-of)
        r (* 0.44 gap)]
    (vec (concat
          [[:line cx (* 0.02 frame.height) cx (+ top (* gap count-of)) ["#d0d4e0" 0.10] 3]]
          (mapcat (fn [k]
                    (let [y (+ top (* gap (+ k 0.5)))
                          x (+ cx (* reach (swing k seconds)))]
                      [[:glow (+ x (* 0.5 r)) (+ y (* 0.9 r)) (* 2.2 r) ["#000000" 0.55]]
                       [:line cx y x y ["#c8ccd8" 0.22] 1]]))
                  (range count-of))
          (mapcat (fn [k]
                    (let [y (+ top (* gap (+ k 0.5)))
                          x (+ cx (* reach (swing k seconds)))]
                      (sphere x y r (nth shaded (mod k (count shaded))) 1.0)))
                  (range count-of))))))

(test "the pendulums all line up again after a cycle"
  (expect (every? (fn [k] (< (abs (- (swing k 0) (swing k cycle))) 0.0001)) (range count-of)))
  (expect (some (fn [k] (> (abs (- (swing k 7) (swing 0 7))) 0.5)) (range count-of))))
