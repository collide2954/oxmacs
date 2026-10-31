(module
  (import (ox math) (screensaver noise :as noise))
  (export background curtain draw))

(def background "#020611")
(def step 8)

(def ribbons
  [{color: "#5cffb0" base: 0.42 swell: 0.07 k1: 0.006 w1: 0.18 k2: 0.017 w2: 0.11 phase: 0.0}
   {color: "#3ce6c8" base: 0.5 swell: 0.05 k1: 0.008 w1: 0.13 k2: 0.013 w2: 0.21 phase: 1.7}
   {color: "#a77bff" base: 0.33 swell: 0.06 k1: 0.005 w1: 0.09 k2: 0.021 w2: 0.15 phase: 3.1}])

(def (curtain ribbon x seconds height)
  "Where a ribbon's lower edge is at x, how tall it hangs above it and how bright it is."
  (let [p ribbon.phase]
    {edge: (+ (* height ribbon.base)
              (* height ribbon.swell (math/sin (+ (* ribbon.k1 x) (* ribbon.w1 seconds) p)))
              (* height 0.5 ribbon.swell (math/sin (- (* ribbon.k2 x) (* ribbon.w2 seconds) p))))
     tall: (* height (+ 0.1 (* 0.16 (+ 0.5 (* 0.5 (math/sin (+ (* 0.011 x) (* 0.07 seconds) (* 2 p))))))))
     glow: (+ 0.5 (* 0.5 (math/sin (- (* 0.023 x) (* 0.33 seconds) p))))}))

(def (ribbon-lines ribbon seconds width height)
  (mapcat (fn [x]
            (let [c (curtain ribbon x seconds height)
                  top (- c.edge c.tall)
                  bright (- c.edge (* 0.25 c.tall))]
              [[:line x top x c.edge [ribbon.color (+ 0.04 (* 0.14 c.glow))] step]
               [:line x bright x c.edge [ribbon.color (* 0.32 c.glow)] step]]))
          (range 0 (+ width step) step)))

(def (stars seconds width height)
  (vec (map (fn [i]
              (let [twinkle (+ 0.3 (* 0.6 (+ 0.5 (* 0.5 (math/sin (+ (* seconds (+ 0.5 (noise/of [i :rate])))
                                                                     (* 7 (noise/of [i :phase]))))))))]
                [:circle (* width (noise/of [i :x])) (* height 0.8 (noise/of [i :y]))
                 (+ 0.5 (* 0.9 (noise/of [i :size]))) ["#dfe8ff" twinkle]]))
            (range 150))))

(def (glow width height)
  (vec (map (fn [band]
              [:rect 0 (* height (+ 0.45 (* 0.055 band))) width (+ 1 (* height 0.055)) ["#0c3b4f" (* 0.05 band)]])
            (range 10))))

(def (ridge x height)
  (- height (* height (+ 0.1 (* 0.05 (math/sin (* 0.004 x))) (* 0.03 (math/sin (+ 1 (* 0.013 x))))))))

(def (mountains width height)
  (vec (map (fn [x] [:line x (ridge x height) x height "#01030a" 7]) (range 0 (+ width 6) 6))))

(def (draw frame)
  (let [seconds (/ frame.time 1000.0)]
    (vec (concat (glow frame.width frame.height)
                 (stars seconds frame.width frame.height)
                 (mapcat (fn [ribbon] (ribbon-lines ribbon seconds frame.width frame.height)) ribbons)
                 (mountains frame.width frame.height)))))

(test "curtains wave as time passes"
  (let [ribbon (first ribbons)]
    (expect (not= (get (curtain ribbon 100 0 600) :edge) (get (curtain ribbon 100 5 600) :edge)))
    (expect (every? (fn [x] (< 0 (get (curtain ribbon x 3 600) :edge) 600)) (range 0 800 20)))))
