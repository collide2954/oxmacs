(module
  (import (ox math) (screensaver noise :as noise) (screensaver outline :as outline))
  (export background runner draw))

(def background "#0b0d16")
(def lights ["#ffb347" "#ff6f61" "#6ec6ff" "#ffd56b" "#c39bd3" "#7fffd4"])

(def (city seconds width height)
  (vec (map (fn [i]
              (let [pulse (+ 0.75 (* 0.25 (math/sin (+ (* 0.3 seconds) (* 7 (noise/of [i :pulse]))))))]
                [:circle (* width (noise/of [i :x])) (* height (+ 0.35 (* 0.65 (noise/of [i :y]))))
                 (+ 30 (* 70 (noise/of [i :size])))
                 [(nth lights (mod i (count lights))) (* 0.1 pulse)]]))
            (range 34))))

(def (beads seconds width height)
  (vec (keep (fn [i]
               (let [life (+ 6 (* 10 (noise/of [i :life])))
                     shifted (+ seconds (* life (noise/of [i :born])))
                     round (int (/ shifted life))
                     age (/ (- shifted (* round life)) life)]
                 (when (< age 0.85)
                   (let [x (* width (noise/of [i round :x]))
                         y (* height (noise/of [i round :y]))
                         r (+ 1.2 (* 2.8 (noise/of [i round :r])))]
                     [:circle x y r ["#cfe6ff" (* 0.35 (math/sin (* math/pi (/ age 0.85))))]]))))
             (range 220))))

(def (runner i seconds width height)
  "Running drop i: where it started, where it is now and how big it is."
  (let [span (+ 5 (* 6 (noise/of [i :span])))
        shifted (+ seconds (* span (noise/of [i :born])))
        round (int (/ shifted span))
        age (/ (- shifted (* round span)) span)
        x0 (* width (noise/of [i round :x]))
        y0 (* height 0.3 (noise/of [i round :y]))
        fall (* (- (* 1.15 height) y0) (- age (* 0.06 (math/sin (* 14 age)))))
        wander (fn [y] (+ x0 (* 7 (math/sin (+ (* 0.03 y) (* 5 (noise/of [i round :w])))))))]
    {start: y0 y: (+ y0 (max 0 fall)) wander: wander size: (+ 3 (* 3 (noise/of [i round :size])))}))

(def (trail r)
  (let [steps 16
        points (vec (map (fn [k]
                           (let [y (+ r.start (* (- r.y r.start) (/ k steps)))]
                             [((get r :wander) y) y]))
                         (range (inc steps))))
        [x y] (last points)]
    (vec (concat (outline/path points ["#d7ecff" 0.18] (* 0.7 r.size))
                 [[:circle x y r.size ["#e6f3ff" 0.45]]
                  [:circle (- x (* 0.3 r.size)) (- y (* 0.3 r.size)) (* 0.35 r.size) ["#ffffff" 0.8]]]))))

(def (draw frame)
  (let [seconds (/ frame.time 1000.0)]
    (vec (concat (city seconds frame.width frame.height)
                 (beads seconds frame.width frame.height)
                 (mapcat (fn [i] (trail (runner i seconds frame.width frame.height))) (range 14))))))

(test "running drops run down the glass"
  (let [a (runner 2 1.0 800 600)
        b (runner 2 1.5 800 600)]
    (expect (or (>= b.y a.y) (< b.y 200)))))
