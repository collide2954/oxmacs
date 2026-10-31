(module
  (import (ox math) (screensaver noise :as noise))
  (export background swell draw))

(def background "#1b1d3a")
(def step 6)
(def layers 7)
(def waters ["#2b3a67" "#27406f" "#22477a" "#1d4f86" "#185892" "#12629e" "#0c6cab"])

(def (swell layer x seconds height)
  "Where the water of a layer is at x: far swells low and small, near ones big."
  (let [near (/ layer (dec layers))
        base (* height (+ 0.5 (* 0.07 layer)))
        size (* height (+ 0.006 (* 0.022 near)))
        k (- 0.024 (* 0.014 near))
        pace (+ 0.6 (* 0.5 near))]
    (+ base
       (* size (math/sin (- (* k x) (* pace seconds) (* 1.7 layer))))
       (* 0.35 size (math/sin (- (* 2.3 k x) (* 1.7 pace seconds) layer))))))

(def (sky width height)
  (vec (map (fn [band]
              [:rect 0 (* band height 0.05) width (+ 1 (* height 0.05))
               [(nth ["#2b2350" "#4a2c5e" "#7a3b5e" "#b0515b" "#e2725b" "#f59e5b" "#f9c56b" "#fde3a7" "#fde3a7" "#fde3a7"] band) 1]])
            (range 10))))

(def (sun width height)
  (let [x (* 0.62 width) y (* 0.47 height)]
    [[:circle x y 70 ["#ffd59e" 0.12]] [:circle x y 44 ["#ffe2b0" 0.25]] [:circle x y 28 "#fff1cf"]]))

(def (water layer seconds width height)
  (vec (map (fn [x]
              [:line x (swell layer x seconds height) x height (nth waters layer) (inc step)])
            (range 0 (+ width step) step))))

(def (glints seconds width height)
  (let [sx (* 0.62 width)]
    (vec (keep (fn [i]
                 (let [layer (mod i layers)
                       x (+ sx (* width 0.18 (- (noise/of [i :x]) 0.5) (+ 1 layer)))
                       y (- (swell layer x seconds height) 1)
                       flicker (math/sin (+ (* seconds (+ 2 (* 3 (noise/of [i :rate])))) (* 9 (noise/of [i :phase]))))]
                   (when (> flicker 0.4)
                     [:line (- x 5) y (+ x 5) y ["#fff3d6" (* 0.9 (- flicker 0.4))] 1.5])))
               (range 160)))))

(def (draw frame)
  (let [seconds (/ frame.time 1000.0)
        w frame.width
        h frame.height]
    (vec (concat (sky w h)
                 (sun w h)
                 (mapcat (fn [layer] (water layer seconds w h)) (range layers))
                 (glints seconds w h)))))

(test "near swells are bigger than far ones"
  (let [range-of (fn [layer]
                   (let [ys (map (fn [x] (swell layer x 3 600)) (range 0 800 10))]
                     (- (reduce max ys) (reduce min ys))))]
    (expect (> (range-of 6) (range-of 0)))))
