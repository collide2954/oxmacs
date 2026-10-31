(module
  (import (ox math) (screensaver noise :as noise) (screensaver outline :as outline))
  (export background landing draw))

(def background "#04101b")
(def gap 0.45)
(def life 4.0)
(def speed 70.0)

(def (landing i)
  "When drop i lands, in seconds."
  (* gap (+ i (* 0.8 (noise/of [i :when])))))

(def (rings i seconds width height)
  (let [age (- seconds (landing i))]
    (if (or (< age 0) (> age life))
      []
      (let [x (* width (noise/of [i :x]))
            y (* height (+ 0.2 (* 0.75 (noise/of [i :y]))))
            size (+ 0.6 (* 0.8 (noise/of [i :size])))
            fade (math/pow (- 1 (/ age life)) 1.6)
            waves (mapcat (fn [delay]
                            (let [radius (* (- age delay) speed size)]
                              (if (> radius 1)
                                (outline/ellipse x y radius (* radius 0.42)
                                                 ["#9fd3ff" (* 0.75 fade (- 1 delay))] 1.3 32)
                                [])))
                          [0 0.3 0.6])]
        (if (< age 0.2)
          (conj (vec waves) [:circle x y (* 4 size (- 1 (/ age 0.2))) ["#e8f6ff" 0.9]])
          waves)))))

(def (shimmer seconds width height)
  (vec (map (fn [k]
              (let [y (* height (+ 0.18 (* 0.8 (noise/of [k :y]))))
                    x (* width (noise/of [k :x]))
                    reach (* width (+ 0.03 (* 0.06 (noise/of [k :reach]))))
                    glint (* 0.12 (+ 1 (math/sin (+ (* seconds (+ 0.4 (noise/of [k :rate])))
                                                    (* 6 (noise/of [k :phase]))))))]
                [:line (- x reach) y (+ x reach) y ["#3b6f99" glint] 1]))
            (range 40))))

(def (draw frame)
  (let [seconds (/ frame.time 1000.0)
        oldest (max 0 (- (int (/ (- seconds life) gap)) 1))]
    (vec (concat (shimmer seconds frame.width frame.height)
                 (mapcat (fn [i] (rings i seconds frame.width frame.height))
                         (range oldest (+ 2 (int (/ seconds gap)))))))))

(test "drops keep landing, one after another"
  (expect (every? (fn [i] (< (landing i) (landing (+ i 2)))) (range 200))))

(test "a pond shows its rings after the first drops land"
  (expect (> (count (draw {time: 6000 width: 800 height: 600 font-size: 14})) 100)))
