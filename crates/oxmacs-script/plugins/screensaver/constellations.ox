(module
  (import (ox math) (screensaver noise :as noise))
  (export background bounce draw))

(def background "#060814")
(def stars 70)

(def (bounce at size)
  "at folded back and forth into 0 to size, as if bouncing off the edges."
  (let [folded (mod at (* 2 size))]
    (if (> folded size) (- (* 2 size) folded) folded)))

(def (star i seconds width height)
  [(bounce (+ (* width (noise/of [i :x])) (* 24 (- (noise/of [i :vx]) 0.5) seconds)) width)
   (bounce (+ (* height (noise/of [i :y])) (* 24 (- (noise/of [i :vy]) 0.5) seconds)) height)])

(def (links found reach)
  (let [near (* reach reach)]
    (mapcat (fn [i]
              (let [[x1 y1] (nth found i)]
                (keep (fn [j]
                        (let [[x2 y2] (nth found j)
                              dx (- x2 x1) dy (- y2 y1)
                              apart (+ (* dx dx) (* dy dy))]
                          (when (< apart near)
                            [:line x1 y1 x2 y2 ["#9db4ff" (* 0.55 (- 1 (/ (math/sqrt apart) reach)))] 1])))
                      (range (inc i) stars))))
            (range stars))))

(def (draw frame)
  (let [seconds (/ frame.time 1000.0)
        found (vec (map (fn [i] (star i seconds frame.width frame.height)) (range stars)))]
    (vec (concat (links found (* 0.2 (min frame.width frame.height)))
                 (map (fn [[x y]] [:circle x y 1.9 ["#e3e9ff" 0.9]]) found)))))

(test "drifting stars stay on screen"
  (expect (every? (fn [at] (<= 0 (bounce at 300) 300)) (range -1000 1000 37))))
