(module
  (import (ox math) (screensaver noise :as noise))
  (export background blink draw))

(def background "#03070a")
(def flies 60)
(def cycle 40.0)
(def rate 2.6)

(def (blink i seconds)
  "How brightly firefly i glows: each starts out of step and falls into step with the rest."
  (let [along (mod seconds cycle)
        apart (math/exp (/ (- along) 7.0))
        phase (+ (* rate seconds) (* 2 math/pi apart (noise/of [i (int (/ seconds cycle)) :phase])))
        wave (mod phase (* 2 math/pi))]
    (max 0 (- 1 (* 2.2 (min wave (- (* 2 math/pi) wave)))))))

(def (place i seconds width height)
  [(* width (+ 0.5 (* 0.45 (math/sin (+ (* seconds (+ 0.05 (* 0.06 (noise/of [i :fx])))) (* 9 (noise/of [i :px])))))))
   (* height (+ 0.55 (* 0.3 (math/sin (+ (* seconds (+ 0.04 (* 0.05 (noise/of [i :fy])))) (* 9 (noise/of [i :py])))))))])

(def (grass seconds width height)
  (vec (map (fn [i]
              (let [x (* width (noise/of [i :x]))
                    tall (* height (+ 0.06 (* 0.12 (noise/of [i :tall]))))
                    lean (* 6 (math/sin (+ (* 0.8 seconds) (* 0.01 x))))]
                [:line x height (+ x lean) (- height tall) ["#0f2a1c" 0.95] 2]))
            (range 220))))

(def (draw frame)
  (let [seconds (/ frame.time 1000.0)]
    (vec (concat
          (mapcat (fn [i]
                    (let [[x y] (place i seconds frame.width frame.height)
                          glow (blink i seconds)]
                      (if (> glow 0.02)
                        [[:circle x y 16 ["#c6ff6b" (* 0.12 glow)]]
                         [:circle x y 6 ["#e4ff9e" (* 0.35 glow)]]
                         [:circle x y 2 ["#fbffe0" glow]]]
                        [[:circle x y 1.2 ["#3a4a2a" 0.6]]])))
                  (range flies))
          (grass seconds frame.width frame.height)))))

(test "fireflies fall into step"
  (let [spread (fn [seconds]
                 (let [glows (map (fn [i] (blink i seconds)) (range flies))]
                   (- (reduce max glows) (reduce min glows))))]
    (expect (> (reduce + (map (fn [k] (spread (+ 1 (* 0.1 k)))) (range 20)))
               (reduce + (map (fn [k] (spread (+ 35 (* 0.1 k)))) (range 20)))))))
