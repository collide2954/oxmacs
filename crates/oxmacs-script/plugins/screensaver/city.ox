(module
  (import (ox math) (screensaver noise :as noise))
  (export background skyline lit? draw))

(def background "#070b1a")
(def floor-height 13)
(def window-width 10)

(def (skyline layer width height)
  "The buildings of a layer, near ones taller, as maps of :x, :width and :top."
  (loop [x -10 i 0 found []]
    (if (> x width)
      found
      (let [wide (+ 34 (* 60 (noise/of [layer i :wide])))
            tall (* height (+ 0.18 (* 0.12 layer) (* 0.32 (noise/of [layer i :tall]))))]
        (again (+ x wide 3) (inc i) (conj found {x: x width: wide top: (- height tall) id: i}))))))

(def (lit? layer building column row seconds)
  "Whether a window is lit now: each one keeps its own hours through the night."
  (let [hours (+ 6 (* 20 (noise/of [layer building column row :hours])))
        turn (int (/ (+ seconds (* hours (noise/of [layer building column row :shift]))) hours))]
    (> (noise/of [layer building column row turn]) 0.62)))

(def (building layer b seconds height)
  (let [shade (nth ["#0d1326" "#0a0f1f" "#05070f"] layer)
        columns (int (/ (- b.width 8) window-width))
        rows (int (/ (- height b.top 10) floor-height))
        glow (nth ["#5a6a8a" "#c9a65a" "#ffd88a"] layer)
        windows (for [column (range columns) row (range rows)
                      :when (lit? layer b.id column row seconds)]
                  [:rect (+ b.x 5 (* column window-width)) (+ b.top 8 (* row floor-height)) 5 7
                   [glow (+ 0.45 (* 0.2 layer))]])]
    (vec (concat [[:rect b.x b.top b.width (- height b.top) shade]] windows))))

(def (moon seconds width height)
  (let [x (* width (+ 0.15 (* 0.7 (mod (/ seconds 240.0) 1))))
        y (* height (+ 0.14 (* 0.06 (math/sin (/ seconds 40.0)))))]
    [[:circle x y 46 ["#fdf6e3" 0.05]]
     [:circle x y 30 ["#fdf6e3" 0.1]]
     [:circle x y 18 "#f7f1de"]]))

(def (stars width height)
  (vec (map (fn [i] [:circle (* width (noise/of [i :x])) (* height 0.55 (noise/of [i :y])) 0.8 ["#cfd8ff" 0.6]])
            (range 90))))

(def (draw frame)
  (let [seconds (/ frame.time 1000.0)]
    (vec (concat (stars frame.width frame.height)
                 (moon seconds frame.width frame.height)
                 (mapcat (fn [layer]
                           (mapcat (fn [b] (building layer b seconds frame.height))
                                   (skyline layer frame.width frame.height)))
                         [0 1 2])))))

(test "the skyline spans the screen"
  (let [buildings (skyline 1 800 600)]
    (expect (> (+ (get (last buildings) :x) (get (last buildings) :width)) 800))))

(test "windows go on and off through the night"
  (let [states (map (fn [hour] (lit? 2 3 1 4 (* hour 30))) (range 40))]
    (expect (and (some (fn [lit] lit) states) (some (fn [lit] (not lit)) states)))))
